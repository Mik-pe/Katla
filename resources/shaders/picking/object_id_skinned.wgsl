// Skinned object-ID picking shader.
// Same as object_id.wgsl but with GPU skeletal animation support.

#include <frame_uniforms.wgsl>
#include <lighting_types.wgsl>

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
    @location(0) @interpolate(flat) instance_idx: u32,
}

fn compute_skin_matrix(
    joint_indices: vec4u,
    joint_weights: vec4f,
) -> mat4x4f {
    let m0 = joint_matrices[joint_indices[0]] * joint_weights[0];
    let m1 = joint_matrices[joint_indices[1]] * joint_weights[1];
    let m2 = joint_matrices[joint_indices[2]] * joint_weights[2];
    let m3 = joint_matrices[joint_indices[3]] * joint_weights[3];

    return m0 + m1 + m2 + m3;
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    let obj = objects[instance_idx];

    let skin_matrix = compute_skin_matrix(in.joint_indices, in.joint_weights);
    let skinned_pos = skin_matrix * vec4f(in.position, 1.0);

    let world_pos = obj.model * skinned_pos;

    var out: VertexOutput;
    out.clip_position = frame_data.proj * frame_data.view * world_pos;
    out.instance_idx = instance_idx;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4u {
    return vec4u(in.instance_idx + 1u, 0u, 0u, 1u);
}
