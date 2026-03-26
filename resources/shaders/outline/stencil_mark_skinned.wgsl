// Skinned stencil mark pass for selection outlines.
// Same as stencil_mark.wgsl but with skeletal animation support.

#include <frame_uniforms.wgsl>

@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

@group(2) @binding(0)
var<storage, read> joint_matrices: array<mat4x4f>;

struct VertexInput {
    @location(0) position: vec3f,
    @location(4) joint_indices: vec4u,
    @location(5) joint_weights: vec4f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    let obj = objects[instance_idx];

    var skinned_pos = vec4f(0.0);
    let weights = in.joint_weights;
    for (var i = 0u; i < 4u; i = i + 1u) {
        let joint_idx = in.joint_indices[i];
        let joint_mat = joint_matrices[joint_idx];
        skinned_pos = skinned_pos + weights[i] * (joint_mat * vec4f(in.position, 1.0));
    }
    skinned_pos.w = 1.0;

    let world_pos = obj.model * skinned_pos;

    var out: VertexOutput;
    out.clip_position = frame_data.proj * frame_data.view * world_pos;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return vec4f(0.0);
}
