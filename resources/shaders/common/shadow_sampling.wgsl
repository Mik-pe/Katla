// PCF shadow sampling for cascaded shadow maps.
//
// Uses a shadow atlas (single depth texture) with 4 cascades packed into a 2x2 grid.
// Bind group 4: shadow data, shadow atlas, comparison sampler.

#include <shadow_cascade_data.wgsl>

const NUM_CASCADES: u32 = 4u;
const PCF_SAMPLE_COUNT: u32 = 16u;  // Poisson disc samples

struct ShadowFrameData {
    cascades: array<ShadowCascadeData, 4>,
    light_direction: vec4f,     // xyz = direction, w = num_cascades
    shadow_bias: vec4f,         // x = constant, y = slope, z = normal offset
}

@group(4) @binding(0)
var<storage, read> shadow_data: ShadowFrameData;

@group(4) @binding(1)
var shadow_atlas: texture_depth_2d;

@group(4) @binding(2)
var shadow_sampler: sampler_comparison;

// Poisson disc samples for soft shadow sampling.
// Pre-computed 16-sample disc with good angular distribution.
const POISSON_SAMPLES: array<vec2f, 16> = array<vec2f, 16>(
    vec2f(-0.94201624, -0.39906216),
    vec2f(0.94558609, -0.76890725),
    vec2f(-0.09418410, -0.92938870),
    vec2f(0.34495938, 0.29387760),
    vec2f(-0.91588581, 0.45771432),
    vec2f(-0.81544232, -0.87912464),
    vec2f(-0.38277543, 0.27676845),
    vec2f(0.97484398, 0.75648379),
    vec2f(0.44323325, -0.97511554),
    vec2f(0.53742981, -0.47373420),
    vec2f(-0.26496911, -0.41893023),
    vec2f(0.79197514, 0.19090188),
    vec2f(-0.24188840, 0.99706507),
    vec2f(-0.81409955, 0.91437590),
    vec2f(0.19984126, 0.78641367),
    vec2f(0.14383161, -0.14100790),
);

fn select_cascade(view_z: f32) -> u32 {
    let num_cascades = u32(shadow_data.light_direction.w);
    var selected = num_cascades - 1u;

    for (var i = 0u; i < num_cascades; i++) {
        if (view_z <= shadow_data.cascades[i].split_distance) {
            selected = i;
            break;
        }
    }

    return selected;
}

fn cascade_uv_offset_scale(cascade_idx: u32) -> vec4f {
    // 2x2 grid layout in the shadow atlas:
    //   cascade 0 (near) -> top-left:    offset (0, 0.5),   scale (0.5, 0.5)
    //   cascade 1        -> top-right:   offset (0.5, 0.5),  scale (0.5, 0.5)
    //   cascade 2        -> bottom-left: offset (0, 0),      scale (0.5, 0.5)
    //   cascade 3 (far)  -> bottom-right:offset (0.5, 0),    scale (0.5, 0.5)
    let col = f32(cascade_idx % 2u);
    let row = 1.0 - f32(cascade_idx / 2u);

    return vec4f(col * 0.5, row * 0.5, 0.5, 0.5);
}

fn sample_shadow_pcf(world_pos: vec3f, cascade_idx: u32) -> f32 {
    let cascade = shadow_data.cascades[cascade_idx];

    let light_space = cascade.view_proj * vec4f(world_pos, 1.0);
    let proj = light_space.xyz / light_space.w;

    // Map from [-1,1] to [0,1]
    var uv = proj.xy * 0.5 + 0.5;
    let depth = proj.z;

    // Out-of-bounds check: if UV is outside [0,1], fragment is outside the
    // light frustum for this cascade — return fully lit (1.0).
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 1.0;
    }

    // Offset and scale to cascade's region in the atlas
    let atlas = cascade_uv_offset_scale(cascade_idx);
    uv = atlas.xy + uv * atlas.zw;

    let texel = cascade.texel_size * atlas.zw;
    let constant_bias = shadow_data.shadow_bias.x;

    var visibility = 0.0;

    for (var i = 0u; i < PCF_SAMPLE_COUNT; i++) {
        let offset = POISSON_SAMPLES[i] * texel * 2.0;
        let sample_uv = uv + offset;
        let compare_depth = depth - constant_bias;

        visibility += textureSampleCompare(
            shadow_atlas, shadow_sampler, sample_uv, compare_depth
        );
    }

    return visibility / f32(PCF_SAMPLE_COUNT);
}

fn sample_shadow_cascade_blended(world_pos: vec3f, view_z: f32) -> f32 {
    let cascade_idx = select_cascade(view_z);
    let visibility = sample_shadow_pcf(world_pos, cascade_idx);

    let num_cascades = u32(shadow_data.light_direction.w);
    if (cascade_idx >= num_cascades - 1u) {
        return visibility;
    }

    let split = shadow_data.cascades[cascade_idx].split_distance;
    let next_split = shadow_data.cascades[cascade_idx + 1u].split_distance;
    let range = next_split - split;
    let blend_zone = range * 0.05;

    if (blend_zone <= 0.0 || view_z < split - blend_zone) {
        return visibility;
    }

    let blend_factor = clamp((view_z - (split - blend_zone)) / blend_zone, 0.0, 1.0);
    let next_visibility = sample_shadow_pcf(world_pos, cascade_idx + 1u);

    return mix(visibility, next_visibility, blend_factor);
}

fn sample_shadow(world_pos: vec3f, view_z: f32) -> f32 {
    let num_cascades = u32(shadow_data.light_direction.w);
    if (num_cascades == 0u) {
        return 1.0;
    }

    return sample_shadow_cascade_blended(world_pos, view_z);
}
