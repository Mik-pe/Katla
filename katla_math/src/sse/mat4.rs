//! SSE-accelerated implementation of Mat4
//!
//! This uses SSE intrinsics for high-performance matrix operations.
//! Only available on x86/x86_64 with SSE2 support.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;

use crate::Vec4;
use core::ops::Index;

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
    pub fn new() -> Self {
        Self([
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }

    pub fn from_translation(pos: [f32; 3]) -> Self {
        Self([
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(pos[0], pos[1], pos[2], 1.0),
        ])
    }

    pub fn from_rotaxis(angle: &f32, axis: [f32; 3]) -> Self {
        let cos_part = angle.cos();
        let sin_part = angle.sin();
        let one_sub_cos = 1.0 - cos_part;
        Self([
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
    pub fn identity() -> Self {
        Self([
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        Self([
            Vec4::new(
                Vec4::dot(&self.extract_row(0), &rhs[0]),
                Vec4::dot(&self.extract_row(1), &rhs[0]),
                Vec4::dot(&self.extract_row(2), &rhs[0]),
                Vec4::dot(&self.extract_row(3), &rhs[0]),
            ),
            Vec4::new(
                Vec4::dot(&self.extract_row(0), &rhs[1]),
                Vec4::dot(&self.extract_row(1), &rhs[1]),
                Vec4::dot(&self.extract_row(2), &rhs[1]),
                Vec4::dot(&self.extract_row(3), &rhs[1]),
            ),
            Vec4::new(
                Vec4::dot(&self.extract_row(0), &rhs[2]),
                Vec4::dot(&self.extract_row(1), &rhs[2]),
                Vec4::dot(&self.extract_row(2), &rhs[2]),
                Vec4::dot(&self.extract_row(3), &rhs[2]),
            ),
            Vec4::new(
                Vec4::dot(&self.extract_row(0), &rhs[3]),
                Vec4::dot(&self.extract_row(1), &rhs[3]),
                Vec4::dot(&self.extract_row(2), &rhs[3]),
                Vec4::dot(&self.extract_row(3), &rhs[3]),
            ),
        ])
    }

    pub fn create_ortho(bottom: f32, top: f32, left: f32, right: f32, near: f32, far: f32) -> Self {
        Self([
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

    pub fn create_proj(fov_angles: f32, aspect_ratio: f32, near: f32, far: f32) -> Self {
        let fov_ratio = near * f32::tan(f32::to_radians(fov_angles) / 2.0);

        let r = aspect_ratio * fov_ratio;
        let l = -r;
        let t = fov_ratio;
        let b = -t;
        Self([
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

    pub fn create_lookat(from: crate::Vec3, to: crate::Vec3, up: crate::Vec3) -> Self {
        let dir_fwd = (to - from).normalize();
        let dir_right = dir_fwd.cross(up.normalize()).normalize();
        let dir_up = dir_right.cross(dir_fwd).normalize();
        Self([
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

    pub fn from_scale(scale: crate::Vec3) -> Self {
        Self([
            Vec4::new(scale[0], 0.0, 0.0, 0.0),
            Vec4::new(0.0, scale[1], 0.0, 0.0),
            Vec4::new(0.0, 0.0, scale[2], 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }

    pub fn from_rotation(rotation: crate::Quat) -> Self {
        let m = rotation.make_mat4();
        Self(m.0)
    }

    pub fn from_euler_angles(pitch: f32, yaw: f32, roll: f32) -> Self {
        let q = crate::Quat::from_axis_angle(crate::Vec3::x_axis(), pitch)
            * crate::Quat::from_axis_angle(crate::Vec3::y_axis(), yaw)
            * crate::Quat::from_axis_angle(crate::Vec3::z_axis(), roll);
        let m = q.make_mat4();
        Self(m.0)
    }

    pub fn from_trs(translation: crate::Vec3, rotation: crate::Quat, scale: crate::Vec3) -> Self {
        let scale_mat = Self::from_scale(scale);
        let rot_mat = Self::from_rotation(rotation);
        let pos_mat = Self::from_translation([translation.x(), translation.y(), translation.z()]);
        pos_mat.mul(&rot_mat.mul(&scale_mat))
    }

    pub fn extract_translation(&self) -> crate::Vec3 {
        crate::Vec3::new(self[3][0], self[3][1], self[3][2])
    }

    pub fn decompose(&self) -> crate::Transform {
        let translation = crate::Vec3::new(self[3][0], self[3][1], self[3][2]);

        let sx =
            (self[0][0] * self[0][0] + self[0][1] * self[0][1] + self[0][2] * self[0][2]).sqrt();
        let sy =
            (self[1][0] * self[1][0] + self[1][1] * self[1][1] + self[1][2] * self[1][2]).sqrt();
        let sz =
            (self[2][0] * self[2][0] + self[2][1] * self[2][1] + self[2][2] * self[2][2]).sqrt();
        let scale = crate::Vec3::new(sx, sy, sz);

        let m00 = self[0][0] / sx;
        let m01 = self[0][1] / sy;
        let m02 = self[0][2] / sz;
        let m10 = self[1][0] / sx;
        let m11 = self[1][1] / sy;
        let m12 = self[1][2] / sz;
        let m20 = self[2][0] / sx;
        let m21 = self[2][1] / sy;
        let m22 = self[2][2] / sz;

        let trace = m00 + m11 + m22;

        let rotation = if trace > 0.0 {
            let s = f32::sqrt(trace + 1.0) * 2.0;
            let w = 0.25 * s;
            let x = (m21 - m12) / s;
            let y = (m02 - m20) / s;
            let z = (m10 - m01) / s;
            crate::Quat::new_from_xyzw(x, y, z, w)
        } else if (m00 > m11) && (m00 > m22) {
            let s = f32::sqrt(1.0 + m00 - m11 - m22) * 2.0;
            let w = (m21 - m12) / s;
            let x = 0.25 * s;
            let y = (m01 + m10) / s;
            let z = (m02 + m20) / s;
            crate::Quat::new_from_xyzw(x, y, z, w)
        } else if m11 > m22 {
            let s = f32::sqrt(1.0 + m11 - m00 - m22) * 2.0;
            let w = (m02 - m20) / s;
            let x = (m01 + m10) / s;
            let y = 0.25 * s;
            let z = (m12 + m21) / s;
            crate::Quat::new_from_xyzw(x, y, z, w)
        } else {
            let s = f32::sqrt(1.0 + m22 - m00 - m11) * 2.0;
            let w = (m10 - m01) / s;
            let x = (m02 + m20) / s;
            let y = (m12 + m21) / s;
            let z = 0.25 * s;
            crate::Quat::new_from_xyzw(x, y, z, w)
        };

        crate::Transform {
            position: translation,
            rotation,
            scale,
        }
    }

    /// SIMD-optimized row extraction using shuffle operations
    pub fn extract_row(&self, index: usize) -> Vec4 {
        unsafe {
            match index {
                0 => {
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_X);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_X);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_X);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_X);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                1 => {
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_Y);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_Y);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_Y);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_Y);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                2 => {
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_Z);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_Z);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_Z);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_Z);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                3 => {
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

    /// SIMD-optimized transpose using unpack operations
    pub fn transpose(&self) -> Self {
        unsafe {
            let col0 = self[0].0;
            let col1 = self[1].0;
            let col2 = self[2].0;
            let col3 = self[3].0;

            let tmp0 = _mm_unpacklo_ps(col0, col1);
            let tmp2 = _mm_unpackhi_ps(col0, col1);
            let tmp1 = _mm_unpacklo_ps(col2, col3);
            let tmp3 = _mm_unpackhi_ps(col2, col3);

            let res0 = _mm_movelh_ps(tmp0, tmp1);
            let res1 = _mm_movehl_ps(tmp1, tmp0);
            let res2 = _mm_movelh_ps(tmp2, tmp3);
            let res3 = _mm_movehl_ps(tmp3, tmp2);

            Self([Vec4(res0), Vec4(res1), Vec4(res2), Vec4(res3)])
        }
    }

    /// Transpose the matrix in place
    pub fn transpose_mut(&mut self) -> &mut Self {
        *self = self.transpose();
        self
    }

    /// Extract the scale component
    pub fn extract_scale(&self) -> crate::Vec3 {
        let x =
            f32::sqrt(self[0][0] * self[0][0] + self[0][1] * self[0][1] + self[0][2] * self[0][2]);
        let y =
            f32::sqrt(self[1][0] * self[1][0] + self[1][1] * self[1][1] + self[1][2] * self[1][2]);
        let z =
            f32::sqrt(self[2][0] * self[2][0] + self[2][1] * self[2][1] + self[2][2] * self[2][2]);
        crate::Vec3::new(x, y, z)
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
