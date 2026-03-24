// Draw Command Finalization Compute Shader
//
// Writes the indirect draw command based on the post-simulate alive_count.
// Must be dispatched as a single 1x1x1 workgroup AFTER the simulate pass,
// with a full pipeline barrier between simulate and this dispatch.
//
// The pipeline barrier ensures all workgroups' atomicAdd(&alive_count) from
// the simulate pass are visible, so atomicLoad returns the correct total.

#include "common.wgsl"

struct DrawIndirectCommand {
    vertex_count: u32,
    instance_count: u32,
    first_vertex: u32,
    first_instance: u32,
}

@group(0) @binding(0)
var<storage, read> counters: ParticleCounters;

@group(0) @binding(1)
var<storage, read_write> draw_command: DrawIndirectCommand;

@compute @workgroup_size(1)
fn cs_main() {
    let total_alive = atomicLoad(&counters.alive_count);
    draw_command.vertex_count = total_alive * 6u;
    draw_command.instance_count = 1u;
    draw_command.first_vertex = 0u;
    draw_command.first_instance = 0u;
}
