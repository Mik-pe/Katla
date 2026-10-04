#+build darwin, arm64
//! Native Metal acceptance shares authored packets and ownership checks with Vulkan.
package main

import gfx "../gfx"
import gpu "../gfx/metal"
import acceptance "../gfx_conformance"
import NS "core:sys/darwin/Foundation"
import "core:mem"

shader_source :: `
#include <metal_stdlib>
using namespace metal;
kernel void fill(device uint *data [[buffer(0)]], uint i [[thread_position_in_grid]]) { data[i] = i * 3u + 7u; }
kernel void params(device uint *data [[buffer(0)]], constant uint4 &values [[buffer(1)]], uint i [[thread_position_in_grid]]) { data[i] = data[i] * values.x + values.y; }
struct Raster_Vertex { float4 position [[position]]; };
vertex Raster_Vertex raster_vertex(uint i [[vertex_id]]) {
    const float2 positions[3] = { float2(-1,-1), float2(3,-1), float2(-1,3) };
    Raster_Vertex result; result.position = float4(positions[i], 0.25, 1); return result;
}
fragment float4 raster_fragment() { return float4(0.25,0.5,0.75,1); }
`
main :: proc() {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0,"native owner leaked Odin allocations"); mem.tracking_allocator_destroy(&tracker) }
    context.allocator=mem.tracking_allocator(&tracker)
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    renderer:gpu.Renderer
    assert(gpu.renderer_init(&renderer)==.None,"Metal 4 device required")
    defer { assert(gpu.renderer_destroy(&renderer)==.None) }
    api:=acceptance.API(gpu.Renderer){create_buffer=gpu.create_buffer,create_buffer_with_data=gpu.create_buffer_with_data,destroy_buffer=gpu.destroy_buffer,create_pipeline=gpu.create_pipeline,destroy_pipeline=gpu.destroy_pipeline,read_buffer=gpu.read_buffer,write_buffer=gpu.write_buffer,acquire=gpu.acquire,abort=gpu.abort,submit=gpu.submit,wait=gpu.wait,poll=gpu.poll}
    fill:=gfx.Compute_Desc{entry="fill",metal_entry="fill",metal_source=shader_source,local_size={64,1,1},buffers={{group=0,slot=0,metal_index=0,size_index=-1,usage=.Storage,mode=.Write}},runtime_sizes_index=-1}
    params:=gfx.Compute_Desc{entry="params",metal_entry="params",metal_source=shader_source,local_size={64,1,1},buffers={{group=0,slot=0,metal_index=0,size_index=-1,usage=.Storage,mode=.Read_Write},{group=0,slot=1,metal_index=1,size_index=-1,usage=.Uniform,mode=.Read}},runtime_sizes_index=-1}
    acceptance.run(&renderer,api,fill,params)
    graphics:=acceptance.Graphics_API(gpu.Renderer){create_texture=gpu.create_texture,destroy_texture=gpu.destroy_texture,create_pipeline=gpu.create_graphics_pipeline,destroy_pipeline=gpu.destroy_graphics_pipeline,create_buffer=gpu.create_buffer,destroy_buffer=gpu.destroy_buffer,read_buffer=gpu.read_buffer,acquire=gpu.acquire,abort=gpu.abort,submit=gpu.submit,wait=gpu.wait,source=gpu.graph_texture_source,queue_readback=gpu.queue_texture_readback,poll_readback=gpu.poll_texture_readback,destroy_readback=gpu.destroy_readback,release_exports=gpu.release_graph_exports}
    raster:=gfx.Graphics_Desc{vertex_entry="raster_vertex",fragment_entry="raster_fragment",vertex_metal_entry="raster_vertex",fragment_metal_entry="raster_fragment",vertex_metal_source=shader_source,fragment_metal_source=shader_source,vertex_sizes_index=-1,fragment_sizes_index=-1,colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}},depth={enabled=true,test=true,write=true,compare=.Less,format=.D32_Float},front_counter_clockwise=true}
    acceptance.run_graphics(&renderer,graphics,raster)
    acceptance.run_allocations(&renderer,api,graphics,gpu.allocation_query(&renderer),gpu.allocation_api(&renderer),fill)
}
