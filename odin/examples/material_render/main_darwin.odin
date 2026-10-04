#+build darwin, arm64
//! Actual scene-owner material edits are accepted through native PBR pixels on either backend.
package main

import app "../../app"
import render "../../app/render"
import agent "../../agent"
import editor "../../editor"
import ecs "../../ecs"
import km "../../math"
import gfx "../../gfx"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import shader "../../gfx/shader"
import resources "../../resources"
import "core:path/filepath"
import NS "core:sys/darwin/Foundation"
import "core:mem"
import "core:fmt"
import "core:time"
import "core:os"
import "core:strings"
import "core:c"
import image "vendor:stb/image"

Capture_Ops :: struct($R:typeid) {
    source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),
    queue:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),
    poll:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error),
}
read_pixels :: proc(scene:^render.Native_Scene($R),capture:Capture_Ops(R),submission:gfx.Submission)->gfx.Readback_Data {
    source,error:=capture.source(scene.renderer,submission,scene.graph.color); assert(error==.None)
    ticket:gfx.Readback_Ticket; ticket,error=capture.queue(scene.renderer,source,{width=source.desc.width,height=source.desc.height,aspect=.Color,depth=1}); assert(error==.None)
    return poll_pixels(scene.renderer,capture,ticket,source)
}
poll_pixels :: proc(renderer:^$R,capture:Capture_Ops(R),ticket:gfx.Readback_Ticket,source:gfx.Texture_Source)->gfx.Readback_Data {
    start:=time.tick_now()
    for {
        pixels,ready,poll_error:=capture.poll(renderer,ticket); assert(poll_error==.None)
        if ready { assert(pixels.source==source && pixels.row_pitch==u64(source.desc.width)*4); return pixels }
        assert(time.tick_since(start)<10*time.Second,"native readback did not complete")
        time.sleep(time.Millisecond)
    }
}
render_frame :: proc(scene:^render.Native_Scene($R),frame:render.Frame_Data,objects:[]render.Object_Data,draws:[]gfx.Draw_Op)->(gfx.Submission,render.Native_Error) {
    token,error:=render.native_scene_acquire(scene); if error!=.None { return {},{gpu=error} }
    accepted:=false; defer { if !accepted { scene.operations.abort(scene.renderer,token) } }
    submission,render_error:=render.native_scene_render(scene,token,frame,objects,draws)
    accepted=render_error=={}; return submission,render_error
}
region_mean :: proc(pixels:^gfx.Readback_Data,x0,x1:u32)->[3]f64 {
    result:[3]f64; count:u64
    for y in pixels.region.height/4..<pixels.region.height*3/4 { for x in x0..<x1 {
        offset:=int(u64(y)*pixels.row_pitch+u64(x)*4)
        for channel in 0..<3 { source_channel:=2-channel if pixels.source.desc.format==.BGRA8_Unorm else channel; result[channel]+=f64(pixels.bytes[offset+source_channel]) }
        count+=1
    } }
    for &channel in result { channel/=f64(count) }
    return result
}
save_pixels :: proc(pixels:^gfx.Readback_Data,path:string) {
    bytes:=make([]byte,len(pixels.bytes)); defer delete(bytes); copy(bytes,pixels.bytes)
    if pixels.source.desc.format==.BGRA8_Unorm { for i:=0;i<len(bytes);i+=4 { bytes[i],bytes[i+2]=bytes[i+2],bytes[i] } }
    filename:=strings.clone_to_cstring(path); defer delete(filename)
    assert(image.write_png(filename,c.int(pixels.region.width),c.int(pixels.region.height),4,raw_data(bytes),c.int(pixels.row_pitch))!=0)
}
exercise :: proc(renderer:^$R,operations:render.GPU_Ops(R),capture:Capture_Ops(R),descriptor:gfx.Graphics_Desc,backend:string,output,resource_path:string) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    assert(app.authoring_services_init(&owner)==.None)
    assert(app.asset_resources_init(&owner,filepath.dir(resource_path),resource_path)==resources.Error.None)
    prepared_mesh,prepare_error:=app.scene_mesh_prepare(&owner,{kind=.Recipe,path="meshes/chair-frame.katmesh"}); assert(prepare_error==.None)
    sphere_descriptor:=transmute([]byte)string(`{"kind":"sphere","radius":0.5,"segments":48,"rings":24}`)
    sphere_mesh_a,sphere_error_a:=app.scene_mesh_prepare(&owner,{kind=.Geometry,geometry=sphere_descriptor}); assert(sphere_error_a==.None)
    sphere_mesh_b,sphere_error_b:=app.scene_mesh_prepare(&owner,{kind=.Geometry,geometry=sphere_descriptor}); assert(sphere_error_b==.None)
    red:=km.color_to_linear({0.85,0.12,0.08,1}); blue:=km.color_to_linear({0.08,0.2,0.85,1})
    ids:=[3]ecs.Entity_Id{
        ecs.spawn(&owner.world,struct { transform:app.Scene_Transform, surface:app.Surface_Material, mesh:app.Scene_Mesh }{{km.transform(position={-0.65,0,0},scale={1.4,1.4,1.4})},{red,true,0,0.7,1},sphere_mesh_a}),
        ecs.spawn(&owner.world,struct { transform:app.Scene_Transform, surface:app.Surface_Material, mesh:app.Scene_Mesh }{{km.transform(position={0.65,0,0},scale={1.4,1.4,1.4})},{blue,true,0,0.4,1},sphere_mesh_b}),
        ecs.spawn(&owner.world,struct { transform:app.Scene_Transform, surface:app.Surface_Material, mesh:app.Scene_Mesh }{{km.transform(position={0,-1.08,-0.55},rotation=km.quat_axis_angle(km.VEC3_Y,0.4),scale={0.65,0.65,0.65})},{km.color_to_linear({0.8,0.65,0.3,1}),true,0,0.6,1},prepared_mesh}),
    }
    consumer:render.Native_Consumer(R)
    initialization_error:=render.native_consumer_init(&consumer,&owner,renderer,operations,descriptor,3,384,256)
    fmt.println("Scene initialization:",initialization_error)
    assert(initialization_error=={})
    defer { assert(render.native_consumer_destroy(&consumer)==.None) }
    native,batch:=consumer.active,consumer.batch
    assert(len(batch.entries)==3)
    camera:=render.camera_default(); camera.position={0,0,3.4}
    frame,scene_error:=render.frame_data(camera,384,256,backend=="vulkan"); assert(scene_error==.None)
    draws:=batch.draws
    objects:=batch.objects
    before,error:=render_frame(native,frame,objects[:],draws[:]); assert(error=={})
    before_pixels:=read_pixels(native,capture,before); defer gfx.readback_data_destroy(&before_pixels)
    assert(render.native_scene_wait(native,before)==.None)
    left_before:=region_mean(&before_pixels,48,180); right_before:=region_mean(&before_pixels,204,336)
    fmt.println("Baseline region means:",left_before,right_before)
    save_pixels(&before_pixels,"/tmp/katla-material-baseline.png")
    assert(left_before[0]>left_before[2]+15 && right_before[2]>right_before[0]+15,"native scene did not render independent PBR colors")
    arguments:=fmt.aprintf(`{{"action":"set","entity_ids":["%d"],"base_color":[0.08,0.2,0.85,1],"metallic":0.7,"roughness":0.2}}`,u64(ids[0])); defer delete(arguments)
    ticket,call_error:=agent.submit_call(&owner.agent,{"native-surface", "material",transmute([]byte)arguments}); assert(ticket>0 && call_error==.None)
    assert(app.authoring_tick(&owner)==1)
    response,has_response:=editor.agent_take_result(&owner.agent); assert(has_response && response.ticket==ticket && response.result.error==.None); defer editor.agent_response_destroy(&response)
    assert(render.scene_batch_refresh(batch,&owner)=={})
    changed,render_error:=render_frame(native,frame,objects[:],draws[:]); assert(render_error=={})
    changed_pixels:=read_pixels(native,capture,changed); defer gfx.readback_data_destroy(&changed_pixels)
    assert(render.native_scene_wait(native,changed)==.None)
    left_after:=region_mean(&changed_pixels,48,180); right_after:=region_mean(&changed_pixels,204,336)
    assert(left_after[2]>left_after[0]+12 && right_before==right_after,"material edit failed to affect only its native object")
    assert(app.authoring_undo_last(&owner)==.None)
    assert(render.scene_batch_refresh(batch,&owner)=={})
    restored,restore_error:=render_frame(native,frame,objects[:],draws[:]); assert(restore_error=={})
    restored_pixels:=read_pixels(native,capture,restored); defer gfx.readback_data_destroy(&restored_pixels)
    assert(render.native_scene_wait(native,restored)==.None)
    assert(mem.compare(before_pixels.bytes,restored_pixels.bytes)==0,"undo failed to restore exact native PBR image")
    assert(app.authoring_redo_last(&owner)==.None)
    assert(render.scene_batch_refresh(batch,&owner)=={})
    redone,redo_error:=render_frame(native,frame,objects[:],draws[:]); assert(redo_error=={})
    redone_pixels:=read_pixels(native,capture,redone); defer gfx.readback_data_destroy(&redone_pixels)
    assert(render.native_scene_wait(native,redone)==.None)
    assert(mem.compare(changed_pixels.bytes,redone_pixels.bytes)==0,"redo failed to restore exact native PBR image")
    assert(app.authoring_undo_last(&owner)==.None)
    gesture:app.Material_Gesture; defer app.material_gesture_destroy(&gesture)
    actions_before:=len(owner.agent.session.actions)
    assert(app.material_gesture_begin(&owner,&gesture,ids[:1])==.None)
    for roughness in ([3]f32{0.8,0.4,0.1}) { assert(app.material_gesture_preview(&owner,&gesture,{.Roughness},{roughness=roughness})==.None) }
    assert(app.material_gesture_preview(&owner,&gesture,{.Base_Color},{base_color={0.12,0.82,0.2,1}})==.None)
    assert(len(owner.agent.session.actions)==actions_before)
    assert(app.material_gesture_finish(&owner,&gesture)==.None && len(owner.agent.session.actions)==actions_before+1)
    assert(render.scene_batch_refresh(batch,&owner)=={})
    gestured,gesture_error:=render_frame(native,frame,objects[:],draws[:]); assert(gesture_error=={})
    gesture_pixels:=read_pixels(native,capture,gestured); defer gfx.readback_data_destroy(&gesture_pixels)
    assert(render.native_scene_wait(native,gestured)==.None)
    left_gesture:=region_mean(&gesture_pixels,48,180)
    assert(left_gesture[1]>left_gesture[0]+15 && left_gesture[1]>left_gesture[2]+15)
    assert(app.authoring_undo_last(&owner)==.None)
    assert(render.scene_batch_refresh(batch,&owner)=={})
    gesture_undone,gesture_undo_error:=render_frame(native,frame,objects[:],draws[:]); assert(gesture_undo_error=={})
    gesture_undo_pixels:=read_pixels(native,capture,gesture_undone); defer gfx.readback_data_destroy(&gesture_undo_pixels)
    assert(render.native_scene_wait(native,gesture_undone)==.None)
    assert(mem.compare(before_pixels.bytes,gesture_undo_pixels.bytes)==0,"grouped gesture undo failed to restore first native image")
    assert(app.authoring_redo_last(&owner)==.None)
    assert(render.scene_batch_refresh(batch,&owner)=={})
    gesture_redone,gesture_redo_error:=render_frame(native,frame,objects[:],draws[:]); assert(gesture_redo_error=={})
    gesture_redo_pixels:=read_pixels(native,capture,gesture_redone); defer gfx.readback_data_destroy(&gesture_redo_pixels)
    assert(render.native_scene_wait(native,gesture_redone)==.None)
    assert(mem.compare(gesture_pixels.bytes,gesture_redo_pixels.bytes)==0,"grouped gesture redo failed to restore last native image")
    empty,empty_error:=render_frame(native,frame,nil,nil); assert(empty_error=={})
    empty_pixels:=read_pixels(native,capture,empty); defer gfx.readback_data_destroy(&empty_pixels)
    assert(render.native_scene_wait(native,empty)==.None)
    for offset:=0;offset<len(empty_pixels.bytes);offset+=4 { assert(mem.compare(empty_pixels.bytes[offset:offset+4],([]byte{9,10,13,255}))==0,"empty editor world did not clear its real color attachment") }
    resumed,resume_error:=render_frame(native,frame,objects[:],draws[:]); assert(resume_error=={})
    resumed_pixels:=read_pixels(native,capture,resumed); defer gfx.readback_data_destroy(&resumed_pixels)
    assert(render.native_scene_wait(native,resumed)==.None)
    assert(mem.compare(gesture_pixels.bytes,resumed_pixels.bytes)==0,"scene drawings did not resume after the empty-world access transition")
    previous_scene:=consumer.active
    consumer.operations.create_buffer=fail_upload
    instantiate_arguments:=transmute([]byte)string(`{"action":"instantiate","path":"resources/meshes/chair-frame.katmesh","position":[0,0,0]}`)
    rejected:=editor.agent_execute(&owner.agent.session,&owner.world,&owner.registry,{kind=.Application,tool_name="prefab",value=instantiate_arguments},app.authoring_executor(&owner))
    assert(rejected.result.error!=.None && owner.world.live_count==3 && consumer.active==previous_scene && consumer.last_error.gpu==.Allocation_Failed,"failed native upload partially published an asset")
    consumer.operations.create_buffer=operations.create_buffer
    after_failure,failure_render_error:=render_frame(native,frame,objects[:],draws[:]); assert(failure_render_error=={})
    after_failure_pixels:=read_pixels(native,capture,after_failure); defer gfx.readback_data_destroy(&after_failure_pixels)
    assert(render.native_scene_wait(native,after_failure)==.None)
    assert(mem.compare(gesture_pixels.bytes,after_failure_pixels.bytes)==0,"failed asset native staging changed old rendered pixels")
    accepted:=editor.agent_execute(&owner.agent.session,&owner.world,&owner.registry,{kind=.Application,tool_name="prefab",value=instantiate_arguments},app.authoring_executor(&owner))
    assert(accepted.result.error==.None && owner.world.live_count==4 && consumer.active!=previous_scene && len(consumer.batch.entries)==4)
    native,batch=consumer.active,consumer.batch
    uploaded,uploaded_error:=render_frame(native,frame,batch.objects,batch.draws); assert(uploaded_error=={})
    uploaded_pixels:=read_pixels(native,capture,uploaded); defer gfx.readback_data_destroy(&uploaded_pixels)
    assert(render.native_scene_wait(native,uploaded)==.None)
    assert(mem.compare(gesture_pixels.bytes,uploaded_pixels.bytes)!=0,"successful native asset publication did not render its actual geometry")
    assert(app.authoring_undo_last(&owner)==.None && render.native_consumer_refresh(&consumer)=={})
    native,batch=consumer.active,consumer.batch
    asset_undone,asset_undo_error:=render_frame(native,frame,batch.objects,batch.draws); assert(asset_undo_error=={})
    asset_undo_pixels:=read_pixels(native,capture,asset_undone); defer gfx.readback_data_destroy(&asset_undo_pixels)
    assert(render.native_scene_wait(native,asset_undone)==.None)
    assert(mem.compare(gesture_pixels.bytes,asset_undo_pixels.bytes)==0,"asset undo did not restore old native geometry pixels")
    exercise_asset_documents(&consumer,&owner,capture,frame,resource_path)
    save_pixels(&changed_pixels,output)
    fmt.printf("%s real PBR: left %.1f/%.1f/%.1f -> %.1f/%.1f/%.1f; right unchanged, agent/gesture undo+redo restored every pixel; %s\n",backend,left_before[0],left_before[1],left_before[2],left_after[0],left_after[1],left_after[2],output)
}
fail_upload :: proc(renderer:^$R,descriptor:gfx.Buffer_Desc,bytes:[]byte)->(gfx.Buffer_Handle,gfx.Gpu_Error) { return {},.Allocation_Failed }
main :: proc() {
    assert(len(os.args)>=5,"usage: material_render <naga-library> <metal|vulkan|metal-window|vulkan-window|metal-editor|vulkan-editor> <png-path> <resource-root> [vulkan-library]")
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0,"app consumer leaked Odin allocations"); mem.tracking_allocator_destroy(&tracker) }
    context.allocator=mem.tracking_allocator(&tracker)
    _=NS.scoped_autoreleasepool()
    compiler:shader.Compiler; assert(shader.compiler_init(&compiler,os.args[1])==.None); defer { assert(shader.compiler_destroy(&compiler)==.None) }
    interactive:=strings.has_suffix(os.args[2],"-editor")
    windowed:=strings.has_suffix(os.args[2],"-window") || interactive
    format:=gfx.Texture_Format.BGRA8_Unorm if windowed else .RGBA8_Unorm
    surface,compile_error:=render.surface_shader_compile(&compiler,format); assert(compile_error==.None); defer render.surface_shader_destroy(&surface)
    switch os.args[2] {
    case "metal","metal-window","metal-editor","metal-models":
        renderer:metal.Renderer; assert(metal.renderer_init(&renderer)==.None); defer { assert(metal.renderer_destroy(&renderer)==.None) }
        operations:=render.GPU_Ops(metal.Renderer){metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.create_texture,metal.destroy_texture,metal.acquire,metal.abort,metal.submit,metal.wait,metal.release_graph_exports}
        capture:=Capture_Ops(metal.Renderer){metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback}
        if os.args[2]=="metal-models" { exercise_models(&renderer,operations,capture,surface.descriptor,&compiler,render.Model_GPU_Ops(metal.Renderer){operations,metal.create_sampler,metal.destroy_sampler},"metal",os.args[3],os.args[4]) }
        else if windowed {
            surfaces:=Surface_Ops(metal.Renderer){metal.attach_surface,metal.resize_surface,metal.detach_surface,metal.acquire_surface,metal.abort_surface,metal.present_surface}
            if interactive { exercise_editor(&renderer,operations,capture,surfaces,surface.descriptor,&compiler,render.Model_GPU_Ops(metal.Renderer){operations,metal.create_sampler,metal.destroy_sampler},render.Particle_GPU_Ops(metal.Renderer){metal.create_pipeline,metal.destroy_pipeline,metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.read_buffer},"metal",os.args[3],os.args[4],metal_control_event,metal_assistant_event) } else { exercise_window(&renderer,operations,capture,surfaces,surface.descriptor,"metal",os.args[3]) }
        } else { exercise(&renderer,operations,capture,surface.descriptor,"metal",os.args[3],os.args[4]) }
    case "vulkan","vulkan-window","vulkan-editor","vulkan-models":
        loader:=""; if len(os.args)>5 { loader=os.args[5] }
        renderer:vulkan.Renderer; assert(vulkan.renderer_init(&renderer,validation=true,loader_path=loader)==.None); defer { assert(renderer.validation_errors==0); assert(vulkan.renderer_destroy(&renderer)==.None) }
        operations:=render.GPU_Ops(vulkan.Renderer){vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.create_texture,vulkan.destroy_texture,vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,vulkan.release_graph_exports}
        capture:=Capture_Ops(vulkan.Renderer){vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback}
        if os.args[2]=="vulkan-models" { exercise_models(&renderer,operations,capture,surface.descriptor,&compiler,render.Model_GPU_Ops(vulkan.Renderer){operations,vulkan.create_sampler,vulkan.destroy_sampler},"vulkan",os.args[3],os.args[4]) }
        else if windowed {
            surfaces:=Surface_Ops(vulkan.Renderer){vulkan.attach_surface,vulkan.resize_surface,vulkan.detach_surface,vulkan.acquire_surface,vulkan.abort_surface,vulkan.present_surface}
            if interactive { exercise_editor(&renderer,operations,capture,surfaces,surface.descriptor,&compiler,render.Model_GPU_Ops(vulkan.Renderer){operations,vulkan.create_sampler,vulkan.destroy_sampler},render.Particle_GPU_Ops(vulkan.Renderer){vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.read_buffer},"vulkan",os.args[3],os.args[4],vulkan_control_event,vulkan_assistant_event) } else { exercise_window(&renderer,operations,capture,surfaces,surface.descriptor,"vulkan",os.args[3]) }
        } else { exercise(&renderer,operations,capture,surface.descriptor,"vulkan",os.args[3],os.args[4]) }
    case: assert(false,"unsupported backend")
    }
}
