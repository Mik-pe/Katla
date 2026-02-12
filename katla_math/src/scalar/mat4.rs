//! Scalar (fallback) implementation of Mat4
//!
//! This is used when SSE intrinsics are not available or when the scalar
//! implementation is explicitly preferred.

use crate::{Mat4, Vec4};

impl Mat4 {
    pub fn extract_row(&self, index: usize) -> Vec4 {
        Vec4::new(
            self[0][index],
            self[1][index],
            self[2][index],
            self[3][index],
        )
    }

    /// Transpose the matrix
    pub fn transpose(&self) -> Self {
        Self([
            Vec4::new(self[0][0], self[1][0], self[2][0], self[3][0]),
            Vec4::new(self[0][1], self[1][1], self[2][1], self[3][1]),
            Vec4::new(self[0][2], self[1][2], self[2][2], self[3][2]),
            Vec4::new(self[0][3], self[1][3], self[2][3], self[3][3]),
        ])
    }
}
