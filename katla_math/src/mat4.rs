//! 4x4 Matrix using SIMD (platform intrinsics)
//!
//! Mat4 uses platform-specific SIMD intrinsics for high-performance operations.
//! Uses SSE on x86/x86_64, scalar fallback on other platforms.

use crate::Vec3;
use crate::Vec4;
use core::ops::{Mul, MulAssign};
use std::ops::Index;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4(pub [Vec4; 4]);

impl Mat4 {
    pub fn from_translation(pos: [f32; 3]) -> Self {
        Self([
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(pos[0], pos[1], pos[2], 1.0),
        ])
    }

    #[inline]
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

    #[inline]
    pub fn identity() -> Self {
        Self([
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }

    #[inline]
    pub fn mul(&self, rhs: &Self) -> Self {
        Self([
            Vec4::new(
                Vec4::dot(self.extract_row(0), rhs[0]),
                Vec4::dot(self.extract_row(1), rhs[0]),
                Vec4::dot(self.extract_row(2), rhs[0]),
                Vec4::dot(self.extract_row(3), rhs[0]),
            ),
            Vec4::new(
                Vec4::dot(self.extract_row(0), rhs[1]),
                Vec4::dot(self.extract_row(1), rhs[1]),
                Vec4::dot(self.extract_row(2), rhs[1]),
                Vec4::dot(self.extract_row(3), rhs[1]),
            ),
            Vec4::new(
                Vec4::dot(self.extract_row(0), rhs[2]),
                Vec4::dot(self.extract_row(1), rhs[2]),
                Vec4::dot(self.extract_row(2), rhs[2]),
                Vec4::dot(self.extract_row(3), rhs[2]),
            ),
            Vec4::new(
                Vec4::dot(self.extract_row(0), rhs[3]),
                Vec4::dot(self.extract_row(1), rhs[3]),
                Vec4::dot(self.extract_row(2), rhs[3]),
                Vec4::dot(self.extract_row(3), rhs[3]),
            ),
        ])
    }

    pub fn create_ortho(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
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

    pub fn create_proj_reverse_z(fov_angles: f32, aspect_ratio: f32, near: f32) -> Self {
        let f = 1.0 / f32::tan(f32::to_radians(fov_angles) / 2.0);
        Self([
            Vec4::new(f / aspect_ratio, 0.0, 0.0, 0.0),
            Vec4::new(0.0, -f, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, -1.0),
            Vec4::new(0.0, 0.0, near, 0.0),
        ])
    }

    #[inline]
    pub fn create_proj_perspective(
        fov_angles: f32,
        aspect_ratio: f32,
        near: f32,
        far: f32,
    ) -> Self {
        let f = 1.0 / f32::tan(f32::to_radians(fov_angles) / 2.0);
        Self([
            Vec4::new(f / aspect_ratio, 0.0, 0.0, 0.0),
            Vec4::new(0.0, -f, 0.0, 0.0),
            Vec4::new(0.0, 0.0, far / (near - far), -1.0),
            Vec4::new(0.0, 0.0, near * far / (near - far), 0.0),
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

    #[inline]
    pub fn inverse(&self) -> Option<Mat4> {
        let det = self.calc_det();
        if det.abs() < 1e-6 {
            return None;
        }

        let inv_det = 1.0 / det;
        Some(Self([
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
        ]))
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
        let q = crate::Quat::from_axis_angle(crate::Vec3::X_AXIS, pitch)
            * crate::Quat::from_axis_angle(crate::Vec3::Y_AXIS, yaw)
            * crate::Quat::from_axis_angle(crate::Vec3::Z_AXIS, roll);
        let m = q.make_mat4();
        Self(m.0)
    }

    #[inline]
    pub fn from_trs(translation: crate::Vec3, rotation: crate::Quat, scale: crate::Vec3) -> Self {
        let scale_mat = Self::from_scale(scale);
        let rot_mat = Self::from_rotation(rotation);
        let pos_mat = Self::from_translation([translation.x(), translation.y(), translation.z()]);
        pos_mat.mul(&rot_mat.mul(&scale_mat))
    }

    pub fn extract_translation(&self) -> crate::Vec3 {
        crate::Vec3::new(self[3][0], self[3][1], self[3][2])
    }

    #[inline]
    pub fn decompose(&self) -> crate::Transform {
        let translation = crate::Vec3::new(self[3][0], self[3][1], self[3][2]);

        let sx =
            (self[0][0] * self[0][0] + self[0][1] * self[0][1] + self[0][2] * self[0][2]).sqrt();
        let sy =
            (self[1][0] * self[1][0] + self[1][1] * self[1][1] + self[1][2] * self[1][2]).sqrt();
        let sz =
            (self[2][0] * self[2][0] + self[2][1] * self[2][1] + self[2][2] * self[2][2]).sqrt();
        let scale = crate::Vec3::new(sx, sy, sz);

        let mat3 = crate::Mat3::from_columns(
            crate::Vec3::new(self[0][0] / sx, self[0][1] / sx, self[0][2] / sx),
            crate::Vec3::new(self[1][0] / sy, self[1][1] / sy, self[1][2] / sy),
            crate::Vec3::new(self[2][0] / sz, self[2][1] / sz, self[2][2] / sz),
        );
        let rotation = crate::Quat::from(mat3);

        crate::Transform {
            position: translation,
            rotation,
            scale,
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

    /// Convert to a flat array of 16 floats in column-major order.
    ///
    /// This is the format expected by GPU shaders and Vulkan.
    pub fn to_array(&self) -> [f32; 16] {
        [
            self[0][0], self[0][1], self[0][2], self[0][3], self[1][0], self[1][1], self[1][2],
            self[1][3], self[2][0], self[2][1], self[2][2], self[2][3], self[3][0], self[3][1],
            self[3][2], self[3][3],
        ]
    }
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Index<usize> for Mat4 {
    type Output = Vec4;

    fn index(&self, index: usize) -> &Vec4 {
        //NB: Since this returns a Vec4, we get column-by-column of this matrix
        match index {
            0 => &self.0[0],
            1 => &self.0[1],
            2 => &self.0[2],
            3 => &self.0[3],
            _ => panic!("INDEXING OUT_OF_BOUNDS in Mat4"),
        }
    }
}

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
            Vec4::dot(row0, rhs),
            Vec4::dot(row1, rhs),
            Vec4::dot(row2, rhs),
            Vec4::dot(row3, rhs),
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
            Vec4::dot(row0, *rhs),
            Vec4::dot(row1, *rhs),
            Vec4::dot(row2, *rhs),
            Vec4::dot(row3, *rhs),
        )
    }
}

impl Mul<&Mat4> for Mat4 {
    type Output = Mat4;

    fn mul(self, rhs: &Mat4) -> Mat4 {
        Mat4::mul(&self, rhs)
    }
}

impl Mul<Mat4> for Mat4 {
    type Output = Mat4;

    fn mul(self, rhs: Mat4) -> Mat4 {
        Mat4::mul(&self, &rhs)
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
        self.mul(rhs)
    }
}

impl MulAssign for Mat4 {
    fn mul_assign(&mut self, rhs: Self) {
        *self = (*self).mul(&rhs);
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
