// Depth-only shader for CSM shadow map rendering with GPU skeletal animation.
// Each cascade is rendered as a separate draw with cascade_index set via a storage buffer.

#include <frame_uniforms.wgsl>
#include <shadow_cascade_data.wgsl>

struct ShadowParams {
    cascade_index: u32,
    bias: f32,
    _pad: vec2f,
}

@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

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
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,
    @location(3) vert_texcoord0: vec2f,
    @location(4) joint_indices: vec4u,
    @location(5) joint_weights: vec4f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) world_pos: vec3f,
    @location(1) world_normal: vec3f,
}

struct FragmentInput {
    @location(0) world_pos: vec3f,
    @location(1) world_normal: vec3f,
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
    var out: VertexOutput;

    let obj = objects[instance_idx];
    let cascade = shadow_cascades[shadow_params.cascade_index];

    let skin_matrix = compute_skin_matrix(in.joint_indices, in.joint_weights);
    let skinned_pos = skin_matrix * vec4f(in.position, 1.0);

    let world_pos = obj.model * skinned_pos;
    out.world_pos = world_pos.xyz;

    out.clip_position = cascade.view_proj * world_pos;

    let skin_matrix_3x3 = mat3x3f(
        skin_matrix[0].xyz,
        skin_matrix[1].xyz,
        skin_matrix[2].xyz,
    );

    let normal_matrix = mat3x3f(
        obj.model[0].xyz,
        obj.model[1].xyz,
        obj.model[2].xyz,
    );

    let skinned_normal = skin_matrix_3x3 * in.normal;
    out.world_normal = normal_matrix * skinned_normal;

    return out;
}

struct FragmentOutput {
    @builtin(frag_depth) out_depth: f32,
}

@fragment
fn fs_main(in: FragmentInput, @builtin(position) frag_coord: vec4f) -> FragmentOutput {
    let N = normalize(in.world_normal);
    let L = normalize(frame_data.light_direction.xyz);
    let bias = max(abs(dot(N, L)) * shadow_params.bias, shadow_params.bias * 0.5);

    var out: FragmentOutput;
    out.out_depth = frag_coord.z - bias;
    return out;
}
