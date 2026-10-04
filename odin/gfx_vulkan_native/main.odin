//! Native Vulkan acceptance shares authored packets and ownership checks with Metal.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import acceptance "../gfx_conformance"
import "core:os"
import "core:mem"

load_spirv :: proc(filename:string)->[]u32 {
    bytes,err:=os.read_entire_file(filename,context.allocator); assert(err==nil && len(bytes)>0 && len(bytes)%4==0)
    return mem.slice_data_cast([]u32,bytes)
}
main :: proc() {
    code:=run_native();if code!=0 { os.exit(code) }
}
run_native :: proc()->int {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0,"native owner leaked Odin allocations"); mem.tracking_allocator_destroy(&tracker) }
    context.allocator=mem.tracking_allocator(&tracker)
    if len(os.args)==3 && os.args[1]=="--probe-array-capabilities" { return probe_array_capabilities(os.args[2]) }
    assert(len(os.args)>=5,"Pass fill.spv, params.spv, triangle.spv, color.spv paths, optionally a Vulkan loader path")
    fill_code:=load_spirv(os.args[1]); defer delete(fill_code)
    param_code:=load_spirv(os.args[2]); defer delete(param_code)
    renderer:gpu.Renderer
    loader:=""
    if len(os.args)>=6 { loader=os.args[5] }
    baseline:=false;if len(os.args)>6 { for option in os.args[6:] { if option=="--baseline" { baseline=true } } }
    assert(gpu.renderer_init(&renderer,validation=true,loader_path=loader)==.None,"Vulkan 1.3 + validation required")
    defer { assert(gpu.renderer_destroy(&renderer)==.None) }
    api:=acceptance.API(gpu.Renderer){create_buffer=gpu.create_buffer,create_buffer_with_data=gpu.create_buffer_with_data,destroy_buffer=gpu.destroy_buffer,create_pipeline=gpu.create_pipeline,destroy_pipeline=gpu.destroy_pipeline,read_buffer=gpu.read_buffer,write_buffer=gpu.write_buffer,acquire=gpu.acquire,abort=gpu.abort,submit=gpu.submit,wait=gpu.wait,poll=gpu.poll}
    fill:=gfx.Compute_Desc{entry="main",spirv=fill_code,local_size={64,1,1},buffers={{group=0,slot=0,metal_index=0,size_index=-1,usage=.Storage,mode=.Write,minimum_size=4}}}
    params:=gfx.Compute_Desc{entry="main",spirv=param_code,local_size={64,1,1},buffers={{group=0,slot=0,metal_index=0,size_index=-1,usage=.Storage,mode=.Read_Write,minimum_size=4},{group=0,slot=1,metal_index=1,size_index=-1,usage=.Uniform,mode=.Read,minimum_size=16}}}
    acceptance.run(&renderer,api,fill,params)
    run_indirect(&renderer,fill)
    vertex_code:=load_spirv(os.args[3]); defer delete(vertex_code)
    fragment_code:=load_spirv(os.args[4]); defer delete(fragment_code)
    graphics_api:=acceptance.Graphics_API(gpu.Renderer){create_texture=gpu.create_texture,destroy_texture=gpu.destroy_texture,create_pipeline=gpu.create_graphics_pipeline,destroy_pipeline=gpu.destroy_graphics_pipeline,create_buffer=gpu.create_buffer,destroy_buffer=gpu.destroy_buffer,read_buffer=gpu.read_buffer,acquire=gpu.acquire,abort=gpu.abort,submit=gpu.submit,wait=gpu.wait,source=gpu.graph_texture_source,queue_readback=gpu.queue_texture_readback,poll_readback=gpu.poll_texture_readback,destroy_readback=gpu.destroy_readback,release_exports=gpu.release_graph_exports}
    desc:=gfx.Graphics_Desc{vertex_entry="main",fragment_entry="main",vertex_spirv=vertex_code,fragment_spirv=fragment_code,colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}},depth={enabled=true,test=true,write=true,compare=.Less,format=.D32_Float}}
    acceptance.run_graphics(&renderer,graphics_api,desc)
    acceptance.run_allocations(&renderer,api,graphics_api,gpu.allocation_query(&renderer),gpu.allocation_api(&renderer),fill)
    run_render_state(&renderer,vertex_code,fragment_code)
    run_subresources(&renderer); run_aliases(&renderer)
    for i:=6; i<len(os.args); {
        switch os.args[i] {
        case "--baseline": i+=1
        case "--depth-sense":
            assert(i+2<len(os.args)); code:=load_spirv(os.args[i+1]); tint:=load_spirv(os.args[i+2]);run_depth_sense(&renderer,code,tint);delete(code);delete(tint);i+=3
        case "--storage-arrays":
            assert(i+1<len(os.args));if !baseline { code:=load_spirv(os.args[i+1]); run_storage_arrays(&renderer,code); delete(code) };i+=2
        case "--arrays":
            assert(i+1<len(os.args));if !baseline { code:=load_spirv(os.args[i+1]); run_arrays(&renderer,vertex_code,code); delete(code) };i+=2
        case "--surface": run_window(&renderer); i+=1
        case "--volume":
            assert(i+1<len(os.args)); code:=load_spirv(os.args[i+1]); run_volume(&renderer,code); delete(code); i+=2
        case "--images":
            assert(i+2<len(os.args))
            sample_code:=load_spirv(os.args[i+1]); image_code:=load_spirv(os.args[i+2])
            run_image_bindings(&renderer,vertex_code,sample_code,image_code)
            run_formats(&renderer,vertex_code,sample_code)
            delete(sample_code); delete(image_code); i+=3
        case "--mesh":
            assert(i+2<len(os.args))
            mesh_code:=load_spirv(os.args[i+1]); tint_code:=load_spirv(os.args[i+2])
            run_mesh(&renderer,mesh_code,tint_code)
            run_wireframe(&renderer,mesh_code,tint_code)
            delete(mesh_code); delete(tint_code); i+=3
        case: panic("Unknown native acceptance option")
        }
    }
    assert(gpu.validation_error_count(&renderer)==0)
    return 0
}
