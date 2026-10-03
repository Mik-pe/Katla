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
#include <tangent_frame.wgsl>
#include <material_surface.wgsl>
#include <shadow_sampling.wgsl>

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

@group(0) @binding(2)
var<storage, read> surfaces: array<SurfaceParameters>;

// Set 3: Forward+ light culling data
@group(3) @binding(0)
var<storage, read> point_lights: array<PointLightGPU, MAX_POINT_LIGHTS>;

@group(3) @binding(1)
var<storage, read> tile_light_indices: array<u32>;

@group(3) @binding(2)
var<storage, read> tile_light_counts: array<u32>;

fn accumulate_point_lights(
    clip_position: vec4f,
    world_pos: vec3f,
    N: vec3f, V: vec3f, albedo: vec3f,
    metallic: f32, roughness: f32,
) -> vec3f {
    let tiles_x = frame_data.tiles.x;
    let tiles_y = frame_data.tiles.y;
    let pixel_x = max(u32(clip_position.x), 0u);
    let pixel_y = max(u32(clip_position.y), 0u);
    let tile = vec2<u32>(
        pixel_x / TILE_SIZE,
        pixel_y / TILE_SIZE
    );
    let tile_idx = tile.y * tiles_x + tile.x;

    var Lo_point = vec3f(0.0);

    if (tile.x < tiles_x && tile.y < tiles_y) {
        let light_count = tile_light_counts[tile_idx];
        let base_offset = tile_idx * MAX_LIGHTS_PER_TILE;

        for (var i = 0u; i < light_count; i++) {
            let light_idx = tile_light_indices[base_offset + i];
            if (light_idx >= MAX_POINT_LIGHTS) {
                break;
            }

            let light = point_lights[light_idx];
            let to_light = light.position - world_pos;
            let dist = length(to_light);
            let L_pt = to_light / max(dist, 0.001);

            if (dist > light.range) {
                continue;
            }
            let attenuation = 1.0 - (dist / light.range);
            let atten = attenuation * attenuation;

            let radiance_pt = light.color * light.intensity * atten;
            Lo_point += pbr_direct_light(N, V, L_pt, albedo, metallic, roughness, radiance_pt);
        }
    }

    return Lo_point;
}

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

    let surface_frame = transformed_tangent_frame(
        mat3x3f(obj.model[0].xyz, obj.model[1].xyz, obj.model[2].xyz),
        in.normal, in.vert_tangent,
    );
    out.world_normal = surface_frame.normal;
    out.world_tangent = surface_frame.tangent;
    out.world_bitangent = surface_frame.bitangent;

    out.instance_idx = instance_idx;

    return out;
}

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4f {
    let obj = objects[in.instance_idx];
    let surface = surfaces[in.instance_idx];

    let albedo_idx = obj.texture_indices.x;
    let normal_idx = obj.texture_indices.y;
    let mr_idx = obj.texture_indices.z;
    let ao_idx = obj.texture_indices.w;
    let emission_idx = u32(obj.material_params.w);

    let albedo_sample = sample_texture(albedo_idx, in.tex_coords);
    let albedo = albedo_sample.rgb * obj.base_color.rgb;
    let alpha = surface_alpha(albedo_sample.a * obj.base_color.a, surface);

    let normal_sample = sample_texture(normal_idx, in.tex_coords);

    let tangent_normal = surface_tangent_normal(normal_sample.xyz, surface.normal_occlusion.x);

    let T = normalize(in.world_tangent);
    let B = normalize(in.world_bitangent);
    let N = normalize(in.world_normal);
    let TBN = mat3x3f(T, B, N);

    let final_normal = surface_face_normal(normalize(TBN * tangent_normal), front_facing, surface);

    let mr_sample = sample_texture(mr_idx, in.tex_coords);
    let roughness = max(mr_sample.g * obj.material_params.y, 0.04);
    let metallic = mr_sample.b * obj.material_params.x;

    let ao_sample = sample_texture(ao_idx, in.tex_coords);
    let ao = surface_occlusion(ao_sample.r, surface.normal_occlusion.y, obj.material_params.z);

    let V = normalize(frame_data.camera_position.xyz - in.world_pos);

    // Directional light (sun)
    let L_sun = normalize(frame_data.light_direction.xyz);
    let radiance_sun = frame_data.light_color.rgb * frame_data.light_intensity.x;

    let Lo_sun = pbr_direct_light(final_normal, V, L_sun, albedo, metallic, roughness, radiance_sun);

    // Shadow visibility for directional light
    let view_z = -(frame_data.view * vec4f(in.world_pos, 1.0)).z;
    let shadow_visibility = sample_shadow(in.world_pos, view_z);

    // Point lights (Forward+ tile culling)
    let Lo_point = accumulate_point_lights(
        in.clip_position, in.world_pos,
        final_normal, V, albedo,
        metallic, roughness,
    );

    let Lo = Lo_sun * shadow_visibility + Lo_point;

    // Ambient (SSAO and contact shadows disabled - require separate pass for correct depth buffer layout)
    let ambient = vec3f(0.15) * albedo * ao;

    let emission = surface_emission(sample_texture(emission_idx, in.tex_coords).rgb, surface.emissive.rgb);

    let color = ambient + Lo + emission;

    return vec4f(color, alpha);
}
