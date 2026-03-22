// Depth-only shader for CSM shadow map rendering with GPU skeletal animation.
// Each cascade is rendered as a separate draw with cascade_index set via a storage buffer.

#include <shadow_cascade_data.wgsl>

struct ShadowParams {
    cascade_index: u32,
    bias: f32,
    _pad: vec2f,
}

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

@group(2) @binding(0)
var<storage, read> shadow_cascades: array<ShadowCascadeData, 4>;

@group(2) @binding(1)
var<storage, read> shadow_params: ShadowParams;

@group(3) @binding(0)
var<storage, read> joint_matrices: array<mat4x4f>;

struct VertexInput {
    @location(0) position: vec3f,
    @location(4) joint_indices: vec4u,
    @location(5) joint_weights: vec4f,
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
) -> @builtin(position) vec4f {
    let obj = objects[instance_idx];
    let cascade = shadow_cascades[shadow_params.cascade_index];

    let skin_matrix = compute_skin_matrix(in.joint_indices, in.joint_weights);
    let skinned_pos = skin_matrix * vec4f(in.position, 1.0);

    let world_pos = obj.model * skinned_pos;
    return cascade.view_proj * world_pos;
}

@fragment
fn fs_main() {
}
