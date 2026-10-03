// Numeric value types and vector operations. Angles are radians unless named otherwise.
package katla_math

import m "core:math"

/// Compact scalar vectors; these are values, not Rust ABI representations.
Vec2 :: [2]f32
Vec3 :: [3]f32
Vec4 :: [4]f32

PI: f32 : 3.141592653589793
TAU: f32 : 2 * PI
FRAC_PI_2: f32 : PI / 2
FRAC_PI_3: f32 : PI / 3
FRAC_PI_4: f32 : PI / 4
FRAC_PI_6: f32 : PI / 6
DEG_TO_RAD: f32 : PI / 180
RAD_TO_DEG: f32 : 180 / PI
GOLDEN_RATIO: f32 : 1.618033988749895
SQRT_3: f32 : 1.7320508
EPSILON: f32 : 1.1920929e-7
VEC2_ZERO :: Vec2{0, 0}
VEC2_ONE :: Vec2{1, 1}
VEC2_X :: Vec2{1, 0}
VEC2_Y :: Vec2{0, 1}
VEC3_ZERO :: Vec3{0, 0, 0}
VEC3_ONE :: Vec3{1, 1, 1}
VEC3_X :: Vec3{1, 0, 0}
VEC3_Y :: Vec3{0, 1, 0}
VEC3_Z :: Vec3{0, 0, 1}
VEC4_ZERO :: Vec4{0, 0, 0, 0}
VEC4_ONE :: Vec4{1, 1, 1, 1}
VEC4_X :: Vec4{1, 0, 0, 0}
VEC4_Y :: Vec4{0, 1, 0, 0}
VEC4_Z :: Vec4{0, 0, 1, 0}
VEC4_W :: Vec4{0, 0, 0, 1}

/// Dot product for scalar f32 vectors.
dot :: proc (a, b: [$N]f32) -> f32 {
    result: f32
    for i in 0..<N { result += a[i] * b[i] }
    return result
}
/// Squared Euclidean length.
length_squared :: #force_inline proc (v: [$N]f32) -> f32 { return dot(v, v) }
/// Euclidean length.
length :: #force_inline proc (v: [$N]f32) -> f32 { return m.sqrt(dot(v, v)) }
/// Zero vectors remain zero.
normalize :: #force_inline proc (v: [$N]f32) -> [N]f32 {
    n := length(v)
    if n == 0 { return {} }
    return v / n
}
/// Uses the same f32 epsilon threshold as Rust scalar vectors.
is_normalized :: #force_inline proc (v: [$N]f32) -> bool { return abs(dot(v, v) - 1) < EPSILON }
/// Exact zero test, including negative zero.
is_zero :: #force_inline proc (v: [$N]f32) -> bool { return v == [N]f32{} }
/// Unclamped linear interpolation.
lerp :: #force_inline proc (a, b: [$N]f32, t: f32) -> [N]f32 { return a + (b - a) * t }
/// Euclidean separation.
distance :: #force_inline proc (a, b: [$N]f32) -> f32 { return length(a - b) }
/// Squared Euclidean separation.
distance_squared :: #force_inline proc (a, b: [$N]f32) -> f32 { return dot(a - b, a - b) }
/// Right-handed cross product.
cross :: #force_inline proc (a, b: Vec3) -> Vec3 { return {a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]} }
/// Signed 2D cross product.
cross2 :: #force_inline proc (a, b: Vec2) -> f32 { return a[0]*b[1]-a[1]*b[0] }
/// Counter-clockwise perpendicular.
perpendicular :: #force_inline proc (v: Vec2) -> Vec2 { return {-v[1], v[0]} }
/// Angle from +X in radians.
angle :: #force_inline proc (v: Vec2) -> f32 { return m.atan2(v[1], v[0]) }
/// Unit vector at a radian angle.
from_angle :: #force_inline proc (a: f32) -> Vec2 { return {m.cos(a), m.sin(a)} }
/// Reflect across a unit normal.
reflect :: #force_inline proc (v, normal: Vec3) -> Vec3 { return v - normal * (2 * dot(v, normal)) }
/// Project onto a nonzero vector; a zero target retains Rust's NaN result.
project :: #force_inline proc (v, onto: Vec3) -> Vec3 { return onto * (dot(v, onto) / dot(onto, onto)) }
/// Component perpendicular to a nonzero vector.
reject :: #force_inline proc (v, from: Vec3) -> Vec3 { return v - project(v, from) }
/// Unsigned angle, including the Rust zero-vector atan2 behavior.
angle_between :: #force_inline proc (a, b: Vec3) -> f32 { return m.atan2(length(cross(a, b)), dot(a, b)) }
/// Negative maximum lengths produce zero.
clamp_length :: #force_inline proc (v: Vec3, maximum: f32) -> Vec3 {
    if maximum < 0 { return {} }
    n := length(v)
    if n > maximum { return v * (maximum / n) }
    return v
}
/// Invalid ranges and zero vectors retain the Rust behavior.
clamp_length_min_max :: #force_inline proc (v: Vec3, minimum, maximum: f32) -> Vec3 {
    if maximum < minimum { return {} }
    n := length(v)
    if n < minimum { if n > 0 { return v * (minimum / n) }; return {} }
    if n > maximum { return v * (maximum / n) }
    return v
}
/// Polar angle phi measured from +Y; azimuth theta measured from +X toward +Z.
from_spherical :: #force_inline proc (phi, theta: f32) -> Vec3 { return {m.sin(phi)*m.cos(theta), m.cos(phi), m.sin(phi)*m.sin(theta)} }
/// Drop the homogeneous component.
xyz :: #force_inline proc (v: Vec4) -> Vec3 { return {v[0], v[1], v[2]} }
/// Extend a vector with an explicit homogeneous component.
vec4 :: #force_inline proc (v: Vec3, w: f32 = 0) -> Vec4 { return {v[0], v[1], v[2], w} }
