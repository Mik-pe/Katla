// Matrices store columns: m[column][row]. Points have w=1, directions w=0.
package katla_math

import m "core:math"

Mat2 :: [2]Vec2
Mat3 :: [3]Vec3
Mat4 :: [4]Vec4

/// Explicit identity constructor; Odin zero initialization is not identity.
identity :: proc ($T: typeid/[$N][N]f32) -> T {
    result: T
    for i in 0..<N { result[i][i] = 1 }
    return result
}
/// Matrix product preserving parent * child order.
matrix_mul :: proc (a, b: [$N][N]f32) -> [N][N]f32 {
    result: [N][N]f32
    for c in 0..<N { for r in 0..<N { for k in 0..<N { result[c][r] += a[k][r] * b[c][k] } } }
    return result
}
/// Matrix times column vector.
matrix_vector :: proc (a: [$N][N]f32, v: [N]f32) -> [N]f32 {
    result: [N]f32
    for r in 0..<N { for k in 0..<N { result[r] += a[k][r] * v[k] } }
    return result
}
/// Extract a row without changing storage convention.
extract_row :: proc (a: [$N][N]f32, row: int) -> [N]f32 {
    result: [N]f32
    for c in 0..<N { result[c] = a[c][row] }
    return result
}
/// Mathematical transpose; never required for a backend adaptation.
transpose :: proc (a: [$N][N]f32) -> [N][N]f32 {
    result: [N][N]f32
    for c in 0..<N { for r in 0..<N { result[c][r] = a[r][c] } }
    return result
}
/// Determinant by column cofactor expansion for dimensions 2 through 4.
determinant :: proc (a: [$N][N]f32) -> f32 where N >= 2, N <= 4 {
    when N == 2 { return a[0][0]*a[1][1]-a[1][0]*a[0][1] }
    else {
        result: f32
        for c in 0..<N {
            minor: [N-1][N-1]f32
            dc := 0
            for sc in 0..<N { if sc == c { continue }; for r in 1..<N { minor[dc][r-1] = a[sc][r] }; dc += 1 }
            sign: f32 = 1
            if c % 2 != 0 { sign = -1 }
            result += sign * a[c][0] * determinant(minor)
        }
        return result
    }
}
/// Singular matrices return ok=false; thresholds match each Rust matrix type.
inverse :: proc (a: [$N][N]f32) -> (result: [N][N]f32, ok: bool) where N >= 2, N <= 4 {
    det := determinant(a)
    threshold: f32 = 1e-6
    when N == 2 { threshold = EPSILON }
    if abs(det) < threshold { return {}, false }
    when N == 2 { return {{a[1][1]/det, -a[0][1]/det}, {-a[1][0]/det, a[0][0]/det}}, true }
    else {
        for c in 0..<N { for r in 0..<N {
            minor: [N-1][N-1]f32
            dc := 0
            for sc in 0..<N {
                if sc == r { continue }
                dr := 0
                for sr in 0..<N { if sr == c { continue }; minor[dc][dr] = a[sc][sr]; dr += 1 }
                dc += 1
            }
            sign: f32 = 1
            if (c+r) % 2 != 0 { sign = -1 }
            result[c][r] = sign * determinant(minor) / det
        } }
        return result, true
    }
}
/// Construct Mat2 from row-listed elements into columns.
mat2 :: #force_inline proc (m00, m01, m10, m11: f32) -> Mat2 { return {{m00, m10}, {m01, m11}} }
/// Counter-clockwise 2D rotation.
mat2_rotation :: #force_inline proc (angle: f32) -> Mat2 { return {{m.cos(angle), m.sin(angle)}, {-m.sin(angle), m.cos(angle)}} }
/// Diagonal scale matrix in any vector dimension.
scale_matrix :: proc (scale: [$N]f32) -> [N][N]f32 {
    result: [N][N]f32
    for i in 0..<N { result[i][i] = scale[i] }
    return result
}
/// Translation occupies column three.
mat4_translation :: #force_inline proc (v: Vec3) -> Mat4 { result := identity(Mat4); result[3] = vec4(v, 1); return result }
/// Affine scale with homogeneous identity.
mat4_scale :: #force_inline proc (v: Vec3) -> Mat4 { return {{v[0],0,0,0}, {0,v[1],0,0}, {0,0,v[2],0}, {0,0,0,1}} }
/// Rotation shares quaternion axis normalization and handedness.
mat4_rotaxis :: #force_inline proc(angle: f32, axis: Vec3) -> Mat4 { return quat_to_mat4(quat_axis_angle(axis,angle)) }
/// Right-handed orthographic projection with Vulkan depth [0,1] and Y flip.
mat4_ortho :: proc (left, right, bottom, top, near, far: f32) -> Mat4 {
    return {{2/(right-left),0,0,0}, {0,-2/(top-bottom),0,0}, {0,0,1/(near-far),0}, {-(right+left)/(right-left),(top+bottom)/(top-bottom),near/(near-far),1}}
}
/// Infinite reverse-Z, Vulkan Y flip, near maps to depth 1; FOV is degrees.
mat4_reverse_z :: proc (fov_degrees, aspect, near: f32) -> Mat4 {
    f := 1 / m.tan(fov_degrees * DEG_TO_RAD / 2)
    return {{f/aspect,0,0,0}, {0,-f,0,0}, {0,0,0,-1}, {0,0,near,0}}
}
/// Finite Vulkan perspective depth [0,1]; FOV is degrees.
mat4_perspective :: proc (fov_degrees, aspect, near, far: f32) -> Mat4 {
    f := 1 / m.tan(fov_degrees * DEG_TO_RAD / 2)
    return {{f/aspect,0,0,0}, {0,-f,0,0}, {0,0,far/(near-far),-1}, {0,0,near*far/(near-far),0}}
}
/// Returns camera-to-world, not the view matrix.
mat4_lookat :: proc (from, to, up: Vec3) -> Mat4 {
    forward := normalize(to-from)
    right := normalize(cross(forward, normalize(up)))
    corrected := normalize(cross(right, forward))
    return {vec4(right),vec4(corrected),vec4(-forward),vec4(from,1)}
}
/// Affine point transformation without perspective divide, like Rust Mat4 * Vec3.
transform_point :: #force_inline proc (a: Mat4, v: Vec3) -> Vec3 { return xyz(matrix_vector(a, vec4(v, 1))) }
/// Affine direction transformation without translation.
transform_direction :: #force_inline proc (a: Mat4, v: Vec3) -> Vec3 { return xyz(matrix_vector(a, vec4(v))) }
/// Upper-left columns, excluding homogeneous components.
mat4_to_mat3 :: #force_inline proc (a: Mat4) -> Mat3 { return {xyz(a[0]),xyz(a[1]),xyz(a[2])} }
/// Embed a 3x3 matrix in homogeneous identity.
mat3_to_mat4 :: #force_inline proc (a: Mat3) -> Mat4 { return {vec4(a[0]),vec4(a[1]),vec4(a[2]),{0,0,0,1}} }
/// Flatten columns for shader uploads, without relying on a foreign ABI cast.
mat4_to_array :: proc (a: Mat4) -> [16]f32 {
    result: [16]f32
    for c in 0..<4 { for r in 0..<4 { result[c*4+r] = a[c][r] } }
    return result
}
/// Positive column lengths; sign and shear are not recovered.
mat4_extract_scale :: #force_inline proc (a: Mat4) -> Vec3 { return {length(xyz(a[0])),length(xyz(a[1])),length(xyz(a[2]))} }
/// Translation column.
mat4_extract_translation :: #force_inline proc (a: Mat4) -> Vec3 { return xyz(a[3]) }
/// Rotation angle of a 2D rotation matrix.
mat2_to_rotation :: #force_inline proc (a: Mat2) -> f32 { return m.atan2(a[0][1], a[0][0]) }
/// Diagonal scale extraction, matching Rust.
mat2_to_scale :: #force_inline proc (a: Mat2) -> Vec2 { return {a[0][0],a[1][1]} }
