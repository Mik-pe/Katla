// Depth prepass with object-ID output.
// Renders scene depth from camera's perspective (reverse-Z, CompareOp::Greater).
// Also outputs instance_index + 1 as a flat R32Uint color for GPU-based entity picking.
// This combines the depth prepass and object-ID pass into a single render pass.

#include <frame_uniforms.wgsl>

@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

struct VertexInput {
    @location(0) position: vec3f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) @interpolate(flat) instance_idx: u32,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    let obj = objects[instance_idx];
    let world_pos = obj.model * vec4f(in.position, 1.0);

    var out: VertexOutput;
    out.clip_position = frame_data.proj * frame_data.view * world_pos;
    out.instance_idx = instance_idx;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4u {
    // Depth is written by the rasterizer.
    // Encode instance_index + 1 into the R channel for picking.
    // Value 0 (cleared) means no object was hit.
    return vec4u(in.instance_idx + 1u, 0u, 0u, 1u);
}
