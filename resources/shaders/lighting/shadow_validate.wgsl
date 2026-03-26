// Shadow sampling validation compute shader.
//
// Validates the shadow cascade data and depth texture by performing
// a manual shadow comparison (equivalent to textureSampleCompare).
//
// Divergences from production shadow_sampling.wgsl (compute shader
// restrictions prevent full parity):
//   - Single-point textureLoad instead of 16-sample PCF Poisson disc.
//     PCF requires textureSampleCompare which is fragment-only.
//   - These divergences are acceptable for validating the core depth
//     comparison and cascade selection logic.
//
// Two entry points:
//   cs_main          - single-point sampling, no cascade blending
//   cs_main_blended  - single-point sampling with 5% cascade blend zone
//
// Descriptor layout:
//   @group(0) @binding(0)  shadow_data: storage buffer (ShadowFrameData)
//   @group(0) @binding(1)  shadow_atlas: depth texture 2d
//   @group(0) @binding(2)  output_data: storage buffer (array<f32>)
//   @group(0) @binding(3)  test_params: uniform buffer (TestParams)

const NUM_CASCADES: u32 = 4u;

struct ShadowCascadeData {
    view_proj: mat4x4f,
    split_distance: f32,
    texel_size: f32,
    _pad: vec2f,
}

struct ShadowFrameData {
    cascades: array<ShadowCascadeData, 4>,
    light_direction: vec4f,
    shadow_bias: vec4f,
}

struct TestParams {
    test_world_pos: vec3f,
    test_view_z: f32,
    test_index: u32,
    use_blending: u32,
    _pad: vec2u,
}

@group(0) @binding(0)
var<storage, read> shadow_data: ShadowFrameData;

@group(0) @binding(1)
var shadow_atlas: texture_depth_2d;

@group(0) @binding(2)
var<storage, read_write> output_data: array<f32>;

@group(0) @binding(3)
var<uniform> test_params: TestParams;

fn cascade_uv_offset_scale(cascade_idx: u32) -> vec4f {
    let col = f32(cascade_idx % 2u);
    let row = 1.0 - f32(cascade_idx / 2u);
    return vec4f(col * 0.5, row * 0.5, 0.5, 0.5);
}

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

fn sample_shadow_manual(world_pos: vec3f, view_z: f32) -> f32 {
    let num_cascades = u32(shadow_data.light_direction.w);
    if (num_cascades == 0u) {
        return 1.0;
    }

    let cascade_idx = select_cascade(view_z);
    let cascade = shadow_data.cascades[cascade_idx];

    let light_space = cascade.view_proj * vec4f(world_pos, 1.0);
    let proj = light_space.xyz / light_space.w;

    // Map from NDC [-1,1] to texture [0,1] (same as shadow_sampling.wgsl)
    var uv = proj.xy * 0.5 + 0.5;
    // Map depth from NDC [-1,1] to depth buffer [0,1] for comparison
    let depth = proj.z * 0.5 + 0.5;

    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 1.0;
    }

    // Offset and scale to cascade's region in the atlas
    let atlas = cascade_uv_offset_scale(cascade_idx);
    uv = atlas.xy + uv * atlas.zw;

    let constant_bias = shadow_data.shadow_bias.x;
    let slope_bias = shadow_data.shadow_bias.y;
    let compare_depth = depth - constant_bias - slope_bias;

    // textureLoad for depth texture (compute shader compatible).
    const ATLAS_SIZE: u32 = 256u;
    let coords = vec2i(clamp(vec2f(uv) * vec2f(f32(ATLAS_SIZE)), vec2f(0.0), vec2f(f32(ATLAS_SIZE) - 1.0)));
    let stored_depth = textureLoad(shadow_atlas, coords, 0);

    if (compare_depth <= stored_depth) {
        return 1.0;
    } else {
        return 0.0;
    }
}

fn sample_shadow_blended(world_pos: vec3f, view_z: f32) -> f32 {
    let num_cascades = u32(shadow_data.light_direction.w);
    if (num_cascades == 0u) {
        return 1.0;
    }

    let cascade_idx = select_cascade(view_z);
    let visibility = sample_shadow_manual(world_pos, view_z);

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

    // Force cascade selection to the next cascade by passing a view_z
    // that is guaranteed to select it (midpoint between this split and the next).
    let next_cascade_view_z = (split + next_split) * 0.5;
    let next_visibility = sample_shadow_manual(world_pos, next_cascade_view_z);
    return mix(visibility, next_visibility, blend_factor);
}

@compute @workgroup_size(1, 1, 1)
fn cs_main() {
    var visibility: f32;
    if (test_params.use_blending != 0u) {
        visibility = sample_shadow_blended(test_params.test_world_pos, test_params.test_view_z);
    } else {
        visibility = sample_shadow_manual(test_params.test_world_pos, test_params.test_view_z);
    }
    output_data[test_params.test_index] = visibility;
}

