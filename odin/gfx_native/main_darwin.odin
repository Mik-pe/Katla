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
    api:=acceptance.API(gpu.Renderer){gpu.create_buffer,gpu.destroy_buffer,gpu.create_pipeline,gpu.destroy_pipeline,gpu.read_buffer,gpu.write_buffer,gpu.submit,gpu.wait,gpu.poll}
    fill:=gfx.Compute_Desc{entry="fill",metal_source=shader_source,local_size={64,1,1},buffers={{0,.Storage}}}
    params:=gfx.Compute_Desc{entry="params",metal_source=shader_source,local_size={64,1,1},buffers={{0,.Storage},{1,.Uniform}}}
    acceptance.run(&renderer,api,fill,params)
}
