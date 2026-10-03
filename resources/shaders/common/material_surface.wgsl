// Application-authored surface layout; object slots match ObjectUniforms.
struct SurfaceParameters {
    emissive: vec4f,
    normal_occlusion: vec4f,
    coverage: vec4f,
}

fn surface_tangent_normal(sample: vec3f, scale: f32) -> vec3f {
    let unpacked = sample * 2.0 - 1.0;
    let scaled = vec3f(unpacked.xy * scale, unpacked.z);
    let length_squared = dot(scaled, scaled);
    if (length_squared < 1e-12) { return vec3f(0.0, 0.0, 1.0); }
    return scaled * inverseSqrt(length_squared);
}

fn surface_occlusion(sample: f32, strength: f32, multiplier: f32) -> f32 {
    return mix(1.0, sample, strength) * multiplier;
}

fn surface_emission(sample: vec3f, factor: vec3f) -> vec3f {
    return sample * factor;
}

fn surface_alpha(sampled_alpha: f32, surface: SurfaceParameters) -> f32 {
    if (surface.coverage.y < 0.5) { return 1.0; }
    if (surface.coverage.y < 1.5) {
        if (sampled_alpha < surface.coverage.x) { discard; }
        return 1.0;
    }
    if (sampled_alpha <= 0.0) { discard; }
    return clamp(sampled_alpha, 0.0, 1.0);
}

fn surface_face_normal(normal: vec3f, front_facing: bool, surface: SurfaceParameters) -> vec3f {
    let mirrored = surface.coverage.w > 0.5;
    if (surface.coverage.z > 0.5 && front_facing == mirrored) { return -normal; }
    return normal;
}
