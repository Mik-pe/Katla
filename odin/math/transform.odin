// TRS uses T * R * S; hierarchy composition returns matrices to retain shear.
package katla_math

/// Position, component scale and xyzw unit rotation.
Transform :: struct { position: Vec3, scale: Vec3, rotation: Quat }
TRANSFORM_IDENTITY :: Transform{VEC3_ZERO,VEC3_ONE,QUAT_IDENTITY}

/// Construct with identity defaults, rather than Odin's all-zero defaults.
transform :: #force_inline proc (position := VEC3_ZERO, rotation := QUAT_IDENTITY, scale := VEC3_ONE) -> Transform { return {position,scale,rotation} }
/// Exact T * R * S matrix used by rendering and bounds.
mat4_trs :: #force_inline proc (position: Vec3, rotation: Quat, scale: Vec3) -> Mat4 { return matrix_mul(mat4_translation(position),matrix_mul(quat_to_mat4(rotation),mat4_scale(scale))) }
/// Compose the transform into an affine matrix.
transform_to_mat4 :: #force_inline proc (t: Transform) -> Mat4 { return mat4_trs(t.position,t.rotation,t.scale) }
/// Identity test follows Rust's quaternion sign and epsilon rules.
transform_is_identity :: proc (t: Transform) -> bool {
    if !is_zero(t.position) || !quat_is_normalized(t.rotation) { return false }
    for i in 0..<3 { if abs(t.scale[i]-1) >= EPSILON || abs(t.rotation[i]) >= EPSILON { return false } }
    return abs(t.rotation[3]-1) < EPSILON
}
/// Scale locally, rotate, then translate, consistently with transform_to_mat4.
transform_vector :: #force_inline proc(t: Transform,v: Vec3) -> Vec3 { return t.position+quat_transform_vector(t.rotation,t.scale*v) }
/// Exact parent * child composition, retaining shear in a matrix.
transform_compose :: #force_inline proc(parent,child: Transform) -> Mat4 { return matrix_mul(transform_to_mat4(parent),transform_to_mat4(child)) }
/// Exact matrix inverse; zero scale reports ok=false.
transform_inverse :: #force_inline proc(t: Transform) -> (Mat4,bool) { return inverse(transform_to_mat4(t)) }
/// Unclamped position/scale lerp with quaternion slerp.
transform_lerp :: #force_inline proc (a,b: Transform,t: f32) -> Transform { return {lerp(a.position,b.position,t),lerp(a.scale,b.scale,t),quat_slerp(a.rotation,b.rotation,t)} }
/// Approximate positive-scale decomposition; singular scale returns ok=false.
mat4_decompose_approx :: proc (a: Mat4) -> (Transform,bool) {
    scale := mat4_extract_scale(a)
    for v in scale { if v == 0 { return {},false } }
    rot := Mat3{xyz(a[0])/scale[0],xyz(a[1])/scale[1],xyz(a[2])/scale[2]}
    return {xyz(a[3]),scale,quat_from_mat3(rot)},true
}
/// Camera-to-world rotation at origin with unit scale; forward is local -Z.
transform_look_direction :: proc(direction,up: Vec3) -> Transform {
    return transform(rotation=quat_from_mat4(mat4_lookat(VEC3_ZERO,direction,up)))
}
/// Preserve position and scale while orienting local -Z toward the target.
transform_look_at :: proc(t: Transform,target,up: Vec3) -> Transform {
    result := t
    result.rotation = transform_look_direction(target-t.position,up).rotation
    return result
}
/// Rotated -Z direction.
transform_forward :: #force_inline proc (t: Transform) -> Vec3 { return quat_transform_vector(t.rotation,-VEC3_Z) }
/// Rotated +Y direction.
transform_up :: #force_inline proc (t: Transform) -> Vec3 { return quat_transform_vector(t.rotation,VEC3_Y) }
/// Rotated +X direction.
transform_right :: #force_inline proc (t: Transform) -> Vec3 { return quat_transform_vector(t.rotation,VEC3_X) }
