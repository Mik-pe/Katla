// Surface-aware scene coverage shared by depth, picking and shadow passes.
#include <surface_coverage.wgsl>
@group(0) @binding(0) var<storage, read> frame_data: FrameUniforms;
@group(2) @binding(0) var<storage, read> joint_matrices: array<mat4x4f>;
struct VertexInput {
    @location(0) position: vec3f,
    @location(3) tex_coords: vec2f,
    @location(6) tex_coords1: vec2f,
    @location(4) joint_indices: vec4u,
    @location(5) joint_weights: vec4f,
}
fn vertex_position(position: vec4f, tex_coords: vec2f, tex_coords1: vec2f, instance_idx: u32) -> VertexOutput {
    let world_pos = objects[instance_idx].model * position;
    var out: VertexOutput;
    out.clip_position = frame_data.proj * frame_data.view * world_pos;
    out.instance_idx = instance_idx;
    out.tex_coords = tex_coords;
    out.tex_coords1 = tex_coords1;
    return out;
}
@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance_idx: u32) -> VertexOutput {
    let skin = joint_matrices[in.joint_indices.x] * in.joint_weights.x
             + joint_matrices[in.joint_indices.y] * in.joint_weights.y
             + joint_matrices[in.joint_indices.z] * in.joint_weights.z
             + joint_matrices[in.joint_indices.w] * in.joint_weights.w;
    return vertex_position(skin * vec4f(in.position, 1.0), in.tex_coords, in.tex_coords1, instance_idx);
}
