//! SSE-accelerated implementation of Mat4
//!
//! This uses SSE intrinsics for high-performance matrix operations.
//! Only available on x86/x86_64 with SSE2 support.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;

use crate::Quat;
use crate::Transform;
use crate::Vec3;
use crate::Vec4;
use core::ops::{Index, Mul, MulAssign};

// SSE shuffle control masks for _mm_shuffle_ps(dest, src, mask)
// Mask format: [dest[3] dest[2] src[1] src[0]] where each nibble selects an element
const SHUFFLE_X: i32 = 0b00_00_00_00; // Broadcast element 0 (x)
const SHUFFLE_Y: i32 = 0b01_01_01_01; // Broadcast element 1 (y)
const SHUFFLE_Z: i32 = 0b10_10_10_10; // Broadcast element 2 (z)
const SHUFFLE_W: i32 = 0b11_11_11_11; // Broadcast element 3 (w)

#[derive(Debug, Clone, PartialEq)]
pub struct Mat4(pub [Vec4; 4]);

impl Index<usize> for Mat4 {
    type Output = Vec4;

    fn index(&self, index: usize) -> &Vec4 {
        match index {
            0 => &self.0[0],
            1 => &self.0[1],
            2 => &self.0[2],
            3 => &self.0[3],
            _ => panic!("INDEXING OUT_OF_BOUNDS in Mat4"),
        }
    }
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::new()
    }
}

