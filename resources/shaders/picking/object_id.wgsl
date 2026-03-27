// Object-ID picking shader.
// Renders each object with a flat color encoding its entity ID.
// The entity ID is packed into the R channel of a R32Uint texture.
// Uses the same depth test as the main render (reverse-Z, Greater).
// Depth is reused from the depth prepass (LoadOp::Load).

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
    // Encode instance_index + 1 into the R channel.
    // +1 so that index 0 (reserved for fullscreen passes) maps to 1,
    // and "no object" (background/cleared) maps to 0.
    return vec4u(in.instance_idx + 1u, 0u, 0u, 1u);
}
