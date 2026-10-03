// Affine surface frames shared by static and skinned model shaders.

struct TangentFrame {
    normal: vec3f,
    tangent: vec3f,
    bitangent: vec3f,
}

fn normalized_or(vector: vec3f, fallback: vec3f) -> vec3f {
    let magnitude_sq = dot(vector, vector);
    if (magnitude_sq > 0.000000000001) {
        return vector * inverseSqrt(magnitude_sq);
    }
    return fallback;
}

fn transformed_tangent_frame(transform: mat3x3f, normal: vec3f, tangent: vec4f) -> TangentFrame {
    let cofactors = mat3x3f(
        cross(transform[1], transform[2]),
        cross(transform[2], transform[0]),
        cross(transform[0], transform[1]),
    );
    let orientation = select(1.0, -1.0, dot(transform[0], cofactors[0]) < 0.0);
    let N = normalized_or(cofactors * normal * orientation,
        normalized_or(transform * normal, vec3f(0.0, 1.0, 0.0)));
    let transformed_tangent = transform * tangent.xyz;
    let axis = select(vec3f(1.0, 0.0, 0.0), vec3f(0.0, 0.0, 1.0), abs(N.x) > 0.9);
    let fallback_tangent = normalize(cross(axis, N));
    let T = normalized_or(transformed_tangent - N * dot(N, transformed_tangent), fallback_tangent);
    let B = cross(N, T) * tangent.w * orientation;
    return TangentFrame(N, T, B);
}
