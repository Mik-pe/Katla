#+build darwin,arm64
//! Real model pixels and exact native export ownership survive transactional resize rejection.
package main
import app "../../app"
import render "../../app/render"
import gfx "../../gfx"
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import shader "../../gfx/shader"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import NS "core:sys/darwin/Foundation"
import "core:os"
import "core:fmt"
import "core:mem"
import "core:time"
import "core:path/filepath"

texture_budget:int=-1
sampler_budget:int=-1
reject_exports:bool
track_candidates:bool
candidate_handles:[dynamic]gfx.Texture_Handle
fault_texture :: proc($R:typeid)->proc(^R,gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error) {
    return proc(renderer:^R,descriptor:gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error) {
        if texture_budget==0 { return {},.Allocation_Failed }
        if texture_budget>0 { texture_budget-=1 }
        handle:gfx.Texture_Handle; error:gfx.Gpu_Error
        when R==metal.Renderer { handle,error=metal.create_texture(renderer,descriptor) }
        else { handle,error=vulkan.create_texture(renderer,descriptor) }
        if error==.None && track_candidates { append(&candidate_handles,handle) }
        return handle,error
    }
}
fault_sampler :: proc($R:typeid)->proc(^R,gfx.Sampler_Desc)->(gfx.Sampler_Handle,gfx.Gpu_Error) {
    return proc(renderer:^R,descriptor:gfx.Sampler_Desc)->(gfx.Sampler_Handle,gfx.Gpu_Error) {
        if sampler_budget==0 { return {},.Allocation_Failed }; if sampler_budget>0 { sampler_budget-=1 }
        when R==metal.Renderer { return metal.create_sampler(renderer,descriptor) }
        else { return vulkan.create_sampler(renderer,descriptor) }
    }
}
fault_destroy_texture :: proc($R:typeid)->proc(^R,gfx.Texture_Handle)->gfx.Gpu_Error {
    return proc(renderer:^R,handle:gfx.Texture_Handle)->gfx.Gpu_Error {
        error:gfx.Gpu_Error
        when R==metal.Renderer { error=metal.destroy_texture(renderer,handle) }
        else { error=vulkan.destroy_texture(renderer,handle) }
        if error==.None && track_candidates {
            for candidate,i in candidate_handles { if candidate==handle { ordered_remove(&candidate_handles,i); break } }
        }
        return error
    }
}
fault_exports :: proc($R:typeid)->proc(^R,^gfx.Graph)->gfx.Gpu_Error {
    return proc(renderer:^R,graph:^gfx.Graph)->gfx.Gpu_Error {
        if reject_exports { return .Native_Failure }
        when R==metal.Renderer { return metal.release_graph_exports(renderer,graph) }
        else { return vulkan.release_graph_exports(renderer,graph) }
    }
}
Capture_Ops :: struct($R:typeid) {
    source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),
    queue:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),
    poll:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error),
}
pixels :: proc(renderer:^$R,operations:Capture_Ops(R),ticket:gfx.Readback_Ticket)->gfx.Readback_Data {
    started:=time.tick_now()
    for {
        data,done,error:=operations.poll(renderer,ticket); assert(error==.None)
        if done { return data }
        assert(time.tick_since(started)<10*time.Second); time.sleep(time.Millisecond)
    }
}
render_frame :: proc(scene:^render.Native_Scene($R),frame:render.Frame_Data)->gfx.Submission {
    token,error:=render.native_scene_acquire(scene); assert(error==.None)
    submission,submit_error:=render.native_scene_render(scene,token,frame,nil,nil); assert(submit_error=={})
    assert(render.native_scene_wait(scene,submission)==.None); return submission
}
Reload_Host :: struct($R:typeid) { consumers:^[4]render.Native_Consumer(R) }
Reload_Batch :: struct { tokens:[4]rawptr }
host_prepare :: proc($R:typeid)->proc(rawptr,^app.Authoring,[]ecs.Entity_Id,app.Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    return proc(state:rawptr,owner:^app.Authoring,ids:[]ecs.Entity_Id,mode:app.Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
        host:=cast(^Reload_Host(R))state; batch:=new(Reload_Batch)
        for &consumer,index in host.consumers^ {
            token,error:=render.native_consumer_prepare(&consumer,owner,ids,mode)
            if error!=.None { for i in 0..<index { render.native_consumer_finish(&host.consumers[i],batch.tokens[i],false) }; free(batch); return nil,error }
            batch.tokens[index]=token
        }; return batch,.None
    }
}
host_finish :: proc($R:typeid)->proc(rawptr,rawptr,bool) {
    return proc(state,token:rawptr,commit:bool) {
        host:=cast(^Reload_Host(R))state; batch:=cast(^Reload_Batch)token
        for &consumer,index in host.consumers^ { render.native_consumer_finish(&consumer,batch.tokens[index],commit) }; free(batch)
    }
}
exercise :: proc(renderer:^$R,operations:render.GPU_Ops(R),capture:Capture_Ops(R),compiler:^shader.Compiler,descriptor:render.Scene_Pipelines,model_ops:render.Model_GPU_Ops(R),backend,resource_root:string) {
    texture_budget=-1
    directory,error:=os.mkdir_temp("","katla-texture-reload",context.allocator); assert(error==nil); defer { assert(os.remove_all(directory)==nil); delete(directory) }
    model_path,_:=filepath.join({directory,"Live.gltf"}); defer delete(model_path)
    image_path,_:=filepath.join({directory,"live.png"}); defer delete(image_path)
    assert(os.write_entire_file(model_path,#load("fixtures/Live.gltf",[]byte))==nil)
    red:=#load("fixtures/red.png",[]byte); green:=#load("fixtures/green.png",[]byte); blue:=#load("fixtures/blue.png",[]byte)
    assert(os.write_entire_file(image_path,red)==nil)
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    assert(app.authoring_services_init(&owner)==.None)
    source,load_error:=app.scene_model_prepare(&owner,{path=model_path,root=.File}); assert(load_error==.None)
    entity:=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform,surface:app.Surface_Material}{source,{km.TRANSFORM_IDENTITY},{metallic=1,roughness=1,ao=1}})
    authored:app.Material_Gesture; assert(app.material_gesture_begin(&owner,&authored,{entity})==.None)
    assert(app.material_gesture_preview(&owner,&authored,{.Roughness},{roughness=0.75})==.None)
    assert(app.material_gesture_finish(&owner,&authored)==.None); app.material_gesture_destroy(&authored)
    assert(len(owner.agent.session.actions)==1)
    source_images:=raw_data(ecs.get_component_mut(&owner.world,entity,app.Scene_Model).model.images)
    compiled,compile_error:=render.model_shader_compile(compiler); assert(compile_error==.None); defer render.model_shader_destroy(&compiled)
    config:=render.Model_Config(R){&compiled,model_ops}
    consumers:[4]render.Native_Consumer(R)
    defer { for &consumer in consumers { assert(render.native_consumer_destroy(&consumer)==.None) } }
    caches:[4]^render.Native_Model(R)
    camera:=render.camera_default(); camera.position={0,0,3}; camera.target={0,0,0}
    frame,frame_error:=render.frame_data(camera,96,64,backend=="Vulkan1.3"); assert(frame_error==.None)
    handles:[4]gfx.Texture_Handle; revisions:[4]u64; image_counts:[4]int
    for &consumer,i in consumers {
        assert(render.native_consumer_init(&consumer,&owner,renderer,operations,descriptor,3,96,64,models=&config,install_participant=false)=={})
        consumer.active.feature_settings.sky=false; consumer.active.feature_settings.grid=false; consumer.active.feature_settings.postprocess={1,.Linear}
        caches[i]=consumer.active.models; handles[i]=caches[i].textures[0].native.texture
        revisions[i]=consumer.active.graph.graph.revision; image_counts[i]=len(consumer.active.graph.graph.images)
    }
    for fault in 0..<4 {
        for cache,i in caches { revisions[i]=cache.graph.graph.revision }
        if fault==0 { assert(os.write_entire_file(image_path,[]byte{1,2,3})==nil) }
        else if fault<3 { assert(os.write_entire_file(image_path,green)==nil); texture_budget=0 if fault==1 else 1 }
        else { assert(os.write_entire_file(image_path,blue)==nil); sampler_budget=0 }
        track_candidates=true
        receipt,rejected:=render.model_texture_reload(&owner,caches[:]); assert(rejected!={} && receipt=={})
        track_candidates=false; assert(len(candidate_handles)==0); texture_budget=-1; sampler_budget=-1
        for cache,i in caches { assert(cache.textures[0].native.texture==handles[i] && cache.graph.graph.revision==revisions[i]) }
        assert(raw_data(ecs.get_component_mut(&owner.world,entity,app.Scene_Model).model.images)==source_images)
        for &consumer in consumers {
            submission:=render_frame(consumer.active,frame)
            source,source_error:=capture.source(renderer,submission,consumer.active.graph.output); assert(source_error==.None)
            ticket,queue_error:=capture.queue(renderer,source,{width=96,height=64,depth=1,aspect=.Color}); assert(queue_error==.None)
            data:=pixels(renderer,capture,ticket); offset:=32*data.row_pitch+48*4
            assert(data.bytes[offset]==255 && data.bytes[offset+1]==0 && data.bytes[offset+2]==0); gfx.readback_data_destroy(&data)
        }
    }
    for encoded,index in ([][]byte{green,blue,red,blue,green,red}) {
        assert(os.write_entire_file(image_path,encoded)==nil)
        receipt,reload_error:=render.model_texture_reload(&owner,caches[:]); assert(reload_error=={} && receipt.textures==4 && receipt.caches==4 && receipt.cleanup==.None)
        unchanged,no_change_error:=render.model_texture_reload(&owner,caches[:]); assert(no_change_error=={} && unchanged.textures==0)
        for &consumer,i in consumers {
            assert(len(consumer.active.graph.graph.images)==image_counts[i])
            submission:=render_frame(consumer.active,frame)
            source,source_error:=capture.source(renderer,submission,consumer.active.graph.output); assert(source_error==.None)
            ticket,queue_error:=capture.queue(renderer,source,{width=96,height=64,depth=1,aspect=.Color}); assert(queue_error==.None)
            data:=pixels(renderer,capture,ticket); offset:=32*data.row_pitch+48*4
            channel:=1 if index==0 || index==4 else (2 if index==1 || index==3 else 0)
            for c in 0..<3 { assert(data.bytes[offset+u64(c)]==(255 if c==channel else 0)) }; gfx.readback_data_destroy(&data)
        }
    }
    assert(raw_data(ecs.get_component_mut(&owner.world,entity,app.Scene_Model).model.images)==source_images)
    assert(len(owner.agent.session.actions)==1 && len(owner.agent.session.redo_actions)==0)
    assert(os.write_entire_file(image_path,green)==nil)
    green_receipt,green_error:=render.model_texture_reload(&owner,caches[:]); assert(green_error=={} && green_receipt.textures==4)
    host:=Reload_Host(R){&consumers}
    ecs.insert_resource(&owner.world,app.Scene_Participant{&host,host_prepare(R),host_finish(R)})
    defer ecs.remove_resource(&owner.world,app.Scene_Participant)
    operation:=editor.Scene_Op{kind=.Set_Field,entity=entity,component="SceneTransform",field="local",value=transmute([]byte)string(`{"position":[0.25,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`)}
    result,command:=app.scene_action_execute(&owner,operation); assert(result.error==.None)
    editor.agent_record_action(&owner.agent.session,operation,&result,&command); editor.tool_result_destroy(&result)
    assert(len(owner.agent.session.actions)==2)
    for step in 0..<3 {
        if step==1 { assert(app.authoring_undo_last(&owner)==.None) }
        if step==2 { assert(app.authoring_redo_last(&owner)==.None) }
        for &consumer,i in consumers {
            assert(render.native_consumer_refresh(&consumer)=={})
            caches[i]=consumer.active.models
            submission:=render_frame(consumer.active,frame)
            current,source_error:=capture.source(renderer,submission,consumer.active.graph.output); assert(source_error==.None)
            ticket,queue_error:=capture.queue(renderer,current,{width=96,height=64,depth=1,aspect=.Color}); assert(queue_error==.None)
            data:=pixels(renderer,capture,ticket); offset:=32*data.row_pitch+48*4
            assert(data.bytes[offset]==0 && data.bytes[offset+1]==255 && data.bytes[offset+2]==0,"unrelated native model staging republished stale red CPU image bytes")
            gfx.readback_data_destroy(&data)
        }
        unchanged,no_change_error:=render.model_texture_reload(&owner,caches[:]); assert(no_change_error=={} && unchanged.textures==0)
    }
    assert(len(owner.agent.session.actions)==2 && len(owner.agent.session.redo_actions)==0)
    service:render.Texture_Reload_Service
    polled,poll_error:=render.model_texture_reload_poll(&service,&owner,caches[:]); assert(poll_error=={} && polled.textures==0)
    fmt.println("Texture live reload PASS",backend,"four caches, decode/native/partial-batch rollback, resized mip chains, unchanged history, exact red/green/blue pixels; fresh green survives actual Transform Edit/Undo/Redo native model rebuild")
}
main :: proc() {
    assert(len(os.args)==4,"usage: texture_reload <naga-library> <vulkan-loader> <resource-root>")
    _=NS.scoped_autoreleasepool()
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0); mem.tracking_allocator_destroy(&tracker) }; context.allocator=mem.tracking_allocator(&tracker)
    candidate_handles=make([dynamic]gfx.Texture_Handle); defer delete(candidate_handles)
    compiler:shader.Compiler; assert(shader.compiler_init(&compiler,os.args[1])==.None); defer { assert(shader.compiler_destroy(&compiler)==.None) }
    surface,error:=render.surface_shader_compile(&compiler,.RGBA8_Unorm); assert(error==.None); defer render.surface_shader_destroy(&surface)
    {
        renderer:metal.Renderer; assert(metal.renderer_init(&renderer)==.None); defer { assert(metal.renderer_destroy(&renderer)==.None) }
        operations:=render.GPU_Ops(metal.Renderer){metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,fault_texture(metal.Renderer),fault_destroy_texture(metal.Renderer),metal.acquire,metal.abort,metal.submit,metal.wait,fault_exports(metal.Renderer),metal.create_pipeline,metal.destroy_pipeline,metal.create_sampler,metal.destroy_sampler}
        capture:=Capture_Ops(metal.Renderer){metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback}
        exercise(&renderer,operations,capture,&compiler,render.surface_pipelines(&surface),render.Model_GPU_Ops(metal.Renderer){operations,fault_sampler(metal.Renderer),metal.destroy_sampler},"Metal4",os.args[3])
    }
    {
        renderer:vulkan.Renderer; assert(vulkan.renderer_init(&renderer,validation=true,loader_path=os.args[2])==.None); defer { assert(vulkan.validation_error_count(&renderer)==0); assert(vulkan.renderer_destroy(&renderer)==.None) }
        operations:=render.GPU_Ops(vulkan.Renderer){vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,fault_texture(vulkan.Renderer),fault_destroy_texture(vulkan.Renderer),vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,fault_exports(vulkan.Renderer),vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_sampler,vulkan.destroy_sampler}
        capture:=Capture_Ops(vulkan.Renderer){vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback}
        exercise(&renderer,operations,capture,&compiler,render.surface_pipelines(&surface),render.Model_GPU_Ops(vulkan.Renderer){operations,fault_sampler(vulkan.Renderer),vulkan.destroy_sampler},"Vulkan1.3",os.args[3])
    }
}
