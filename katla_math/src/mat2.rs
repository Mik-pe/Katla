use core::{
    f32,
    ops::{Add, Div, Index, Mul, Neg, Sub},
};

#[derive(Debug, Copy, Clone, PartialEq)]
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
        Mat2([Vec2::new(m00, m10), Vec2::new(m01, m11)])
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

    /// Transpose the matrix
    #[inline]
    pub fn transpose(&self) -> Mat2 {
        // [c0r0  c1r0]     [c0r0  c0r1]
        // [c0r1  c1r1] ->  [c1r0  c1r1]
        Mat2([
            Vec2::new(
                self[0][0], self[1][0], // Swap with c1r0
            ),
            Vec2::new(
                self[0][1], // Swap with c0r1
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

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f32 = 1e-5;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < TOL
    }

    fn mat2_approx_eq(a: Mat2, b: Mat2) -> bool {
        approx_eq(a[0][0], b[0][0])
            && approx_eq(a[0][1], b[0][1])
            && approx_eq(a[1][0], b[1][0])
            && approx_eq(a[1][1], b[1][1])
    }

    #[test]
    fn test_mat2_identity() {
        let id = Mat2::identity();
        let m = Mat2::new(3.0, 5.0, 7.0, 11.0);
        assert!(mat2_approx_eq(id * m, m));
        assert!(mat2_approx_eq(m * id, m));
    }

    #[test]
    fn test_mat2_mul() {
        let a = Mat2::new(1.0, 2.0, 3.0, 4.0);
        let b = Mat2::new(5.0, 6.0, 7.0, 8.0);
        let result = a * b;
        assert!(mat2_approx_eq(result, Mat2::new(19.0, 22.0, 43.0, 50.0)));
    }

    #[test]
    fn test_mat2_determinant() {
        let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
        assert!(approx_eq(m.determinant(), -2.0));
        assert!(approx_eq(Mat2::identity().determinant(), 1.0));
    }

    #[test]
    fn test_mat2_inverse() {
        let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
        let inv = m.inverse().expect("invertible");
        assert!(mat2_approx_eq(m * inv, Mat2::identity()));
        assert!(mat2_approx_eq(inv * m, Mat2::identity()));
    }

    #[test]
    fn test_mat2_inverse_singular() {
        let zero = Mat2::zero();
        assert!(zero.inverse().is_none());
    }

    #[test]
    fn test_mat2_transpose() {
        let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
        assert!(mat2_approx_eq(m.transpose().transpose(), m));
        let id = Mat2::identity();
        assert!(mat2_approx_eq(id.transpose(), id));
    }

    #[test]
    fn test_mat2_from_rotation() {
        let rot = Mat2::from_rotation(std::f32::consts::FRAC_PI_2);
        let v = Vec2::new(1.0, 0.0);
        let result = rot * v;
        assert!(approx_eq(result.x(), 0.0));
        assert!(approx_eq(result.y(), 1.0));
    }

    #[test]
    fn test_mat2_from_scale() {
        let s = Vec2::new(2.0, 3.0);
        let m = Mat2::from_scale(s);
        let v = Vec2::new(1.0, 1.0);
        let result = m * v;
        assert!(approx_eq(result.x(), 2.0));
        assert!(approx_eq(result.y(), 3.0));
    }

    #[test]
    fn test_mat2_to_rotation() {
        let angle = 1.23;
        let m = Mat2::from_rotation(angle);
        assert!(approx_eq(m.to_rotation(), angle));
    }

    #[test]
    fn test_mat2_to_scale() {
        let scale = Vec2::new(4.0, 5.0);
        let m = Mat2::from_scale(scale);
        assert!(approx_eq(m.to_scale().x(), scale.x()));
        assert!(approx_eq(m.to_scale().y(), scale.y()));
    }

    #[test]
    fn test_mat2_mul_vec() {
        let m = Mat2::new(2.0, 0.0, 0.0, 3.0);
        let v = Vec2::new(1.0, 1.0);
        let result = m * v;
        assert!(approx_eq(result.x(), 2.0));
        assert!(approx_eq(result.y(), 3.0));
    }

    #[test]
    fn test_mat2_mul_scalar() {
        let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
        let result = m * 2.0;
        assert!(mat2_approx_eq(result, Mat2::new(2.0, 4.0, 6.0, 8.0)));
    }

    #[test]
    fn test_mat2_add_sub() {
        let a = Mat2::new(1.0, 2.0, 3.0, 4.0);
        let b = Mat2::new(5.0, 6.0, 7.0, 8.0);
        let sum = a + b;
        assert!(mat2_approx_eq(sum, Mat2::new(6.0, 8.0, 10.0, 12.0)));
        let diff = sum - b;
        assert!(mat2_approx_eq(diff, a));
    }
}
