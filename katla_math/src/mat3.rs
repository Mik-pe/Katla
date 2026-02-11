use crate::{Quat, Vec3};
use core::ops::{Index, IndexMut, Mul};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3(pub [Vec3; 3]);

impl Index<usize> for Mat3 {
    type Output = Vec3;

    fn index(&self, index: usize) -> &Vec3 {
        match index {
            0 => &self.0[0],
            1 => &self.0[1],
            2 => &self.0[2],
            _ => panic!("INDEXING OUT_OF_BOUNDS in Mat3"),
        }
    }
}

impl IndexMut<usize> for Mat3 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.0[0],
            1 => &mut self.0[1],
            2 => &mut self.0[2],
            _ => panic!("INDEXING OUT_OF_BOUNDS in Mat3"),
        }
    }
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Mat3 {
    /// Create a new 3x3 identity matrix
    pub fn new() -> Mat3 {
        Mat3([
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ])
    }

    /// Create a 3x3 identity matrix
    pub fn identity() -> Mat3 {
        Mat3([
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ])
    }

    /// Create a 3x3 scale matrix
    pub fn from_scale(scale: Vec3) -> Self {
        Mat3([
            Vec3::new(scale[0], 0.0, 0.0),
            Vec3::new(0.0, scale[1], 0.0),
            Vec3::new(0.0, 0.0, scale[2]),
        ])
    }

    /// Create a 3x3 rotation matrix from a quaternion
    pub fn from_rotation(rotation: Quat) -> Self {
        // Extract the 3x3 portion of the 4x4 rotation matrix
        let mat4 = rotation.make_mat4();
        Mat3([
            Vec3::new(mat4[0][0], mat4[0][1], mat4[0][2]),
            Vec3::new(mat4[1][0], mat4[1][1], mat4[1][2]),
            Vec3::new(mat4[2][0], mat4[2][1], mat4[2][2]),
        ])
    }

    /// Create a 3x3 rotation matrix from Euler angles (pitch, yaw, roll)
    pub fn from_euler_angles(pitch: f32, yaw: f32, roll: f32) -> Self {
        let q = Quat::from_axis_angle(Vec3::x_axis(), pitch)
            * Quat::from_axis_angle(Vec3::y_axis(), yaw)
            * Quat::from_axis_angle(Vec3::z_axis(), roll);
        Self::from_rotation(q)
    }

    /// Create a 3x3 matrix from individual elements
    /// Parameters: m00, m01, m02, m10, m11, m12, m20, m21, m22
    #[allow(clippy::too_many_arguments)]
    pub fn from_elements(
        m00: f32,
        m01: f32,
        m02: f32,
        m10: f32,
        m11: f32,
        m12: f32,
        m20: f32,
        m21: f32,
        m22: f32,
    ) -> Self {
        Mat3([
            Vec3::new(m00, m01, m02),
            Vec3::new(m10, m11, m12),
            Vec3::new(m20, m21, m22),
        ])
    }

    /// Multiply two 3x3 matrices
    pub fn mul(&self, rhs: &Mat3) -> Mat3 {
        Mat3([
            Vec3::new(
                self[0][0] * rhs[0][0] + self[1][0] * rhs[0][1] + self[2][0] * rhs[0][2],
                self[0][1] * rhs[0][0] + self[1][1] * rhs[0][1] + self[2][1] * rhs[0][2],
                self[0][2] * rhs[0][0] + self[1][2] * rhs[0][1] + self[2][2] * rhs[0][2],
            ),
            Vec3::new(
                self[0][0] * rhs[1][0] + self[1][0] * rhs[1][1] + self[2][0] * rhs[1][2],
                self[0][1] * rhs[1][0] + self[1][1] * rhs[1][1] + self[2][1] * rhs[1][2],
                self[0][2] * rhs[1][0] + self[1][2] * rhs[1][1] + self[2][2] * rhs[1][2],
            ),
            Vec3::new(
                self[0][0] * rhs[2][0] + self[1][0] * rhs[2][1] + self[2][0] * rhs[2][2],
                self[0][1] * rhs[2][0] + self[1][1] * rhs[2][1] + self[2][1] * rhs[2][2],
                self[0][2] * rhs[2][0] + self[1][2] * rhs[2][1] + self[2][2] * rhs[2][2],
            ),
        ])
    }

    /// Transpose the matrix
    pub fn transpose(&self) -> Mat3 {
        Mat3([
            Vec3::new(self[0][0], self[1][0], self[2][0]),
            Vec3::new(self[0][1], self[1][1], self[2][1]),
            Vec3::new(self[0][2], self[1][2], self[2][2]),
        ])
    }

    /// Calculate the determinant of the matrix
    pub fn determinant(&self) -> f32 {
        self[0][0] * (self[1][1] * self[2][2] - self[1][2] * self[2][1])
            - self[0][1] * (self[1][0] * self[2][2] - self[1][2] * self[2][0])
            + self[0][2] * (self[1][0] * self[2][1] - self[1][1] * self[2][0])
    }

    /// Calculate the inverse of the matrix
    /// Returns None if the matrix is singular (determinant is 0)
    pub fn inverse(&self) -> Option<Mat3> {
        let det = self.determinant();
        if det.abs() < 1e-6 {
            return None;
        }

        let inv_det = 1.0 / det;

        Some(Mat3([
            Vec3::new(
                (self[1][1] * self[2][2] - self[1][2] * self[2][1]) * inv_det,
                (self[0][2] * self[2][1] - self[0][1] * self[2][2]) * inv_det,
                (self[0][1] * self[1][2] - self[0][2] * self[1][1]) * inv_det,
            ),
            Vec3::new(
                (self[1][2] * self[2][0] - self[1][0] * self[2][2]) * inv_det,
                (self[0][0] * self[2][2] - self[0][2] * self[2][0]) * inv_det,
                (self[0][2] * self[1][0] - self[0][0] * self[1][2]) * inv_det,
            ),
            Vec3::new(
                (self[1][0] * self[2][1] - self[1][1] * self[2][0]) * inv_det,
                (self[0][1] * self[2][0] - self[0][0] * self[2][1]) * inv_det,
                (self[0][0] * self[1][1] - self[0][1] * self[1][0]) * inv_det,
            ),
        ]))
    }
}