impl Mat4 {
    pub fn new() -> Mat4 {
        Mat4([
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }

    pub fn from_translation(pos: [f32; 3]) -> Mat4 {
        Mat4([
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(pos[0], pos[1], pos[2], 1.0),
        ])
    }

    /// SIMD-optimized row extraction using shuffle operations
    pub fn extract_row(&self, index: usize) -> Vec4 {
        unsafe {
            match index {
                0 => {
                    // Extract x component from each column
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_X);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_X);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_X);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_X);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                1 => {
                    // Extract y component from each column
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_Y);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_Y);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_Y);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_Y);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                2 => {
                    // Extract z component from each column
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_Z);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_Z);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_Z);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_Z);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                3 => {
                    // Extract w component from each column
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_W);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_W);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_W);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_W);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                _ => panic!("INDEXING OUT_OF_BOUNDS in Mat4"),
            }
        }
    }

    pub fn from_rotaxis(angle: &f32, axis: [f32; 3]) -> Mat4 {
        let cos_part = angle.cos();
        let sin_part = angle.sin();
        let one_sub_cos = 1.0 - cos_part;
        Mat4([
            Vec4::new(
                one_sub_cos * axis[0] * axis[0] + cos_part,
                one_sub_cos * axis[0] * axis[1] + sin_part * axis[2],
                one_sub_cos * axis[0] * axis[2] - sin_part * axis[1],
                0.0,
            ),
            Vec4::new(
                one_sub_cos * axis[0] * axis[1] - sin_part * axis[2],
                one_sub_cos * axis[1] * axis[1] + cos_part,
                one_sub_cos * axis[1] * axis[2] + sin_part * axis[0],
                0.0,
            ),
            Vec4::new(
                one_sub_cos * axis[0] * axis[2] + sin_part * axis[1],
                one_sub_cos * axis[1] * axis[2] - sin_part * axis[0],
                one_sub_cos * axis[2] * axis[2] + cos_part,
                0.0,
            ),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }

    #[allow(dead_code)]
    pub fn identity() -> Mat4 {
        Mat4([
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }

    pub fn mul(&self, _rhs: &Mat4) -> Mat4 {
        // Extract rows and do dot products with columns of rhs
        // Works with SIMD via extract_row
        let row0 = self.extract_row(0);
        let row1 = self.extract_row(1);
        let row2 = self.extract_row(2);
        let row3 = self.extract_row(3);

        Mat4([
            Vec4::new(
                Vec4::dot(&row0, &_rhs[0]),
                Vec4::dot(&row1, &_rhs[0]),
                Vec4::dot(&row2, &_rhs[0]),
                Vec4::dot(&row3, &_rhs[0]),
            ),
            Vec4::new(
                Vec4::dot(&row0, &_rhs[1]),
                Vec4::dot(&row1, &_rhs[1]),
                Vec4::dot(&row2, &_rhs[1]),
                Vec4::dot(&row3, &_rhs[1]),
            ),
            Vec4::new(
                Vec4::dot(&row0, &_rhs[2]),
                Vec4::dot(&row1, &_rhs[2]),
                Vec4::dot(&row2, &_rhs[2]),
                Vec4::dot(&row3, &_rhs[2]),
            ),
            Vec4::new(
                Vec4::dot(&row0, &_rhs[3]),
                Vec4::dot(&row1, &_rhs[3]),
                Vec4::dot(&row2, &_rhs[3]),
                Vec4::dot(&row3, &_rhs[3]),
            ),
        ])
    }

    pub fn create_ortho(bottom: f32, top: f32, left: f32, right: f32, near: f32, far: f32) -> Mat4 {
        Mat4([
            Vec4::new(
                2.0 / (right - left),
                0.0,
                0.0,
                -(right + left) / (right - left),
            ),
            Vec4::new(
                0.0,
                2.0 / (top - bottom),
                0.0,
                -(top + bottom) / (top - bottom),
            ),
            Vec4::new(0.0, 0.0, -2.0 / (far - near), -(far + near) / (far - near)),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }

    pub fn create_proj(fov_angles: f32, aspect_ratio: f32, near: f32, far: f32) -> Mat4 {
        let fov_ratio = near * f32::tan(f32::to_radians(fov_angles) / 2.0);

        let r = aspect_ratio * fov_ratio;
        let l = -r;
        let t = fov_ratio;
        let b = -t;
        Mat4([
            Vec4::new(2f32 * near / (r - l), 0.0, 0.0, 0.0),
            Vec4::new(0.0, 2f32 * near / (t - b), 0.0, 0.0),
            Vec4::new(
                (r + l) / (r - l),
                (t + b) / (t - b),
                -(far + near) / (far - near),
                -1.0,
            ),
            Vec4::new(0.0, 0.0, -2.0 * far * near / (far - near), 0.0),
        ])
    }

    pub fn create_lookat(from: Vec3, to: Vec3, up: Vec3) -> Mat4 {
        let dir_fwd = (to - from).normalize();
        let dir_right = dir_fwd.cross(up.normalize()).normalize();
        let dir_up = dir_right.cross(dir_fwd).normalize();
        Mat4([
            Vec4::new(dir_right[0], dir_right[1], dir_right[2], 0.0),
            Vec4::new(dir_up[0], dir_up[1], dir_up[2], 0.0),
            Vec4::new(-dir_fwd[0], -dir_fwd[1], -dir_fwd[2], 0.0),
            Vec4::new(from[0], from[1], from[2], 1.0),
        ])
    }

    pub fn calc_det(&self) -> f32 {
        self[0][0] * self[1][1] * self[2][2] * self[3][3]
            + self[0][0] * self[1][2] * self[2][3] * self[3][1]
            + self[0][0] * self[1][3] * self[2][1] * self[3][2]
            + self[0][1] * self[1][0] * self[2][3] * self[3][2]
            + self[0][1] * self[1][2] * self[2][0] * self[3][3]
            + self[0][1] * self[1][3] * self[2][2] * self[3][0]
            + self[0][2] * self[1][0] * self[2][1] * self[3][3]
            + self[0][2] * self[1][1] * self[2][3] * self[3][0]
            + self[0][2] * self[1][3] * self[2][0] * self[3][1]
            + self[0][3] * self[1][0] * self[2][2] * self[3][1]
            + self[0][3] * self[1][1] * self[2][0] * self[3][2]
            + self[0][3] * self[1][2] * self[2][1] * self[3][0]
            - self[0][0] * self[1][1] * self[2][3] * self[3][2]
            - self[0][0] * self[1][2] * self[2][1] * self[3][3]
            - self[0][0] * self[1][3] * self[2][2] * self[3][1]
            - self[0][1] * self[1][0] * self[2][2] * self[3][3]
            - self[0][1] * self[1][2] * self[2][3] * self[3][0]
            - self[0][1] * self[1][3] * self[2][0] * self[3][2]
            - self[0][2] * self[1][0] * self[2][3] * self[3][1]
            - self[0][2] * self[1][1] * self[2][0] * self[3][3]
            - self[0][2] * self[1][3] * self[2][1] * self[3][0]
            - self[0][3] * self[1][0] * self[2][1] * self[3][2]
            - self[0][3] * self[1][1] * self[2][2] * self[3][0]
            - self[0][3] * self[1][2] * self[2][0] * self[3][1]
    }

    pub fn calc_inv_det(&self) -> f32 {
        1.0f32 / self.calc_det()
    }

    pub fn inverse(&self) -> Self {
        let inv_det = self.calc_inv_det();
        Self([
            Vec4::new(
                (self[1][1] * self[2][2] * self[3][3]
                    + self[1][2] * self[2][3] * self[3][1]
                    + self[1][3] * self[2][1] * self[3][2]
                    - self[1][1] * self[2][3] * self[3][2]
                    - self[1][2] * self[2][1] * self[3][3]
                    - self[1][3] * self[2][2] * self[3][1])
                    * inv_det,
                (self[0][1] * self[2][3] * self[3][2]
                    + self[0][2] * self[2][1] * self[3][3]
                    + self[0][3] * self[2][2] * self[3][1]
                    - self[0][1] * self[2][2] * self[3][3]
                    - self[0][2] * self[2][3] * self[3][1]
                    - self[0][3] * self[2][1] * self[3][2])
                    * inv_det,
                (self[0][1] * self[1][2] * self[3][3]
                    + self[0][2] * self[1][3] * self[3][1]
                    + self[0][3] * self[1][1] * self[3][2]
                    - self[0][1] * self[1][3] * self[3][2]
                    - self[0][2] * self[1][1] * self[3][3]
                    - self[0][3] * self[1][2] * self[3][1])
                    * inv_det,
                (self[0][1] * self[1][3] * self[2][2]
                    + self[0][2] * self[1][1] * self[2][3]
                    + self[0][3] * self[1][2] * self[2][1]
                    - self[0][1] * self[1][2] * self[2][3]
                    - self[0][2] * self[1][3] * self[2][1]
                    - self[0][3] * self[1][1] * self[2][2])
                    * inv_det,
            ),
            Vec4::new(
                (self[1][0] * self[2][3] * self[3][2]
                    + self[1][2] * self[2][0] * self[3][3]
                    + self[1][3] * self[2][2] * self[3][0]
                    - self[1][0] * self[2][2] * self[3][3]
                    - self[1][2] * self[2][3] * self[3][0]
                    - self[1][3] * self[2][0] * self[3][2])
                    * inv_det,
                (self[0][0] * self[2][2] * self[3][3]
                    + self[0][2] * self[2][3] * self[3][0]
                    + self[0][3] * self[2][0] * self[3][2]
                    - self[0][0] * self[2][3] * self[3][2]
                    - self[0][2] * self[2][0] * self[3][3]
                    - self[0][3] * self[2][2] * self[3][0])
                    * inv_det,
                (self[0][0] * self[1][3] * self[3][2]
                    + self[0][2] * self[1][0] * self[3][3]
                    + self[0][3] * self[1][2] * self[3][0]
                    - self[0][0] * self[1][2] * self[3][3]
                    - self[0][2] * self[1][3] * self[3][0]
                    - self[0][3] * self[1][0] * self[3][2])
                    * inv_det,
                (self[0][0] * self[1][2] * self[2][3]
                    + self[0][2] * self[1][3] * self[2][0]
                    + self[0][3] * self[1][0] * self[2][2]
                    - self[0][0] * self[1][3] * self[2][2]
                    - self[0][2] * self[1][0] * self[2][3]
                    - self[0][3] * self[1][2] * self[2][0])
                    * inv_det,
            ),
            Vec4::new(
                (self[1][0] * self[2][1] * self[3][3]
                    + self[1][1] * self[2][3] * self[3][0]
                    + self[1][3] * self[2][0] * self[3][1]
                    - self[1][0] * self[2][3] * self[3][1]
                    - self[1][1] * self[2][0] * self[3][3]
                    - self[1][3] * self[2][1] * self[3][0])
                    * inv_det,
                (self[0][0] * self[2][3] * self[3][1]
                    + self[0][1] * self[2][0] * self[3][3]
                    + self[0][3] * self[2][1] * self[3][0]
                    - self[0][0] * self[2][1] * self[3][3]
                    - self[0][1] * self[2][3] * self[3][0]
                    - self[0][3] * self[2][0] * self[3][1])
                    * inv_det,
                (self[0][0] * self[1][1] * self[3][3]
                    + self[0][1] * self[1][3] * self[3][0]
                    + self[0][3] * self[1][0] * self[3][1]
                    - self[0][0] * self[1][3] * self[3][1]
                    - self[0][1] * self[1][0] * self[3][3]
                    - self[0][3] * self[1][1] * self[3][0])
                    * inv_det,
                (self[0][0] * self[1][3] * self[2][1]
                    + self[0][1] * self[1][0] * self[2][3]
                    + self[0][3] * self[1][1] * self[2][0]
                    - self[0][0] * self[1][1] * self[2][3]
                    - self[0][1] * self[1][3] * self[2][0]
                    - self[0][3] * self[1][0] * self[2][1])
                    * inv_det,
            ),
            Vec4::new(
                (self[1][0] * self[2][2] * self[3][1]
                    + self[1][1] * self[2][0] * self[3][2]
                    + self[1][2] * self[2][1] * self[3][0]
                    - self[1][0] * self[2][1] * self[3][2]
                    - self[1][1] * self[2][2] * self[3][0]
                    - self[1][2] * self[2][0] * self[3][1])
                    * inv_det,
                (self[0][0] * self[2][1] * self[3][2]
                    + self[0][1] * self[2][2] * self[3][0]
                    + self[0][2] * self[2][0] * self[3][1]
                    - self[0][0] * self[2][2] * self[3][1]
                    - self[0][1] * self[2][0] * self[3][2]
                    - self[0][2] * self[2][1] * self[3][0])
                    * inv_det,
                (self[0][0] * self[1][2] * self[3][1]
                    + self[0][1] * self[1][0] * self[3][2]
                    + self[0][2] * self[1][1] * self[3][0]
                    - self[0][0] * self[1][1] * self[3][2]
                    - self[0][1] * self[1][2] * self[3][0]
                    - self[0][2] * self[1][0] * self[3][1])
                    * inv_det,
                (self[0][0] * self[1][1] * self[2][2]
                    + self[0][1] * self[1][2] * self[2][0]
                    + self[0][2] * self[1][0] * self[2][1]
                    - self[0][0] * self[1][2] * self[2][1]
                    - self[0][1] * self[1][0] * self[2][2]
                    - self[0][2] * self[1][1] * self[2][0])
                    * inv_det,
            ),
        ])
    }

    /// SIMD-optimized transpose using unpack operations
    pub fn transpose(&self) -> Mat4 {
        unsafe {
            // Original columns: col0, col1, col2, col3
            let col0 = self[0].0;
            let col1 = self[1].0;
            let col2 = self[2].0;
            let col3 = self[3].0;

            // First pass: unpack low and high elements
            // tmp0 = [col0.x, col1.x, col0.y, col1.y]
            // tmp1 = [col2.x, col3.x, col2.y, col3.y]
            // tmp2 = [col0.z, col1.z, col0.w, col1.w]
            // tmp3 = [col2.z, col3.z, col2.w, col3.w]
            let tmp0 = _mm_unpacklo_ps(col0, col1);
            let tmp2 = _mm_unpackhi_ps(col0, col1);
            let tmp1 = _mm_unpacklo_ps(col2, col3);
            let tmp3 = _mm_unpackhi_ps(col2, col3);

            // Second pass: combine to get final transposed columns
            // result_col0 = [col0.x, col1.x, col2.x, col3.x]
            // result_col1 = [col0.y, col1.y, col2.y, col3.y]
            // result_col2 = [col0.z, col1.z, col2.z, col3.z]
            // result_col3 = [col0.w, col1.w, col2.w, col3.w]
            let res0 = _mm_movelh_ps(tmp0, tmp1);
            let res1 = _mm_movehl_ps(tmp1, tmp0);
            let res2 = _mm_movelh_ps(tmp2, tmp3);
            let res3 = _mm_movehl_ps(tmp3, tmp2);

            Mat4([
                Vec4(res0),
                Vec4(res1),
                Vec4(res2),
                Vec4(res3),
            ])
        }
    }

    /// Transpose the matrix in place
    pub fn transpose_mut(&mut self) -> &mut Self {
        *self = self.transpose();
        self
    }

    /// Create a scale matrix
    pub fn from_scale(scale: Vec3) -> Self {
        Mat4([
            Vec4::new(scale[0], 0.0, 0.0, 0.0),
            Vec4::new(0.0, scale[1], 0.0, 0.0),
            Vec4::new(0.0, 0.0, scale[2], 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }

    /// Create a rotation matrix from a quaternion
    pub fn from_rotation(rotation: Quat) -> Self {
        let m = rotation.make_mat4();
        Self(m.0)
    }

    /// Create a rotation matrix from Euler angles (pitch, yaw, roll)
    pub fn from_euler_angles(pitch: f32, yaw: f32, roll: f32) -> Self {
        let q = Quat::from_axis_angle(Vec3::x_axis(), pitch)
            * Quat::from_axis_angle(Vec3::y_axis(), yaw)
            * Quat::from_axis_angle(Vec3::z_axis(), roll);
        let m = q.make_mat4();
        Self(m.0)
    }

    /// Create a TRS (Translation, Rotation, Scale) matrix
    pub fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        let scale_mat = Self::from_scale(scale);
        let rot_mat = Self::from_rotation(rotation);
        let pos_mat = Self::from_translation([translation.x(), translation.y(), translation.z()]);
        pos_mat.mul(&rot_mat.mul(&scale_mat))
    }

    /// Extract the translation component
    /// This is a fast operation that only reads the translation column
    pub fn extract_translation(&self) -> Vec3 {
        Vec3::new(self[3][0], self[3][1], self[3][2])
    }

    /// Decompose the matrix into translation, rotation, and scale components
    /// This is more efficient than calling extract_translation, extract_rotation,
    /// and extract_scale separately as it reuses intermediate calculations.
    /// Note: This may not produce correct results if the matrix has non-uniform scaling or skew
    pub fn decompose(&self) -> Transform {
        // Extract translation (column 3)
        let translation = Vec3::new(self[3][0], self[3][1], self[3][2]);

        // Extract scale from the 3x3 portion
        // Scale is the length of each column/row in the rotation-scale matrix
        let sx = (self[0][0] * self[0][0] + self[0][1] * self[0][1] + self[0][2] * self[0][2]).sqrt();
        let sy = (self[1][0] * self[1][0] + self[1][1] * self[1][1] + self[1][2] * self[1][2]).sqrt();
        let sz = (self[2][0] * self[2][0] + self[2][1] * self[2][1] + self[2][2] * self[2][2]).sqrt();
        let scale = Vec3::new(sx, sy, sz);

        // Extract rotation from the 3x3 portion, removing scale
        // Get the 3x3 rotation-scale matrix elements
        let m00 = self[0][0] / sx;
        let m01 = self[0][1] / sy;
        let m02 = self[0][2] / sz;
        let m10 = self[1][0] / sx;
        let m11 = self[1][1] / sy;
        let m12 = self[1][2] / sz;
        let m20 = self[2][0] / sx;
        let m21 = self[2][1] / sy;
        let m22 = self[2][2] / sz;

        // Calculate trace of the rotation matrix
        let trace = m00 + m11 + m22;

        let rotation = if trace > 0.0 {
            let s = f32::sqrt(trace + 1.0) * 2.0;
            let w = 0.25 * s;
            let x = (m21 - m12) / s;
            let y = (m02 - m20) / s;
            let z = (m10 - m01) / s;
            Quat::new_from_xyzw(x, y, z, w)
        } else if (m00 > m11) && (m00 > m22) {
            let s = f32::sqrt(1.0 + m00 - m11 - m22) * 2.0;
            let w = (m21 - m12) / s;
            let x = 0.25 * s;
            let y = (m01 + m10) / s;
            let z = (m02 + m20) / s;
            Quat::new_from_xyzw(x, y, z, w)
        } else if m11 > m22 {
            let s = f32::sqrt(1.0 + m11 - m00 - m22) * 2.0;
            let w = (m02 - m20) / s;
            let x = (m01 + m10) / s;
            let y = 0.25 * s;
            let z = (m12 + m21) / s;
            Quat::new_from_xyzw(x, y, z, w)
        } else {
            let s = f32::sqrt(1.0 + m22 - m00 - m11) * 2.0;
            let w = (m10 - m01) / s;
            let x = (m02 + m20) / s;
            let y = (m12 + m21) / s;
            let z = 0.25 * s;
            Quat::new_from_xyzw(x, y, z, w)
        };

        Transform {
            position: translation,
            rotation,
            scale,
        }
    }

    /// Extract the rotation component as a quaternion
    /// Note: This may not produce correct results if the matrix has non-uniform scaling
    pub fn extract_rotation(&self) -> Quat {
        // Extract the 3x3 rotation portion
        let m00 = self[0][0];
        let m01 = self[0][1];
        let m02 = self[0][2];
        let m10 = self[1][0];
        let m11 = self[1][1];
        let m12 = self[1][2];
        let m20 = self[2][0];
        let m21 = self[2][1];
        let m22 = self[2][2];

        // Calculate trace
        let trace = m00 + m11 + m22;

        if trace > 0.0 {
            let s = f32::sqrt(trace + 1.0) * 2.0;
            let w = 0.25 * s;
            let x = (m21 - m12) / s;
            let y = (m02 - m20) / s;
            let z = (m10 - m01) / s;
            Quat::new_from_xyzw(x, y, z, w)
        } else if (m00 > m11) && (m00 > m22) {
            let s = f32::sqrt(1.0 + m00 - m11 - m22) * 2.0;
            let w = (m21 - m12) / s;
            let x = 0.25 * s;
            let y = (m01 + m10) / s;
            let z = (m02 + m20) / s;
            Quat::new_from_xyzw(x, y, z, w)
        } else if m11 > m22 {
            let s = f32::sqrt(1.0 + m11 - m00 - m22) * 2.0;
            let w = (m02 - m20) / s;
            let x = (m01 + m10) / s;
            let y = 0.25 * s;
            let z = (m12 + m21) / s;
            Quat::new_from_xyzw(x, y, z, w)
        } else {
            let s = f32::sqrt(1.0 + m22 - m00 - m11) * 2.0;
            let w = (m10 - m01) / s;
            let x = (m02 + m20) / s;
            let y = (m12 + m21) / s;
            let z = 0.25 * s;
            Quat::new_from_xyzw(x, y, z, w)
        }
    }

    /// Extract the scale component
    pub fn extract_scale(&self) -> Vec3 {
        let x = f32::sqrt(self[0][0] * self[0][0] + self[0][1] * self[0][1] + self[0][2] * self[0][2]);
        let y = f32::sqrt(self[1][0] * self[1][0] + self[1][1] * self[1][1] + self[1][2] * self[1][2]);
        let z = f32::sqrt(self[2][0] * self[2][0] + self[2][1] * self[2][1] + self[2][2] * self[2][2]);
        Vec3::new(x, y, z)
    }

    /// Extract the 3x3 portion of the matrix
    pub fn to_mat3(&self) -> crate::Mat3 {
        crate::Mat3([
            crate::Vec3::new(self[0][0], self[0][1], self[0][2]),
            crate::Vec3::new(self[1][0], self[1][1], self[1][2]),
            crate::Vec3::new(self[2][0], self[2][1], self[2][2]),
        ])
    }
}

// Matrix-vector multiplication traits
impl Mul<Vec3> for Mat4 {
    type Output = Vec3;

    fn mul(self, rhs: Vec3) -> Vec3 {
        let row0 = self.extract_row(0);
        let row1 = self.extract_row(1);
        let row2 = self.extract_row(2);
        Vec3::new(
            Vec3::new(row0[0], row0[1], row0[2]).dot(rhs) + row0[3],
            Vec3::new(row1[0], row1[1], row1[2]).dot(rhs) + row1[3],
            Vec3::new(row2[0], row2[1], row2[2]).dot(rhs) + row2[3],
        )
    }
}

impl Mul<&Vec3> for &Mat4 {
    type Output = Vec3;

    fn mul(self, rhs: &Vec3) -> Vec3 {
        let row0 = self.extract_row(0);
        let row1 = self.extract_row(1);
        let row2 = self.extract_row(2);
        Vec3::new(
            Vec3::new(row0[0], row0[1], row0[2]).dot(*rhs) + row0[3],
            Vec3::new(row1[0], row1[1], row1[2]).dot(*rhs) + row1[3],
            Vec3::new(row2[0], row2[1], row2[2]).dot(*rhs) + row2[3],
        )
    }
}

impl Mul<Vec4> for Mat4 {
    type Output = Vec4;

    fn mul(self, rhs: Vec4) -> Vec4 {
        let row0 = self.extract_row(0);
        let row1 = self.extract_row(1);
        let row2 = self.extract_row(2);
        let row3 = self.extract_row(3);
        Vec4::new(
            Vec4::dot(&row0, &rhs),
            Vec4::dot(&row1, &rhs),
            Vec4::dot(&row2, &rhs),
            Vec4::dot(&row3, &rhs),
        )
    }
}

impl Mul<&Vec4> for &Mat4 {
    type Output = Vec4;

    fn mul(self, rhs: &Vec4) -> Vec4 {
        let row0 = self.extract_row(0);
        let row1 = self.extract_row(1);
        let row2 = self.extract_row(2);
        let row3 = self.extract_row(3);
        Vec4::new(
            Vec4::dot(&row0, rhs),
            Vec4::dot(&row1, rhs),
            Vec4::dot(&row2, rhs),
            Vec4::dot(&row3, rhs),
        )
    }
}

// Matrix-matrix multiplication traits
impl Mul<&Mat4> for Mat4 {
    type Output = Mat4;

    fn mul(self, rhs: &Mat4) -> Mat4 {
        Mat4::mul(&self, rhs)
    }
}

impl Mul<Mat4> for Mat4 {
    type Output = Mat4;

    fn mul(self, rhs: Mat4) -> Mat4 {
        self.mul(&rhs)
    }
}

impl Mul<Mat4> for &Mat4 {
    type Output = Mat4;

    fn mul(self, rhs: Mat4) -> Mat4 {
        Mat4::mul(self, &rhs)
    }
}

impl Mul<&Mat4> for &Mat4 {
    type Output = Mat4;

    fn mul(self, rhs: &Mat4) -> Mat4 {
        Mat4::mul(self, rhs)
    }
}

impl MulAssign for Mat4 {
    fn mul_assign(&mut self, rhs: Self) {
        *self = self.clone().mul(&rhs);
    }
}

impl From<Mat4> for [[f32; 4]; 4] {
    fn from(val: Mat4) -> Self {
        let vec_arr = val.0;

        [
            vec_arr[0].into(),
            vec_arr[1].into(),
            vec_arr[2].into(),
            vec_arr[3].into(),
        ]
    }
}
