use core::{
    f32,
    ops::{Add, Div, Index, Mul, Neg, Sub},
};

#[derive(Debug, Copy, Clone)]
pub struct Mat2(pub [Vec2; 2]);

// Import Vec2 for use in Mat2
use crate::Vec2;

impl Index<usize> for Mat2 {
    type Output = Vec2;
    fn index(&self, index: usize) -> &Vec2 {
        &self.0[index]
    }
}

impl Mat2 {
    #[inline]
    pub fn zero() -> Mat2 {
        Mat2([Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)])
    }

    #[inline]
    pub fn identity() -> Mat2 {
        Mat2([Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)])
    }

    #[inline]
    pub fn new(m00: f32, m01: f32, m10: f32, m11: f32) -> Mat2 {
        // Column-major storage: column 0 (m00, m10), column 1 (m01, m11)
        Mat2([
            Vec2::new(m00, m10),
            Vec2::new(m01, m11),
        ])
    }

    /// Create a 2x2 rotation matrix
    #[inline]
    pub fn from_rotation(angle: f32) -> Self {
        let c = f32::cos(angle);
        let s = f32::sin(angle);
        // Standard 2D rotation matrix (counterclockwise):
        // [c -s]
        // [s  c]
        // In column-major storage: column 0 is [c, s], column 1 is [-s, c]
        Self::new(c, -s, s, c)
    }

    /// Create a 2x2 scale matrix
    #[inline]
    pub fn from_scale(scale: Vec2) -> Self {
        Self::new(scale.x(), 0.0, 0.0, scale.y())
    }

    /// Multiply two 2x2 matrices
    #[inline]
    pub fn mul(&self, rhs: &Mat2) -> Mat2 {
        Mat2([
            Vec2::new(
                self[0][0] * rhs[0][0] + self[1][0] * rhs[1][0],
                self[0][1] * rhs[0][0] + self[1][1] * rhs[1][0],
            ),
            Vec2::new(
                self[0][0] * rhs[0][1] + self[1][0] * rhs[1][1],
                self[0][1] * rhs[0][1] + self[1][1] * rhs[1][1],
            ),
        ])
    }

    /// Transpose the matrix
    #[inline]
    pub fn transpose(&self) -> Mat2 {
        // [c0r0  c1r0]     [c0r0  c0r1]
        // [c0r1  c1r1] ->  [c1r0  c1r1]
        Mat2([
            Vec2::new(
                self[0][0],
                self[1][0],  // Swap with c1r0
            ),
            Vec2::new(
                self[0][1],  // Swap with c0r1
                self[1][1],
            ),
        ])
    }

    /// Calculate the determinant of the matrix
    #[inline]
    pub fn determinant(&self) -> f32 {
        self[0][0] * self[1][1] - self[1][0] * self[0][1]
    }

    /// Calculate the inverse of the matrix
    #[inline]
    pub fn inverse(&self) -> Option<Mat2> {
        let det = self.determinant();
        if det.abs() < f32::EPSILON {
            return None;
        }

        let inv_det = 1.0 / det;
        // For column-major storage, swap the diagonal elements and negate off-diagonals
        // In row-major: [[d, -b], [-c, a]] / det
        // In column-major: column 0 is [d; -c], column 1 is [-b; a]
        Some(Mat2::new(
            self[1][1] * inv_det,  // d
            -self[1][0] * inv_det, // -c
            -self[0][1] * inv_det, // -b
            self[0][0] * inv_det,  // a
        ))
    }

    /// Extract rotation angle from rotation matrix
    #[inline]
    pub fn to_rotation(&self) -> f32 {
        f32::atan2(self[0][1], self[0][0])
    }

    /// Extract scale from scale matrix (or diagonal elements)
    #[inline]
    pub fn to_scale(&self) -> Vec2 {
        Vec2::new(self[0][0], self[1][1])
    }
}

impl Mul<Mat2> for Mat2 {
    type Output = Mat2;

    fn mul(self, rhs: Mat2) -> Self::Output {
        // Standard matrix multiplication with column-major storage m[col][row]
        // result[col][row] = sum over k of self[k][row] * rhs[col][k]
        Mat2([
            Vec2::new(
                self[0][0] * rhs[0][0] + self[1][0] * rhs[0][1],
                self[0][1] * rhs[0][0] + self[1][1] * rhs[0][1],
            ),
            Vec2::new(
                self[0][0] * rhs[1][0] + self[1][0] * rhs[1][1],
                self[0][1] * rhs[1][0] + self[1][1] * rhs[1][1],
            ),
        ])
    }
}

impl Mul<Vec2> for Mat2 {
    type Output = Vec2;

    fn mul(self, rhs: Vec2) -> Vec2 {
        // Matrix-vector multiplication: result[i] = sum over j of m[i][j] * v[j]
        // With column-major m[col][row]:
        // result[0] = m[0][0] * v[0] + m[1][0] * v[1]
        // result[1] = m[0][1] * v[0] + m[1][1] * v[1]
        Vec2::new(
            self[0][0] * rhs.x() + self[1][0] * rhs.y(),
            self[0][1] * rhs.x() + self[1][1] * rhs.y(),
        )
    }
}

impl Mul<f32> for Mat2 {
    type Output = Mat2;

    fn mul(self, scalar: f32) -> Mat2 {
        Mat2([
            Vec2::new(self[0][0] * scalar, self[0][1] * scalar),
            Vec2::new(self[1][0] * scalar, self[1][1] * scalar),
        ])
    }
}

impl Div<f32> for Mat2 {
    type Output = Mat2;

    fn div(self, scalar: f32) -> Mat2 {
        Mat2([
            Vec2::new(self[0][0] / scalar, self[0][1] / scalar),
            Vec2::new(self[1][0] / scalar, self[1][1] / scalar),
        ])
    }
}

impl Add for Mat2 {
    type Output = Mat2;

    fn add(self, rhs: Mat2) -> Mat2 {
        Mat2([
            Vec2::new(self[0][0] + rhs[0][0], self[0][1] + rhs[0][1]),
            Vec2::new(self[1][0] + rhs[1][0], self[1][1] + rhs[1][1]),
        ])
    }
}

impl Sub for Mat2 {
    type Output = Mat2;

    fn sub(self, rhs: Mat2) -> Mat2 {
        Mat2([
            Vec2::new(self[0][0] - rhs[0][0], self[0][1] - rhs[0][1]),
            Vec2::new(self[1][0] - rhs[1][0], self[1][1] - rhs[1][1]),
        ])
    }
}

impl Neg for Mat2 {
    type Output = Mat2;

    fn neg(self) -> Mat2 {
        Mat2([
            Vec2::new(-self[0][0], -self[0][1]),
            Vec2::new(-self[1][0], -self[1][1]),
        ])
    }
}

impl Default for Mat2 {
    fn default() -> Self {
        Self::identity()
    }
}

impl PartialEq for Mat2 {
    fn eq(&self, other: &Self) -> bool {
        self[0][0] == other[0][0]
            && self[0][1] == other[0][1]
            && self[1][0] == other[1][0]
            && self[1][1] == other[1][1]
    }
}
