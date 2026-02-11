//! 4x4 Matrix using SIMD (platform intrinsics)
//!
//! Mat4 uses platform-specific SIMD intrinsics for high-performance operations.
//! Uses SSE on x86/x86_64, scalar fallback on other platforms.

use crate::Vec3;
use crate::Vec4;
use core::ops::{Mul, MulAssign};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use crate::sse::mat4::Mat4;

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub use crate::scalar::mat4::Mat4;

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
