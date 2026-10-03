// Application-authored surface layout; object slots match ObjectUniforms.
struct TextureCoordinates {
    matrix: vec4f,
    offset_set: vec4f,
}
struct SurfaceParameters {
    emissive: vec4f,
    normal_occlusion: vec4f,
    coverage: vec4f,
    coordinates: array<TextureCoordinates, 5>,
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

fn material_uv(surface: SurfaceParameters, role: u32, uv0: vec2f, uv1: vec2f) -> vec2f {
    let coordinates = surface.coordinates[role];
    let uv = select(uv0, uv1, coordinates.offset_set.z > 0.5);
    return mat2x2f(coordinates.matrix.xy, coordinates.matrix.zw) * uv + coordinates.offset_set.xy;
}

fn material_tangent_basis(position: vec3f, uv: vec2f, normal: vec3f, tangent: vec3f, bitangent: vec3f, regenerate: bool) -> mat3x3f {
    let dx = dpdx(position);
    let dy = dpdy(position);
    let ux = dpdx(uv);
    let uy = dpdy(uv);
    let determinant = ux.x * uy.y - ux.y * uy.x;
    let n = normalize(normal);
    var t = normalize(tangent);
    var b = normalize(bitangent);
    if (regenerate && abs(determinant) > 1e-12) {
        let direction = (dx * uy.y - dy * ux.y) / determinant;
        let orthogonal = direction - n * dot(n, direction);
        if (dot(orthogonal, orthogonal) > 1e-12) {
            t = normalize(orthogonal);
            let bitangent_direction = (dy * ux.x - dx * uy.x) / determinant;
            b = cross(n, t) * select(-1.0, 1.0, dot(cross(n, t), bitangent_direction) >= 0.0);
        }
    }
    return mat3x3f(t, b, n);
}
