// PBR shader with BINDLESS TEXTURES, HDR output, and Forward+ dynamic lighting.
//
// Uses storage buffers for uniform data with instance_index for per-object selection.
// Three descriptor sets: uniforms (set 0), bindless textures (set 1), light culling (set 3).
//
// Implements:
// - Metallic/Roughness workflow with texture support
// - Tangent-space normal mapping
// - Directional lighting (sun) from frame_data
// - Dynamic point lights via Forward+ tile culling
// - HDR linear output (NO tonemapping - handled by post-process pass)

#include <frame_uniforms.wgsl>
#include <lighting_types.wgsl>
#include <bindless.wgsl>
#include <pbr.wgsl>

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

// Set 3: Forward+ light culling data
@group(3) @binding(0)
var<storage, read> point_lights: array<PointLightGPU, MAX_POINT_LIGHTS>;

@group(3) @binding(1)
var<storage, read> tile_light_indices: array<u32>;

@group(3) @binding(2)
var<storage, read> tile_light_counts: array<u32>;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,  // w component = handedness
    @location(3) vert_texcoord0: vec2f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) world_pos: vec3f,
    @location(1) tex_coords: vec2f,
    @location(2) world_normal: vec3f,
    @location(3) world_tangent: vec3f,
    @location(4) world_bitangent: vec3f,
    @location(5) @interpolate(flat) instance_idx: u32,
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

    // Gram-Schmidt reorthogonalization for proper tangent frame
    let T = normalize(normal_matrix * in.vert_tangent.xyz);
    let N = normalize(normal_matrix * in.normal);
    out.world_normal = N;

    let handedness = in.vert_tangent.w;
    out.world_tangent = normalize(T - dot(T, N) * N);
    out.world_bitangent = normalize(cross(N, out.world_tangent) * handedness);

    out.instance_idx = instance_idx;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let obj = objects[in.instance_idx];

    let albedo_idx = obj.texture_indices.x;
    let normal_idx = obj.texture_indices.y;
    let mr_idx = obj.texture_indices.z;
    let ao_idx = obj.texture_indices.w;
    let emission_idx = u32(obj.material_params.w);

    let albedo_sample = sample_texture(albedo_idx, in.tex_coords);
    let albedo = albedo_sample.rgb * obj.base_color.rgb;
    let alpha = albedo_sample.a * obj.base_color.a;

    let normal_sample = sample_texture(normal_idx, in.tex_coords);

    let unpacked = normal_sample.xyz * 2.0 - 1.0;
    let tangent_normal = vec3f(unpacked.x, unpacked.y, unpacked.z);

    let T = normalize(in.world_tangent);
    let B = normalize(in.world_bitangent);
    let N = normalize(in.world_normal);
    let TBN = mat3x3f(T, B, N);

    let final_normal = normalize(TBN * tangent_normal);

    let mr_sample = sample_texture(mr_idx, in.tex_coords);
    let roughness = max(mr_sample.g * obj.material_params.y, 0.04);
    let metallic = mr_sample.b * obj.material_params.x;

    let ao_sample = sample_texture(ao_idx, in.tex_coords);
    let ao = ao_sample.r * obj.material_params.z;

    let V = normalize(frame_data.camera_position.xyz - in.world_pos);

    let F0 = mix(vec3f(0.04), albedo, metallic);

    let roughness_sq = roughness * roughness;

    // Directional light (sun)
    let L_sun = normalize(frame_data.light_direction.xyz);
    let radiance_sun = frame_data.light_color.rgb * frame_data.light_intensity.x;

    let F_sun = fresnel_schlick(max(dot(normalize(V + L_sun), V), 0.0), F0);
    let kS = F_sun;
    let kD = (1.0 - kS) * (1.0 - metallic);
    let diffuse = kD * albedo / PI;

    let Lo_sun = pbr_direct_light(final_normal, V, L_sun, F0, roughness_sq, kD, diffuse, radiance_sun);

    // Point lights (Forward+ tile culling)
    let Lo_point = accumulate_point_lights(
        in.clip_position, in.world_pos,
        final_normal, V, F0,
        roughness_sq, kD, diffuse,
        frame_data.tiles.x, frame_data.tiles.y,
        tile_light_counts, tile_light_indices, point_lights,
    );

    let Lo = Lo_sun + Lo_point;

    let ambient = vec3f(0.03) * albedo * ao;

    var emission = vec3f(0.0);
    if (emission_idx > 0u) {
        let emission_sample = sample_texture(emission_idx, in.tex_coords);
        emission = emission_sample.rgb;
    }

    let color = ambient + Lo + emission;

    return vec4f(color, alpha);
}
