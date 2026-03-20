// Shared PBR lighting functions.

const PI: f32 = 3.14159265359;

fn fresnel_schlick(cos_theta: f32, F0: vec3f) -> vec3f {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

fn distribution_ggx(N: vec3f, H: vec3f, roughness_sq: f32) -> f32 {
    let a2 = roughness_sq;
    let NdotH = max(dot(N, H), 0.0);
    let NdotH2 = NdotH * NdotH;

    let num = a2;
    var denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return num / denom;
}

fn geometry_schlick_ggx(NdotV: f32, roughness_sq: f32) -> f32 {
    let r = roughness_sq + 1.0;
    let k = (r * r) / 8.0;

    let num = NdotV;
    let denom = NdotV * (1.0 - k) + k;

    return num / denom;
}

fn geometry_smith(N: vec3f, V: vec3f, L: vec3f, roughness_sq: f32) -> f32 {
    let NdotV = max(dot(N, V), 0.0);
    let NdotL = max(dot(N, L), 0.0);
    let ggx1 = geometry_schlick_ggx(NdotV, roughness_sq);
    let ggx2 = geometry_schlick_ggx(NdotL, roughness_sq);

    return ggx1 * ggx2;
}

fn sample_texture(idx: u32, coords: vec2f) -> vec4f {
    return textureSample(bindless_textures[idx], shared_sampler, coords);
}

// Compute PBR direct lighting for a single light direction.
// Returns Lo (radiance contribution) for the given light.
fn pbr_direct_light(
    N: vec3f, V: vec3f, L: vec3f, F0: vec3f,
    roughness_sq: f32, kD: vec3f, diffuse: vec3f,
    radiance: vec3f,
) -> vec3f {
    let H = normalize(V + L);
    let D = distribution_ggx(N, H, roughness_sq);
    let G = geometry_smith(N, V, L, roughness_sq);
    let F = fresnel_schlick(max(dot(H, V), 0.0), F0);
    let numerator = D * G * F;
    let NdotL = max(dot(N, L), 0.0);
    let denominator = 4.0 * max(dot(N, V), 0.0) * NdotL + 0.0001;
    let specular = numerator / denominator;

    return (diffuse + specular) * radiance * NdotL;
}

// Accumulate Forward+ point lights for the current tile.
fn accumulate_point_lights(
    clip_position: vec4f,
    world_pos: vec3f,
    N: vec3f, V: vec3f, F0: vec3f,
    roughness_sq: f32, kD: vec3f, diffuse: vec3f,
    tiles_x: u32, tiles_y: u32,
    tile_light_counts: array<u32>,
    tile_light_indices: array<u32>,
    point_lights: array<PointLightGPU>,
) -> vec3f {
    let pixel_x = max(u32(clip_position.x), 0u);
    let pixel_y = max(u32(clip_position.y), 0u);
    let tile = vec2<u32>(
        pixel_x / TILE_SIZE,
        pixel_y / TILE_SIZE
    );
    let tile_idx = tile.y * tiles_x + tile.x;

    var Lo_point = vec3f(0.0);

    if (tile.x < tiles_x && tile.y < tiles_y && tile_idx < arrayLength(&tile_light_counts)) {
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
            Lo_point += pbr_direct_light(N, V, L_pt, F0, roughness_sq, kD, diffuse, radiance_pt);
        }
    }

    return Lo_point;
}
