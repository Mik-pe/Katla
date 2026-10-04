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
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0,"native owner leaked Odin allocations"); mem.tracking_allocator_destroy(&tracker) }
    context.allocator=mem.tracking_allocator(&tracker)
    assert(len(os.args)==3 || len(os.args)==4,"Pass fill.spv and params.spv paths, optionally a Vulkan loader path")
    fill_code:=load_spirv(os.args[1]); defer delete(fill_code)
    param_code:=load_spirv(os.args[2]); defer delete(param_code)
    renderer:gpu.Renderer
    loader:=""
    if len(os.args)==4 { loader=os.args[3] }
    assert(gpu.renderer_init(&renderer,validation=true,loader_path=loader)==.None,"Vulkan 1.3 + validation required")
    defer { assert(gpu.renderer_destroy(&renderer)==.None) }
    api:=acceptance.API(gpu.Renderer){gpu.create_buffer,gpu.destroy_buffer,gpu.create_pipeline,gpu.destroy_pipeline,gpu.read_buffer,gpu.write_buffer,gpu.submit,gpu.wait,gpu.poll}
    fill:=gfx.Compute_Desc{entry="main",spirv=fill_code,local_size={64,1,1},buffers={{0,.Storage}}}
    params:=gfx.Compute_Desc{entry="main",spirv=param_code,local_size={64,1,1},buffers={{0,.Storage},{1,.Uniform}}}
    acceptance.run(&renderer,api,fill,params)
    assert(gpu.validation_error_count(&renderer)==0)
}
