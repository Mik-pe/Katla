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
