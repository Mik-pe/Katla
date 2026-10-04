#+build darwin, arm64
//! Real file watching publishes complete native shader families while accepted GPU work retains old handles.
package main

import render "../app/render"
import gfx "../gfx"
import shader "../gfx/shader"
import adapter "../gfx/shader_adapter"
import metal "../gfx/metal"
import vulkan "../gfx/vulkan"
import NS "core:sys/darwin/Foundation"
import "core:os"
import "core:mem"
import "core:path/filepath"
import "core:fmt"
import "core:time"

SOURCE :: `//include "tint.wgsl"
@vertex fn vs_main(@builtin(vertex_index) index:u32)->@builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));return vec4f(p[index],0.25,1);
}
@fragment fn fs_main()->@location(0) vec4f { return vec4f(RED,0.5,0.75,1); }
`
API :: struct($R:typeid) {
    create_pipeline:proc(^R,gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error),
    destroy_pipeline:proc(^R,gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error,
    create_texture:proc(^R,gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error),
    destroy_texture:proc(^R,gfx.Texture_Handle)->gfx.Gpu_Error,
    create_buffer:proc(^R,gfx.Buffer_Desc)->(gfx.Buffer_Handle,gfx.Gpu_Error),
    destroy_buffer:proc(^R,gfx.Buffer_Handle)->gfx.Gpu_Error,
    read_buffer:proc(^R,gfx.Buffer_Handle,u64,[]byte)->gfx.Gpu_Error,
    acquire:proc(^R)->(gfx.Frame_Token,gfx.Gpu_Error),
    submit:proc(^R,gfx.Frame_Token,^gfx.Graph,^gfx.Compiled_Graph,[]gfx.Buffer_Input,[]gfx.Texture_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error),
    wait:proc(^R,gfx.Submission)->gfx.Gpu_Error,
    release:proc(^R,^gfx.Graph)->gfx.Gpu_Error,
}
State :: struct($R:typeid) {
    renderer:^R,api:API(R),ui:render.UI_GPU(R),pick:render.Picking_Native(R),
    ui_reference:^render.UI_Shader,pick_references:[2]^shader.Compiled,color_reference:^shader.Compiled,
    current:[2]gfx.Graphics_Pipeline_Handle,published,created,destroyed:int,fail:bool,allocator:mem.Allocator,
}
Candidate :: struct($R:typeid) { owner:^State(R),colors:[2]gfx.Graphics_Pipeline_Handle,ui:^render.UI_Shader_Reload_Candidate(R),pick:^render.Picking_Shader_Reload_Candidate(R) }
prepare :: proc(state:^State($R),artifacts:[]shader.Compiled)->(rawptr,render.Shader_Reload_Error) {
    if len(artifacts)!=6 { return nil,.Prepare }
    candidate:=new(Candidate(R),state.allocator);candidate.owner=state
    for i in 0..<2 {
        if !render.shader_reload_interface_compatible(state.color_reference,&artifacts[i]) { return candidate,.Prepare }
        mapped,error:=adapter.graphics(&artifacts[i],"vs_main","fs_main",{colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}},state.allocator)
        if error!=.None { return candidate,.Prepare }
        if state.fail && i==1 { mapped.descriptor.vertex_entry="absent_entry";mapped.descriptor.vertex_metal_entry="absent_entry" }
        handle,native_error:=state.api.create_pipeline(state.renderer,mapped.descriptor);adapter.graphics_destroy(&mapped)
        if native_error!=.None { return candidate,.Prepare }
        candidate.colors[i]=handle;state.created+=1
    }
    error:render.Shader_Reload_Error
    candidate.ui,error=render.ui_shader_reload_prepare(&state.ui,state.ui_reference,artifacts[2:4]);if error!=.None { return candidate,error }
    candidate.pick,error=render.picking_shader_reload_prepare(&state.pick,state.pick_references,artifacts[4:6],state.allocator);if error!=.None { return candidate,error }
    return candidate,.None
}
publish :: proc(state:^State($R),pointer:rawptr)->rawptr {
    candidate:=cast(^Candidate(R))pointer
    old:=state.current;state.current=candidate.colors;candidate.colors=old
    candidate.ui=render.ui_shader_reload_publish(candidate.ui);candidate.pick=render.picking_shader_reload_publish(candidate.pick)
    state.published+=1;return candidate
}
destroy :: proc(state:^State($R),pointer:rawptr) {
    candidate:=cast(^Candidate(R))pointer
    for handle in candidate.colors { if handle.owner!=nil { assert(state.api.destroy_pipeline(state.renderer,handle)==.None);state.destroyed+=1 } }
    render.ui_shader_reload_destroy(candidate.ui);render.picking_shader_reload_destroy(candidate.pick);free(candidate,state.allocator)
}
prepare_metal :: proc(state:rawptr,artifacts:[]shader.Compiled)->(rawptr,render.Shader_Reload_Error) { return prepare(cast(^State(metal.Renderer))state,artifacts) }
publish_metal :: proc(state,candidate:rawptr)->rawptr { return publish(cast(^State(metal.Renderer))state,candidate) }
destroy_metal :: proc(state,candidate:rawptr) { destroy(cast(^State(metal.Renderer))state,candidate) }
prepare_vulkan :: proc(state:rawptr,artifacts:[]shader.Compiled)->(rawptr,render.Shader_Reload_Error) { return prepare(cast(^State(vulkan.Renderer))state,artifacts) }
publish_vulkan :: proc(state,candidate:rawptr)->rawptr { return publish(cast(^State(vulkan.Renderer))state,candidate) }
destroy_vulkan :: proc(state,candidate:rawptr) { destroy(cast(^State(vulkan.Renderer))state,candidate) }
Work :: struct { graph:gfx.Graph,plan:gfx.Compiled_Graph,target:gfx.Texture_Handle,buffer:gfx.Buffer_Handle,submission:gfx.Submission }
work :: proc(state:^State($R),index:int,result:^Work) {
    gfx.graph_init(&result.graph,state.allocator)
    desc:=gfx.Texture_Desc{width=32,height=8,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Color_Attachment,.Transfer_Source}}
    error:gfx.Gpu_Error;result.target,error=state.api.create_texture(state.renderer,desc);assert(error==.None)
    output_desc:=gfx.Buffer_Desc{size=1024,usage={.Readback,.Transfer_Destination}}
    result.buffer,error=state.api.create_buffer(state.renderer,output_desc);assert(error==.None)
    image,image_error:=gfx.graph_image(&result.graph,desc,{},false,true);assert(image_error==.None)
    bytes,bytes_error:=gfx.graph_buffer(&result.graph,output_desc,false,true);assert(bytes_error==.None)
    access:=gfx.Image_Access{image,gfx.image_full_range(desc),.Write,.Color_Attachment}
    raster,pass_error:=gfx.graph_pass(&result.graph,"current watched source",.Graphics,nil,images={access});assert(pass_error==.None)
    assert(gfx.graph_set_packet(&result.graph,raster,gfx.Render{colors={{access,.Clear,.Store,{0,0,0,1}}},phases={{pipeline=state.current[index],draws={gfx.Draw{3,1,0,0}}}}})==.None)
    transfer,transfer_error:=gfx.graph_pass(&result.graph,"actual watched shader pixels",.Transfer,{{bytes,{0,1024},.Write,.Transfer_Destination}},images={{image,gfx.image_full_range(desc),.Read,.Transfer_Source}});assert(transfer_error==.None)
    assert(gfx.graph_set_packet(&result.graph,transfer,gfx.Copy_Image_Buffer{image,{width=32,height=8,depth=1,aspect=.Color},bytes,0})==.None)
    compile_error:gfx.Graph_Error;result.plan,compile_error=gfx.graph_compile(&result.graph);assert(compile_error==.None)
    token,acquire_error:=state.api.acquire(state.renderer);assert(acquire_error==.None)
    packet_error:gfx.Packet_Error;result.submission,error,packet_error=state.api.submit(state.renderer,token,&result.graph,&result.plan,{{bytes,result.buffer}},{{image,result.target}});assert(error==.None && packet_error==.None)
}
verify :: proc(state:^State($R),accepted:^Work,red:byte) {
    assert(state.api.wait(state.renderer,accepted.submission)==.None)
    pixels:[1024]byte;assert(state.api.read_buffer(state.renderer,accepted.buffer,0,pixels[:])==.None)
    for index in 0..<256 { offset:=index*4;assert(pixels[offset]==red && pixels[offset+1]==128 && pixels[offset+2]==191 && pixels[offset+3]==255) }
    assert(state.api.release(state.renderer,&accepted.graph)==.None);gfx.compiled_graph_destroy(&accepted.plan);gfx.graph_destroy(&accepted.graph)
    assert(state.api.destroy_texture(state.renderer,accepted.target)==.None);assert(state.api.destroy_buffer(state.renderer,accepted.buffer)==.None)
}
write_source :: proc(root,path,source:string) { absolute,error:=filepath.join({root,path});assert(error==nil);defer delete(absolute);assert(os.write_entire_file(absolute,transmute([]byte)source)==nil) }
wait_reload :: proc(service:^render.Shader_Reload_Service,frame:^u64)->render.Shader_Reload_Status {
    started:=time.tick_now()
    for time.tick_since(started)<30*time.Second { frame^+=1;result:=render.shader_reload_poll(service,frame^);if result.published!=0 || result.failed!=0 { return result };time.sleep(time.Millisecond) }
    panic("native shader family reload timed out")
}
run :: proc(state:^State($R),compiler:^shader.Compiler,shader_root:string,publisher:render.Shader_Reload_Publisher,ui_ops:render.UI_GPU_Ops(R),gpu_ops:render.GPU_Ops(R)) {
    root,error:=os.make_directory_temp("","katla-app-native-reload-*",state.allocator);assert(error==nil);defer { os.remove_all(root);delete(root,state.allocator) }
    names:=[4]string{"ui.wgsl","ui_transfer.wgsl","picking.wgsl","picking_mask.wgsl"}
    for name in names { original,path_error:=filepath.join({shader_root,name});assert(path_error==nil);defer delete(original);copy_to,copy_error:=filepath.join({root,name});assert(copy_error==nil);defer delete(copy_to);assert(os.copy_file(copy_to,original)==nil) }
    write_source(root,"first.wgsl",SOURCE);write_source(root,"second.wgsl",SOURCE);write_source(root,"tint.wgsl","const RED:f32=0.25;")
    initial,error_initial:=shader.compile(compiler,"const RED:f32=0.25;\n"+SOURCE[len("//include \"tint.wgsl\"\n"):],{{"vs_main",.Vertex},{"fs_main",.Fragment}},allocator=state.allocator);assert(error_initial==.None);defer shader.compiled_destroy(&initial);state.color_reference=&initial
    ui_shader,ui_error:=render.ui_shader_compile(compiler,.RGBA8_Unorm,state.allocator);assert(ui_error==.None);defer render.ui_shader_destroy(&ui_shader);state.ui_reference=&ui_shader
    assert(render.ui_gpu_init(&state.ui,state.renderer,ui_ops,&ui_shader,state.allocator)==.None);defer assert(render.ui_gpu_destroy(&state.ui)==.None)
    picking:[2]render.Picking_Shader
    for i in 0..<2 { compile_error:shader.Error;picking[i],compile_error=render.picking_shader_compile(compiler,allocator=state.allocator,masked=i==1);assert(compile_error==.None);state.pick_references[i]=&picking[i].compiled }
    defer { for &compiled in picking { render.picking_shader_destroy(&compiled) } }
    assert(render.picking_native_init(&state.pick,state.renderer,gpu_ops,compiler)=={});defer assert(render.picking_native_destroy(&state.pick)==.None)
    service:render.Shader_Reload_Service;assert(render.shader_reload_init(&service,compiler,root,state.allocator)==.None);defer assert(render.shader_reload_destroy(&service)==.None)
    family,registration_error:=render.shader_reload_register(&service,{name="two-scenes+UI+picking",modules={{path="first.wgsl",selections={{"vs_main",.Vertex},{"fs_main",.Fragment}}},{path="second.wgsl",selections={{"vs_main",.Vertex},{"fs_main",.Fragment}}},{path="ui.wgsl",selections={{"vs_ui",.Vertex},{"fs_ui",.Fragment}}},{path="ui_transfer.wgsl",selections={{"vs_transfer",.Vertex},{"fs_encode",.Fragment},{"fs_decode",.Fragment}}},{path="picking.wgsl",selections={{"vs_pick",.Vertex},{"fs_pick",.Fragment}}},{path="picking_mask.wgsl",selections={{"vs_pick",.Vertex},{"fs_pick",.Fragment}}}},publisher=publisher});assert(registration_error==.None)
    frame:u64;status:=wait_reload(&service,&frame);assert(status.published==1)
    runs:=compiler.process_runs;for _ in 0..<8 { frame+=1;assert(render.shader_reload_poll(&service,frame).changed==0) };assert(compiler.process_runs==runs)
    first,second:Work;work(state,0,&first);work(state,1,&second)
    write_source(root,"tint.wgsl","const RED:f32=0.75;");status=wait_reload(&service,&frame);assert(status.published==1)
    verify(state,&first,64);verify(state,&second,64)
    for i in 0..<4 { current:Work;work(state,i%2,&current);verify(state,&current,191) }
    previous:=state.current;write_source(root,"second.wgsl","invalid WGSL edit");status=wait_reload(&service,&frame);assert(status.error==.Compile && state.current==previous)
    for i in 0..<4 { current:Work;work(state,i%2,&current);verify(state,&current,191) }
    write_source(root,"second.wgsl",SOURCE);write_source(root,"tint.wgsl","const RED:f32=0.5;");state.fail=true
    status=wait_reload(&service,&frame);assert(status.error==.Prepare && state.current==previous)
    for i in 0..<4 { current:Work;work(state,i%2,&current);verify(state,&current,191) }
    state.fail=false;write_source(root,"tint.wgsl","const RED:f32=0.125;");status=wait_reload(&service,&frame);assert(status.published==1)
    for i in 0..<4 { current:Work;work(state,i%2,&current);verify(state,&current,32) }
    assert(render.shader_reload_set_options(&service,family,{1})==.None);status=wait_reload(&service,&frame);assert(status.published==1)
    for handle in state.current { assert(state.api.destroy_pipeline(state.renderer,handle)==.None);state.destroyed+=1 };state.current={};assert(state.created==state.destroyed)
    fmt.println("Native source watcher: coherent two-scene/UI/picking family, 4608 exact GPU pixels, old pending handles, failed WGSL/native rejection retention, recovery and unchanged-process cache verified")
}
main :: proc() {
    assert(len(os.args)==4,"Pass offline compiler, native Vulkan loader, canonical Odin shader root")
    pool:=NS.AutoreleasePool.alloc()->init();defer pool->drain()
    backing:=context.allocator;tracker:mem.Tracking_Allocator;mem.tracking_allocator_init(&tracker,backing);context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing;assert(len(tracker.allocation_map)==0,"shader reload native ownership leak");mem.tracking_allocator_destroy(&tracker) }
    compiler:shader.Compiler;assert(shader.compiler_init(&compiler,os.args[1])==.None);defer shader.compiler_destroy(&compiler)
    {
        renderer:metal.Renderer;assert(metal.renderer_init(&renderer)==.None);defer assert(metal.renderer_destroy(&renderer)==.None)
        state:=State(metal.Renderer){renderer=&renderer,allocator=context.allocator,api={metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_texture,metal.destroy_texture,metal.create_buffer,metal.destroy_buffer,metal.read_buffer,metal.acquire,metal.submit,metal.wait,metal.release_graph_exports}}
        ui_ops:=render.UI_GPU_Ops(metal.Renderer){metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.create_texture_with_data,metal.destroy_texture,metal.create_sampler,metal.destroy_sampler,metal.create_texture}
        gpu_ops:=render.GPU_Ops(metal.Renderer){create_pipeline=metal.create_graphics_pipeline,destroy_pipeline=metal.destroy_graphics_pipeline}
        run(&state,&compiler,os.args[3],{&state,prepare_metal,publish_metal,destroy_metal},ui_ops,gpu_ops)
    }
    {
        renderer:vulkan.Renderer;assert(vulkan.renderer_init(&renderer,validation=true,loader_path=os.args[2])==.None);defer assert(vulkan.renderer_destroy(&renderer)==.None)
        state:=State(vulkan.Renderer){renderer=&renderer,allocator=context.allocator,api={vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_texture,vulkan.destroy_texture,vulkan.create_buffer,vulkan.destroy_buffer,vulkan.read_buffer,vulkan.acquire,vulkan.submit,vulkan.wait,vulkan.release_graph_exports}}
        ui_ops:=render.UI_GPU_Ops(vulkan.Renderer){vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.create_texture_with_data,vulkan.destroy_texture,vulkan.create_sampler,vulkan.destroy_sampler,vulkan.create_texture}
        gpu_ops:=render.GPU_Ops(vulkan.Renderer){create_pipeline=vulkan.create_graphics_pipeline,destroy_pipeline=vulkan.destroy_graphics_pipeline}
        run(&state,&compiler,os.args[3],{&state,prepare_vulkan,publish_vulkan,destroy_vulkan},ui_ops,gpu_ops);assert(vulkan.validation_error_count(&renderer)==0)
    }
}
