// Depth-only prepass shader.
// Renders scene depth from camera's perspective (reverse-Z, CompareOp::Greater).
// Used before the main geometry pass to populate the depth buffer,
// enabling early-Z rejection in the PBR pass and tighter CSM cascade fitting.

#include <frame_uniforms.wgsl>

@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,
    @location(3) vert_texcoord0: vec2f,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> @builtin(position) vec4f {
    let obj = objects[instance_idx];
    let world_pos = obj.model * vec4f(in.position, 1.0);
    return frame_data.proj * frame_data.view * world_pos;
}

@fragment
fn fs_main() {
    // Depth is written by the rasterizer — no color output needed.
}
