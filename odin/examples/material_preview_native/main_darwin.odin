#+build darwin,arm64
//! Accepted material source previews survive scene replacement and failed atomic publication.
package main
import render "../../app/render"
import gfx "../../gfx"
import ui "../../ui"
import resources "../../resources"
import app "../../app"
import editor "../../editor"
import ecs "../../ecs"
import km "../../math"
import shader "../../gfx/shader"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import NS "core:sys/darwin/Foundation"
import "core:os"
import "core:mem"
import "core:fmt"
import "core:time"
import "core:path/filepath"

API :: struct($R:typeid) {
    gpu:render.GPU_Ops(R),ui:render.UI_GPU_Ops(R),acquire:proc(^R)->(gfx.Frame_Token,gfx.Gpu_Error),
    submit:proc(^R,gfx.Frame_Token,^gfx.Graph,^gfx.Compiled_Graph,[]gfx.Buffer_Input,[]gfx.Texture_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error),
    wait:proc(^R,gfx.Submission)->gfx.Gpu_Error,release:proc(^R,^gfx.Graph)->gfx.Gpu_Error,
    source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),
    queue:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),
    poll:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error),
}
capture :: proc(renderer:^$R,api:API(R),owner:^render.UI_GPU(R),fonts:^render.UI_Font_System,cache:^render.Material_Preview_Cache(R),ids:[5]ui.Texture_Id,down:bool,release_before_wait:bool)->gfx.Readback_Data {
    clip:=ui.Rect{0,0,96,16}
    commands:=make([]ui.Draw_Command,5); defer delete(commands)
    for id,index in ids { assert(id!=0);commands[index]=ui.Image_Draw{texture=id,bounds={f32(index*16),0,16,16},uv={0,0,1,1},clip=clip,tint={1,1,1,1}} }
    mesh,mesh_error:=render.ui_prepare(fonts,{commands=commands,logical_size={96,16},pixel_scale=1}); assert(mesh_error==.None); defer render.ui_mesh_destroy(&mesh)
    prepared,prepare_error:=render.ui_gpu_prepare(owner,fonts,&mesh); assert(prepare_error==.None)
    graph:gfx.Graph; gfx.graph_init(&graph);defer gfx.graph_destroy(&graph)
    desc:=gfx.Texture_Desc{width=96,height=16,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Color_Attachment,.Transfer_Source}}
    target,target_error:=gfx.graph_image(&graph,desc,{initial=.Undefined,final=.Transfer_Source},false,true); assert(target_error==.None)
    handle,handle_error:=owner.ops.create_target(renderer,desc);assert(handle_error==.None)
    input,input_error:=render.ui_graph_append(&graph,owner,&prepared,&mesh,target,desc,true,down);assert(input_error=={});defer render.ui_graph_input_destroy(&input)
    textures:=make([dynamic]gfx.Texture_Input);append(&textures,gfx.Texture_Input{target,handle});append(&textures,..input.textures);defer delete(textures)
    plan,plan_error:=gfx.graph_compile(&graph);assert(plan_error==.None);defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=api.acquire(renderer);assert(acquire_error==.None)
    submission,submit_error,packet_error:=api.submit(renderer,token,&graph,&plan,input.buffers,textures[:]);assert(submit_error==.None&&packet_error==.None)
    source,source_error:=api.source(renderer,submission,target);assert(source_error==.None)
    ticket,ticket_error:=api.queue(renderer,source,{width=96,height=16,depth=1,aspect=.Color});assert(ticket_error==.None)
    assert(render.ui_gpu_frame_destroy(owner,&prepared)==.None)
    assert(owner.ops.destroy_texture(renderer,handle)==.None)
    assert(api.release(renderer,&graph)==.None)
    if release_before_wait { assert(render.material_preview_destroy(cache)=={});assert(len(owner.textures)==0) }
    assert(api.wait(renderer,submission)==.None)
    start:=time.tick_now();data:gfx.Readback_Data
    for { ready:bool;error:gfx.Gpu_Error;data,ready,error=api.poll(renderer,ticket);assert(error==.None);if ready { break };assert(time.tick_since(start)<10*time.Second);time.sleep(time.Millisecond) }
    assert(len(data.bytes)>=96*16*4)
    return data
}
write_preview_source :: proc(path:string) {
    bytes:[58]byte;header:=[]byte{0x42,0x4d,58,0,0,0,0,0,0,0,54,0,0,0,40,0,0,0,1,0,0,0,1,0,0,0,1,0,24,0,0,0,0,0,4,0,0,0};copy(bytes[:],header);bytes[55]=255
    assert(os.write_entire_file(path,bytes[:])==nil)
}
fail_after:int= -1
create_target :: proc($R:typeid)->proc(^R,gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error) {
    return proc(renderer:^R,desc:gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error) {
        if fail_after==0 { return {},.Allocation_Failed };if fail_after>0 { fail_after-=1 }
        when R==metal.Renderer { return metal.create_texture(renderer,desc) } else { return vulkan.create_texture(renderer,desc) }
    }
}
exercise :: proc(renderer:^$R,api:API(R),compiler:^shader.Compiler,fonts:^render.UI_Font_System,down:bool,name,resource_path:string) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner)
    assert(app.authoring_services_init(&owner)==.None)
    directory,error:=os.mkdir_temp("","katla-material-preview",context.allocator);assert(error==nil);defer { assert(os.remove_all(directory)==nil);delete(directory) }
    target,_:=filepath.join({directory,"resources"});defer delete(target);assert(os.copy_directory_all(target,resource_path)==nil)
    assert(app.asset_resources_init(&owner,directory,target)==resources.Error.None)
    source,source_error:=app.scene_model_prepare(&owner,{path="models/Box.gltf"});assert(source_error==.None)
    entity:=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform,surface:app.Surface_Material}{source,{km.TRANSFORM_IDENTITY},{metallic=1,roughness=1,ao=1}})
    material,material_error:=render.model_shader_compile(compiler);assert(material_error==.None);defer render.model_shader_destroy(&material)
    config:=render.Model_Config(R){shader=&material,operations={gpu=api.gpu,create_sampler=api.ui.create_sampler,destroy_sampler=api.ui.destroy_sampler}}
    accepted:render.Native_Model(R);assert(render.model_native_init(&accepted,&owner,{entity},renderer,config,3)=={});defer assert(render.model_native_destroy(&accepted)==.None)
    compiled,compile_error:=render.ui_shader_compile(compiler,.RGBA8_Unorm);assert(compile_error==.None);defer render.ui_shader_destroy(&compiled)
    gpu:render.UI_GPU(R);assert(render.ui_gpu_init(&gpu,renderer,api.ui,&compiled)==.None);defer assert(render.ui_gpu_destroy(&gpu)==.None)
    preview:render.Material_Preview_Cache(R);assert(render.material_preview_init(&preview,&gpu,api.gpu)=={});defer assert(render.material_preview_destroy(&preview)=={})
    first:=render.material_preview_update(&preview,&accepted,&owner,entity);assert(first.error=={}&&first.ready&&first.changed==5)
    baseline:=capture(renderer,api,&gpu,fonts,&preview,first.textures,down,false);defer gfx.readback_data_destroy(&baseline)
    unchanged:=render.material_preview_update(&preview,&accepted,&owner,entity);assert(unchanged.error=={}&&unchanged.changed==0&&unchanged.textures==first.textures)
    replacement,read_error:=app.scene_model_prepare(&owner,{path="models/DamagedHelmet.glb"});assert(read_error==.None)
    ecs.remove_component(&owner.world,entity,app.Scene_Model);ecs.add_component(&owner.world,entity,replacement)
    next:render.Native_Model(R);assert(render.model_native_init(&next,&owner,{entity},renderer,config,3)=={});defer assert(render.model_native_destroy(&next)==.None)
    preview.operations.create_texture=create_target(R);fail_after=1
    rejected:=render.material_preview_update(&preview,&next,&owner,entity);fail_after= -1
    assert(rejected.error.gpu==.Allocation_Failed&&rejected.ready&&rejected.textures==first.textures&&rejected.changed==0)
    preserved:=capture(renderer,api,&gpu,fonts,&preview,rejected.textures,down,false);defer gfx.readback_data_destroy(&preserved);assert(mem.compare(baseline.bytes,preserved.bytes)==0)
    repaired:=render.material_preview_update(&preview,&next,&owner,entity);assert(repaired.error=={}&&repaired.ready&&repaired.changed>0)
    filename,_:=filepath.join({target,"preview.bmp"});defer delete(filename);write_preview_source(filename)
    assigned,assignment:=app.material_texture_execute(&owner,{entities={entity},role=.Albedo,source={kind=.File,path="preview.bmp",root=.Resource}});defer editor.tool_result_destroy(&assigned);defer editor.undo_group_destroy(&assignment);assert(assigned.error==.None)
    selected,selection:=app.material_texture_execute(&owner,{entities={entity},role=.Emission,source={kind=.Gltf_Image,path="models/DamagedHelmet.glb",root=.Resource,image_index=0}});defer editor.tool_result_destroy(&selected);defer editor.undo_group_destroy(&selection);assert(selected.error==.None)
    explicit:render.Native_Model(R);assert(render.model_native_init(&explicit,&owner,{entity},renderer,config,3)=={});defer assert(render.model_native_destroy(&explicit)==.None)
    published:=render.material_preview_update(&preview,&explicit,&owner,entity);assert(published.error=={}&&published.ready&&published.changed>=2)
    assert(render.model_native_destroy(&accepted)==.None&&render.model_native_destroy(&next)==.None&&render.model_native_destroy(&explicit)==.None)
    after:=capture(renderer,api,&gpu,fonts,&preview,published.textures,down,true)
    for y in 0..<16 { for x in 0..<16 { offset:=int(after.row_pitch)*y+x*4;assert(mem.compare(after.bytes[offset:offset+4],([]byte{0,255,0,255}))==0) } }defer gfx.readback_data_destroy(&after);assert(mem.compare(after.bytes,baseline.bytes)!=0)
    fmt.println("Native material previews PASS",name,"accepted inherited + exact neutral + confined file/glTF-image sources, all five independent UI images, same-frame source owner removal, partial native failure preserves every pixel/ID, repair and accepted UI retention after preview removal")
}
main :: proc() {
    assert(len(os.args)==6,"Pass offline compiler,font library,resources,Vulkan loader,--metal/--vulkan/--both")
    backing:=context.allocator;tracker:mem.Tracking_Allocator;mem.tracking_allocator_init(&tracker,backing);context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing;assert(len(tracker.allocation_map)==0&&len(tracker.bad_free_array)==0);mem.tracking_allocator_destroy(&tracker) }
    pool:=NS.AutoreleasePool.alloc()->init();defer pool->drain()
    compiler:shader.Compiler;assert(shader.compiler_init(&compiler,os.args[1])==.None);defer assert(shader.compiler_destroy(&compiler)==.None)
    fonts:render.UI_Font_System;assert(render.ui_font_init(&fonts,os.args[2],os.args[3])==.None);defer render.ui_font_destroy(&fonts)
    backend:=os.args[5];assert(backend=="--metal"||backend=="--vulkan"||backend=="--both")
    if backend=="--metal"||backend=="--both" {
    m:metal.Renderer;assert(metal.renderer_init(&m)==.None)
    ma:=API(metal.Renderer){gpu={metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.create_texture,metal.destroy_texture,metal.acquire,metal.abort,metal.submit,metal.wait,metal.release_graph_exports,metal.create_pipeline,metal.destroy_pipeline,metal.create_sampler,metal.destroy_sampler},ui={metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.create_texture_with_data,metal.destroy_texture,metal.create_sampler,metal.destroy_sampler,metal.create_texture},acquire=metal.acquire,submit=metal.submit,wait=metal.wait,release=metal.release_graph_exports,source=metal.graph_texture_source,queue=metal.queue_texture_readback,poll=metal.poll_texture_readback}
    exercise(&m,ma,&compiler,&fonts,false,"Metal",os.args[3]);assert(metal.renderer_destroy(&m)==.None)
    }
    if backend=="--vulkan"||backend=="--both" {
    v:vulkan.Renderer;assert(vulkan.renderer_init(&v,validation=true,loader_path=os.args[4])==.None)
    va:=API(vulkan.Renderer){gpu={vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.create_texture,vulkan.destroy_texture,vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,vulkan.release_graph_exports,vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_sampler,vulkan.destroy_sampler},ui={vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.create_texture_with_data,vulkan.destroy_texture,vulkan.create_sampler,vulkan.destroy_sampler,vulkan.create_texture},acquire=vulkan.acquire,submit=vulkan.submit,wait=vulkan.wait,release=vulkan.release_graph_exports,source=vulkan.graph_texture_source,queue=vulkan.queue_texture_readback,poll=vulkan.poll_texture_readback}
    exercise(&v,va,&compiler,&fonts,true,"Vulkan",os.args[3]);assert(vulkan.validation_error_count(&v)==0);assert(vulkan.renderer_destroy(&v)==.None)
    }
}
