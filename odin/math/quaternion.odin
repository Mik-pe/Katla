// Unit quaternion rotations stored in xyzw order.
package katla_math

import m "core:math"

/// Distinct from Vec4; construction does not imply normalization.
Quat :: distinct [4]f32
QUAT_IDENTITY :: Quat{0,0,0,1}

/// Unit-length check with Rust's quaternion threshold.
quat_is_normalized :: #force_inline proc (q: Quat) -> bool { return abs(1-dot(Vec4(q),Vec4(q))) < 0.001 }
/// Normalize a quaternion, leaving zero unchanged.
quat_normalize :: #force_inline proc (q: Quat) -> Quat { return Quat(normalize(Vec4(q))) }
/// Unit quaternion inverse (conjugate); no division by magnitude.
quat_conjugate :: #force_inline proc (q: Quat) -> Quat { return {-q[0],-q[1],-q[2],q[3]} }
/// Quaternion dot product.
quat_dot :: #force_inline proc (a,b: Quat) -> f32 { return dot(Vec4(a),Vec4(b)) }
/// Hamilton product. Both inputs must be normalized.
quat_mul :: #force_inline proc (a,b: Quat) -> Quat {
    assert(quat_is_normalized(a) && quat_is_normalized(b))
    return {a[3]*b[0]+a[0]*b[3]+a[1]*b[2]-a[2]*b[1], a[3]*b[1]-a[0]*b[2]+a[1]*b[3]+a[2]*b[0], a[3]*b[2]+a[0]*b[1]-a[1]*b[0]+a[2]*b[3], a[3]*b[3]-a[0]*b[0]-a[1]*b[1]-a[2]*b[2]}
}
/// Matches Rust's Quat * Vec3 operator; requires a unit quaternion for rotation.
quat_transform_vector :: #force_inline proc (q: Quat, v: Vec3) -> Vec3 {
    assert(quat_is_normalized(q))
    u := xyz(Vec4(q))
    t := 2 * cross(u,v)
    return v + q[3]*t + cross(u,t)
}
/// Matches Rust rotate_vec3, including its non-unit input behavior.
quat_rotate_vec3 :: #force_inline proc (q: Quat, v: Vec3) -> Vec3 {
    u := xyz(Vec4(q))
    return 2*dot(u,v)*u + (q[3]*q[3]-dot(u,u))*v + 2*q[3]*cross(u,v)
}
/// Normalizes axis and output; a zero axis represents no rotation.
quat_axis_angle :: #force_inline proc (axis: Vec3, angle: f32) -> Quat {
    if is_zero(axis) { return QUAT_IDENTITY }
    return quat_normalize(Quat(vec4(normalize(axis)*m.sin(angle/2),m.cos(angle/2))))
}
/// Shortest rotation; opposite vectors select an orthogonal fallback axis.
quat_rotation_between :: proc (from,to: Vec3) -> Quat {
    a,b := normalize(from),normalize(to)
    d := dot(a,b)
    if d >= 0.99999 { return QUAT_IDENTITY }
    if d <= -0.99999 {
        axis := cross(VEC3_X,a)
        if dot(axis,axis) < 0.0001 { axis = cross(VEC3_Y,a) }
        return Quat(vec4(normalize(axis)))
    }
    return quat_axis_angle(normalize(cross(a,b)),m.acos(d))
}
/// Shortest-path normalized interpolation.
quat_nlerp :: proc (a,b: Quat,t: f32) -> Quat {
    aa,bb := quat_normalize(a),quat_normalize(b)
    if quat_dot(aa,bb) < 0 { bb = -bb }
    return quat_normalize((1-t)*aa+t*bb)
}
/// Shortest-path spherical interpolation; near-equal inputs retain the first.
quat_slerp :: proc (a,b: Quat,t: f32) -> Quat {
    aa,bb := quat_normalize(a),quat_normalize(b)
    cs := quat_dot(aa,bb)
    if cs < 0 { bb = -bb; cs = -cs }
    angle := m.acos(clamp(cs,-1,1))
    if abs(angle) >= 0.001 {
        inv_sin := 1/m.sin(angle)
        return quat_normalize(aa*(m.sin(angle-t*angle)*inv_sin)+bb*(m.sin(t*angle)*inv_sin))
    }
    return aa
}
/// Rotation matrix columns matching quaternion-vector rotation.
quat_to_mat3 :: proc (q: Quat) -> Mat3 {
    assert(quat_is_normalized(q))
    x,y,z,w := q[0],q[1],q[2],q[3]
    return {{1-2*(y*y+z*z),2*(x*y+w*z),2*(x*z-w*y)}, {2*(x*y-w*z),1-2*(x*x+z*z),2*(y*z+w*x)}, {2*(x*z+w*y),2*(y*z-w*x),1-2*(x*x+y*y)}}
}
/// Homogeneous rotation matrix.
quat_to_mat4 :: #force_inline proc (q: Quat) -> Mat4 { return mat3_to_mat4(quat_to_mat3(q)) }
/// Normalized quaternion from rotation columns, including 180-degree branches.
quat_from_mat3 :: proc (a: Mat3) -> Quat {
    trace := a[0][0]+a[1][1]+a[2][2]
    q: Quat
    if trace > 0 {
        s := m.sqrt(trace+1)*2
        q = {(a[1][2]-a[2][1])/s,(a[2][0]-a[0][2])/s,(a[0][1]-a[1][0])/s,0.25*s}
    } else if a[0][0] > a[1][1] && a[0][0] > a[2][2] {
        s := m.sqrt(1+a[0][0]-a[1][1]-a[2][2])*2
        q = {0.25*s,(a[1][0]+a[0][1])/s,(a[2][0]+a[0][2])/s,(a[1][2]-a[2][1])/s}
    } else if a[1][1] > a[2][2] {
        s := m.sqrt(1+a[1][1]-a[0][0]-a[2][2])*2
        q = {(a[1][0]+a[0][1])/s,0.25*s,(a[2][1]+a[1][2])/s,(a[2][0]-a[0][2])/s}
    } else {
        s := m.sqrt(1+a[2][2]-a[0][0]-a[1][1])*2
        q = {(a[2][0]+a[0][2])/s,(a[2][1]+a[1][2])/s,0.25*s,(a[0][1]-a[1][0])/s}
    }
    return quat_normalize(q)
}
/// Extract the rotation part before conversion.
quat_from_mat4 :: #force_inline proc (a: Mat4) -> Quat { return quat_from_mat3(mat4_to_mat3(a)) }
