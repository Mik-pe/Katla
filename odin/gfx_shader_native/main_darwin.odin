#+build darwin, arm64
//! Real WGSL replacement publication and pending native pipeline ownership on both adapters.
package main

import gfx "../gfx"
import shader "../gfx/shader"
import adapter "../gfx/shader_adapter"
import metal "../gfx/metal"
import vulkan "../gfx/vulkan"
import NS "core:sys/darwin/Foundation"
import "core:mem"
import "core:os"
import "core:fmt"
import "core:thread"
import "core:time"

SOURCE :: `
override RED:f32=0.25;
@vertex fn vertex_main(@builtin(vertex_index) id:u32)->@builtin(position) vec4f {
    let positions=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));
    return vec4f(positions[id],0.25,1);
}
@fragment fn fragment_main()->@location(0) vec4f { return vec4f(RED,0.5,0.75,1); }
`
API :: struct($R:typeid) {
    create_texture:proc(^R,gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error),
    destroy_texture:proc(^R,gfx.Texture_Handle)->gfx.Gpu_Error,
    create_pipeline:proc(^R,gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error),
    destroy_pipeline:proc(^R,gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error,
    create_buffer:proc(^R,gfx.Buffer_Desc)->(gfx.Buffer_Handle,gfx.Gpu_Error),
    destroy_buffer:proc(^R,gfx.Buffer_Handle)->gfx.Gpu_Error,
    read_buffer:proc(^R,gfx.Buffer_Handle,u64,[]byte)->gfx.Gpu_Error,
    acquire:proc(^R)->(gfx.Frame_Token,gfx.Gpu_Error),
    submit:proc(^R,gfx.Frame_Token,^gfx.Graph,^gfx.Compiled_Graph,[]gfx.Buffer_Input,[]gfx.Texture_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error),
    wait:proc(^R,gfx.Submission)->gfx.Gpu_Error,
}
Native_State :: struct($R:typeid) { renderer:^R, api:API(R), created,destroyed:int, fail:bool, allocator:mem.Allocator }
Native_Pipeline :: struct { handle:gfx.Graphics_Pipeline_Handle }
prepare_native :: proc(state:^Native_State($R),compiled:^shader.Compiled)->(rawptr,bool) {
    input,err:=adapter.graphics(compiled,"vertex_main","fragment_main",{colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}},front_counter_clockwise=true},state.allocator)
    if err!=.None { fmt.eprintln("WGSL adapter rejected",err); return nil,false }; defer adapter.graphics_destroy(&input)
    if state.fail { input.descriptor.vertex_entry="missing_selected_entry"; input.descriptor.vertex_metal_entry="missing_selected_entry" }
    handle,native_error:=state.api.create_pipeline(state.renderer,input.descriptor)
    if native_error!=.None { fmt.eprintln("Native WGSL pipeline rejected",native_error); return nil,false }
    pipeline:=new(Native_Pipeline,state.allocator); pipeline.handle=handle; state.created+=1
    return pipeline,true
}
destroy_native :: proc(state:^Native_State($R),native:rawptr) {
    pipeline:=cast(^Native_Pipeline)native
    assert(state.api.destroy_pipeline(state.renderer,pipeline.handle)==.None)
    state.destroyed+=1; free(pipeline,state.allocator)
}
prepare_metal :: proc(data:rawptr,compiled:^shader.Compiled)->(rawptr,bool) { return prepare_native(cast(^Native_State(metal.Renderer))data,compiled) }
destroy_metal :: proc(data,native:rawptr) { destroy_native(cast(^Native_State(metal.Renderer))data,native) }
prepare_vulkan :: proc(data:rawptr,compiled:^shader.Compiled)->(rawptr,bool) { return prepare_native(cast(^Native_State(vulkan.Renderer))data,compiled) }
destroy_vulkan :: proc(data,native:rawptr) { destroy_native(cast(^Native_State(vulkan.Renderer))data,native) }
Work :: struct { graph:gfx.Graph, plan:gfx.Compiled_Graph, color:gfx.Texture_Handle, output:gfx.Buffer_Handle, image:gfx.Image_Id, bytes:gfx.Resource_Id, submission:gfx.Submission }
work_init :: proc(work:^Work,r:^$R,api:API(R),pipeline:gfx.Graphics_Pipeline_Handle) {
    color_desc:=gfx.Texture_Desc{width=64,height=8,mip_levels=1,layers=1,format=.RGBA8_Unorm,usage={.Color_Attachment,.Transfer_Source},depth=1}
    color,ce:=api.create_texture(r,color_desc); assert(ce==.None); work.color=color
    output,be:=api.create_buffer(r,{size=2048,usage={.Transfer_Destination,.Readback},memory=.CPU_Visible}); assert(be==.None); work.output=output
    gfx.graph_init(&work.graph)
    image,ie:=gfx.graph_image(&work.graph,color_desc,{},false,true); assert(ie==.None); work.image=image
    bytes,re:=gfx.graph_buffer(&work.graph,{size=2048,usage={.Transfer_Destination,.Readback},memory=.CPU_Visible},false,true); assert(re==.None); work.bytes=bytes
    write:=gfx.Image_Access{image,gfx.image_full_range(color_desc),.Write,.Color_Attachment}
    raster,pe:=gfx.graph_pass(&work.graph,"selected WGSL pipeline",.Graphics,nil,images={write}); assert(pe==.None)
    assert(gfx.graph_set_packet(&work.graph,raster,gfx.Render{colors={{write,.Clear,.Store,{0,0,0,1}}},phases={{pipeline=pipeline,draws={gfx.Draw{3,1,0,0}}}}})==.None)
    transfer,te:=gfx.graph_pass(&work.graph,"verify rendered bytes",.Transfer,{{bytes,{0,2048},.Write,.Transfer_Destination}},images={{image,gfx.image_full_range(color_desc),.Read,.Transfer_Source}}); assert(te==.None)
    assert(gfx.graph_set_packet(&work.graph,transfer,gfx.Copy_Image_Buffer{image,{width=64,height=8,aspect=.Color,depth=1},bytes,0})==.None)
    plan,error:=gfx.graph_compile(&work.graph); assert(error==.None); work.plan=plan
    token,ae:=api.acquire(r); assert(ae==.None)
    submission,se,preflight:=api.submit(r,token,&work.graph,&work.plan,{{bytes,output}},{{image,color}})
    assert(se==.None && preflight==.None); work.submission=submission
}
work_verify :: proc(work:^Work,r:^$R,api:API(R),red:byte) {
    assert(api.wait(r,work.submission)==.None)
    pixels:[2048]byte; assert(api.read_buffer(r,work.output,0,pixels[:])==.None)
    for pixel in 0..<512 { offset:=pixel*4; assert(pixels[offset]==red && pixels[offset+1]==128 && pixels[offset+2]==191 && pixels[offset+3]==255) }
    assert(api.destroy_texture(r,work.color)==.None); assert(api.destroy_buffer(r,work.output)==.None)
    gfx.compiled_graph_destroy(&work.plan); gfx.graph_destroy(&work.graph)
}
publication :: proc(registry:^shader.Registry)->shader.Publication {
    started:=time.tick_now()
    for time.tick_since(started)<10*time.Second {
        result,done:=shader.registry_poll(registry)
        if done { return result }
        thread.yield()
    }
    panic("WGSL replacement did not complete")
}
request :: proc(service:^shader.Service,red:f64)->u64 {
    revision,err:=shader.service_submit(service,1,SOURCE,{{"vertex_main",.Vertex},{"fragment_main",.Fragment}},{{"RED",red}})
    assert(err==.None); return revision
}
run :: proc(r:^$R,api:API(R),compiler:^shader.Compiler,prepare:proc(rawptr,^shader.Compiled)->(rawptr,bool),destroy:proc(rawptr,rawptr)) {
    state:=Native_State(R){renderer=r,api=api,allocator=context.allocator}
    service:shader.Service; assert(shader.service_init(&service,compiler)==.None)
    defer { assert(shader.service_destroy(&service)==.None) }
    registry:shader.Registry; assert(shader.registry_init(&registry,&service,{&state,prepare,destroy})==.None)
    defer { assert(shader.registry_destroy(&registry)==.None) }
    first_revision:=request(&service,0.25)
    first:=publication(&registry); assert(first.error==.None && first.revision==first_revision); shader.publication_destroy(&first)
    old,oe:=shader.registry_acquire(&registry,1); assert(oe==.None)
    works:[2]Work
    work_init(&works[0],r,api,(cast(^Native_Pipeline)old.pipeline).handle)
    second_revision:=request(&service,0.5)
    second:=publication(&registry); assert(second.error==.None && second.revision==second_revision); shader.publication_destroy(&second)
    assert(state.destroyed==0)
    current,ce:=shader.registry_acquire(&registry,1); assert(ce==.None && current.revision==second_revision && current.pipeline!=old.pipeline)
    work_init(&works[1],r,api,(cast(^Native_Pipeline)current.pipeline).handle)
    assert(shader.registry_destroy(&registry)==.Busy)
    assert(shader.registry_release(&registry,&old)==.None && state.destroyed==1)
    _,invalid:=shader.service_submit(&service,1,"invalid WGSL",{{"vertex_main",.Vertex},{"fragment_main",.Fragment}}); assert(invalid==.None)
    failed:=publication(&registry); assert(failed.error==.Compile_Failed && failed.compiler_error==.Parse); shader.publication_destroy(&failed)
    state.fail=true
    request(&service,0.75)
    rejected:=publication(&registry); assert(rejected.error==.Prepare_Failed); shader.publication_destroy(&rejected)
    survivor,survivor_error:=shader.registry_acquire(&registry,1); assert(survivor_error==.None && survivor.pipeline==current.pipeline && survivor.revision==second_revision)
    assert(shader.registry_release(&registry,&survivor)==.None)
    assert(shader.registry_release(&registry,&current)==.None)
    work_verify(&works[0],r,api,64); work_verify(&works[1],r,api,128)
    assert(shader.registry_destroy(&registry)==.None)
    assert(state.created==state.destroyed)
    fmt.println("WGSL native reload: 1024 RGBA pixels verified across old/new pipelines, pending-owner release, compile failure and native preparation failure")
}
main :: proc() {
    assert(len(os.args)==3,"Pass explicit Naga compiler library and Vulkan loader paths")
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0,"shader/native owner leaked allocations"); mem.tracking_allocator_destroy(&tracker) }
    context.allocator=mem.tracking_allocator(&tracker)
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    compiler:shader.Compiler; assert(shader.compiler_init(&compiler,os.args[1])==.None)
    defer { assert(shader.compiler_destroy(&compiler)==.None) }
    m:metal.Renderer; assert(metal.renderer_init(&m)==.None)
    ma:=API(metal.Renderer){metal.create_texture,metal.destroy_texture,metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer,metal.destroy_buffer,metal.read_buffer,metal.acquire,metal.submit,metal.wait}
    run(&m,ma,&compiler,prepare_metal,destroy_metal)
    assert(metal.renderer_destroy(&m)==.None)
    v:vulkan.Renderer; assert(vulkan.renderer_init(&v,validation=true,loader_path=os.args[2])==.None)
    va:=API(vulkan.Renderer){vulkan.create_texture,vulkan.destroy_texture,vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer,vulkan.destroy_buffer,vulkan.read_buffer,vulkan.acquire,vulkan.submit,vulkan.wait}
    run(&v,va,&compiler,prepare_vulkan,destroy_vulkan)
    assert(vulkan.validation_error_count(&v)==0); assert(vulkan.renderer_destroy(&v)==.None)
}
