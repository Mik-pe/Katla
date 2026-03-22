// Depth-only shader for CSM shadow map rendering.
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

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,
    @location(3) vert_texcoord0: vec2f,
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

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let obj = objects[instance_idx];
    let cascade = shadow_cascades[shadow_params.cascade_index];

    let world_pos = obj.model * vec4f(in.position, 1.0);
    out.world_pos = world_pos.xyz;

    out.clip_position = cascade.view_proj * world_pos;

    let normal_matrix = mat3x3f(
        obj.model[0].xyz,
        obj.model[1].xyz,
        obj.model[2].xyz,
    );
    out.world_normal = normal_matrix * in.normal;

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
