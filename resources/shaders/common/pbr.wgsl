// Metallic/roughness direct lighting in linear color space.

const PI: f32 = 3.14159265359;

fn fresnel_schlick(cos_theta: f32, F0: vec3f) -> vec3f {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// Authored roughness is perceptual: GGX alpha = roughness squared.
fn distribution_ggx(N: vec3f, H: vec3f, perceptual_roughness: f32) -> f32 {
    let roughness = clamp(perceptual_roughness, 0.04, 1.0);
    let alpha = roughness * roughness;
    let alpha_sq = alpha * alpha;
    let NdotH = clamp(dot(N, H), 0.0, 1.0);
    let NdotH_sq = NdotH * NdotH;
    let denominator = (1.0 - NdotH_sq) + NdotH_sq * alpha_sq;
    return alpha_sq / (PI * denominator * denominator);
}

// Height-correlated Smith GGX visibility, including the 1/(4 NoV NoL) factor.
fn visibility_smith_ggx(NdotV: f32, NdotL: f32, perceptual_roughness: f32) -> f32 {
    let roughness = clamp(perceptual_roughness, 0.04, 1.0);
    let alpha = roughness * roughness;
    let alpha_sq = alpha * alpha;
    let view_mask = NdotL * sqrt(NdotV * NdotV * (1.0 - alpha_sq) + alpha_sq);
    let light_mask = NdotV * sqrt(NdotL * NdotL * (1.0 - alpha_sq) + alpha_sq);
    return 0.5 / max(view_mask + light_mask, 0.00000001);
}

fn pbr_direct_light(
    N: vec3f, V: vec3f, L: vec3f,
    albedo: vec3f, metallic: f32, perceptual_roughness: f32,
    radiance: vec3f,
) -> vec3f {
    let NdotV = clamp(dot(N, V), 0.0, 1.0);
    let NdotL = clamp(dot(N, L), 0.0, 1.0);
    if (NdotV <= 0.0 || NdotL <= 0.0) {
        return vec3f(0.0);
    }
    let H = normalize(V + L);
    let F0 = mix(vec3f(0.04), albedo, metallic);
    let F = fresnel_schlick(max(dot(H, V), 0.0), F0);
    let D = distribution_ggx(N, H, perceptual_roughness);
    let visibility = visibility_smith_ggx(NdotV, NdotL, perceptual_roughness);
    let diffuse = (1.0 - F) * (1.0 - metallic) * albedo / PI;
    let specular = D * visibility * F;
    return (diffuse + specular) * radiance * NdotL;
}