// Matrix-matrix multiplication traits
impl Mul for Mat3 {
    type Output = Mat3;

    fn mul(self, rhs: Self) -> Self::Output {
        self.mul(&rhs)
    }
}

impl Mul<&Mat3> for Mat3 {
    type Output = Mat3;

    fn mul(self, rhs: &Mat3) -> Mat3 {
        Mat3::mul(&self, rhs)
    }
}

impl Mul<Mat3> for &Mat3 {
    type Output = Mat3;

    fn mul(self, rhs: Mat3) -> Mat3 {
        Mat3::mul(self, &rhs)
    }
}

impl Mul<&Mat3> for &Mat3 {
    type Output = Mat3;

    fn mul(self, rhs: &Mat3) -> Mat3 {
        Mat3::mul(self, rhs)
    }
}

// Matrix-vector multiplication
impl Mul<Vec3> for Mat3 {
    type Output = Vec3;

    fn mul(self, rhs: Vec3) -> Vec3 {
        Vec3::new(
            self[0][0] * rhs[0] + self[1][0] * rhs[1] + self[2][0] * rhs[2],
            self[0][1] * rhs[0] + self[1][1] * rhs[1] + self[2][1] * rhs[2],
            self[0][2] * rhs[0] + self[1][2] * rhs[1] + self[2][2] * rhs[2],
        )
    }
}

impl Mul<&Vec3> for &Mat3 {
    type Output = Vec3;

    fn mul(self, rhs: &Vec3) -> Vec3 {
        Vec3::new(
            self[0][0] * rhs[0] + self[1][0] * rhs[1] + self[2][0] * rhs[2],
            self[0][1] * rhs[0] + self[1][1] * rhs[1] + self[2][1] * rhs[2],
            self[0][2] * rhs[0] + self[1][2] * rhs[1] + self[2][2] * rhs[2],
        )
    }
}

// Conversions from other types
impl From<Quat> for Mat3 {
    fn from(q: Quat) -> Self {
        Mat3::from_rotation(q)
    }
}

impl Mat3 {
    /// Convert 3x3 matrix to 4x4 matrix (embed in upper-left)
    pub fn to_mat4(&self) -> crate::Mat4 {
        crate::Mat4([
            crate::Vec4::new(self[0][0], self[0][1], self[0][2], 0.0),
            crate::Vec4::new(self[1][0], self[1][1], self[1][2], 0.0),
            crate::Vec4::new(self[2][0], self[2][1], self[2][2], 0.0),
            crate::Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }
}

impl From<crate::Mat4> for Mat3 {
    fn from(m: crate::Mat4) -> Self {
        Mat3([
            Vec3::new(m[0][0], m[0][1], m[0][2]),
            Vec3::new(m[1][0], m[1][1], m[1][2]),
            Vec3::new(m[2][0], m[2][1], m[2][2]),
        ])
    }
}
