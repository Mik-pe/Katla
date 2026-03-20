// Simple bindless shader with albedo texture only.
//
// Uses storage buffers for uniform data with instance_index for per-object selection.

#include <frame_uniforms.wgsl>
#include <bindless.wgsl>

// Set 0: Uniforms (storage buffers)
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

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) world_pos: vec3f,
    @location(1) tex_coords: vec2f,
    @location(2) world_normal: vec3f,
    @location(3) @interpolate(flat) instance_idx: u32,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let obj = objects[instance_idx];

    let world_pos = obj.model * vec4f(in.position, 1.0);
    out.world_pos = world_pos.xyz;
    out.clip_position = frame_data.proj * frame_data.view * world_pos;

    out.tex_coords = in.vert_texcoord0;

    let normal_matrix = mat3x3f(
        obj.model[0].xyz,
        obj.model[1].xyz,
        obj.model[2].xyz,
    );
    out.world_normal = normalize(normal_matrix * in.normal);

    out.instance_idx = instance_idx;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let obj = objects[in.instance_idx];

    let albedo_idx = obj.texture_indices.x;

    let albedo_sample = textureSample(bindless_textures[albedo_idx], shared_sampler, in.tex_coords);
    let albedo = albedo_sample.rgb * obj.base_color.rgb;
    let alpha = albedo_sample.a * obj.base_color.a;

    let N = normalize(in.world_normal);
    let L = normalize(frame_data.light_direction.xyz);
    let NdotL = max(dot(N, L), 0.0);

    let diffuse = albedo * NdotL;

    let ambient = albedo * 0.1;

    let color = ambient + diffuse * frame_data.light_intensity.x;

    return vec4f(color, alpha);
}
