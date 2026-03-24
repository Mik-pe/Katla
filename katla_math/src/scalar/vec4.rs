//! Scalar (fallback) implementation of Vec4
//!
//! This is used when SSE intrinsics are not available or when the scalar
//! implementation is explicitly preferred.

use core::{
    f32,
    ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign},
};

/// 4-dimensional vector - scalar implementation
#[derive(Debug, Clone, Copy)]
#[repr(C, align(16))]
pub struct Vec4(pub [f32; 4]);

impl Vec4 {
    pub const ZERO: Vec4 = Vec4([0.0, 0.0, 0.0, 0.0]);
    pub const ONE: Vec4 = Vec4([1.0, 1.0, 1.0, 1.0]);
    pub const X_AXIS: Vec4 = Vec4([1.0, 0.0, 0.0, 0.0]);
    pub const Y_AXIS: Vec4 = Vec4([0.0, 1.0, 0.0, 0.0]);
    pub const Z_AXIS: Vec4 = Vec4([0.0, 0.0, 1.0, 0.0]);
    pub const W_AXIS: Vec4 = Vec4([0.0, 0.0, 0.0, 1.0]);

    #[inline]
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
        Vec4([x, y, z, w])
    }

    #[inline]
    pub fn from_xyz(x: f32, y: f32, z: f32) -> Vec4 {
        Vec4::new(x, y, z, 1.0)
    }

    #[inline]
    pub fn from_xyzw(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
        Vec4::new(x, y, z, w)
    }

    #[inline]
    pub fn x(&self) -> f32 {
        self.0[0]
    }

    #[inline]
    pub fn y(&self) -> f32 {
        self.0[1]
    }

    #[inline]
    pub fn z(&self) -> f32 {
        self.0[2]
    }

    #[inline]
    pub fn w(&self) -> f32 {
        self.0[3]
    }

    #[inline]
    pub fn length_squared(&self) -> f32 {
        self.0[0] * self.0[0]
            + self.0[1] * self.0[1]
            + self.0[2] * self.0[2]
            + self.0[3] * self.0[3]
    }

    #[inline]
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn normalize(&self) -> Vec4 {
        let lensq = self.length_squared();
        if lensq == 0.0 {
            return Vec4::ZERO;
        }
        let len = lensq.sqrt();
        Vec4([
            self.0[0] / len,
            self.0[1] / len,
            self.0[2] / len,
            self.0[3] / len,
        ])
    }

    #[inline]
    pub fn is_normalized(&self) -> bool {
        let lensq = self.length_squared();
        f32::abs(lensq - 1.0) < f32::EPSILON
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0[0] == 0.0 && self.0[1] == 0.0 && self.0[2] == 0.0 && self.0[3] == 0.0
    }

    #[inline]
    pub fn dot(&self, other: &Vec4) -> f32 {
        self.0[0] * other.0[0]
            + self.0[1] * other.0[1]
            + self.0[2] * other.0[2]
            + self.0[3] * other.0[3]
    }

    #[inline]
    pub fn lerp(&self, other: &Vec4, t: f32) -> Vec4 {
        Vec4([
            self.0[0] + (other.0[0] - self.0[0]) * t,
            self.0[1] + (other.0[1] - self.0[1]) * t,
            self.0[2] + (other.0[2] - self.0[2]) * t,
            self.0[3] + (other.0[3] - self.0[3]) * t,
        ])
    }

    #[inline]
    pub fn xyz(&self) -> crate::Vec3 {
        crate::Vec3::new(self.0[0], self.0[1], self.0[2])
    }
}

impl Index<usize> for Vec4 {
    type Output = f32;

    #[inline]
    fn index(&self, index: usize) -> &f32 {
        &self.0[index]
    }
}

impl IndexMut<usize> for Vec4 {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl From<Vec4> for [f32; 4] {
    #[inline]
    fn from(val: Vec4) -> Self {
        val.0
    }
}

impl From<[f32; 4]> for Vec4 {
    #[inline]
    fn from(val: [f32; 4]) -> Self {
        Vec4(val)
    }
}

impl Default for Vec4 {
    #[inline]
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

impl PartialEq for Vec4 {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0[0] == other.0[0]
            && self.0[1] == other.0[1]
            && self.0[2] == other.0[2]
            && self.0[3] == other.0[3]
    }
}

// Arithmetic traits
impl Add for Vec4 {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Vec4([
            self.0[0] + other.0[0],
            self.0[1] + other.0[1],
            self.0[2] + other.0[2],
            self.0[3] + other.0[3],
        ])
    }
}

impl Add<&Vec4> for Vec4 {
    type Output = Self;

    #[inline]
    fn add(self, other: &Self) -> Self {
        Vec4([
            self.0[0] + other.0[0],
            self.0[1] + other.0[1],
            self.0[2] + other.0[2],
            self.0[3] + other.0[3],
        ])
    }
}

impl Add<Vec4> for &Vec4 {
    type Output = Vec4;

    #[inline]
    fn add(self, other: Vec4) -> Vec4 {
        Vec4([
            self.0[0] + other.0[0],
            self.0[1] + other.0[1],
            self.0[2] + other.0[2],
            self.0[3] + other.0[3],
        ])
    }
}

impl Add<&Vec4> for &Vec4 {
    type Output = Vec4;

    #[inline]
    fn add(self, other: &Vec4) -> Vec4 {
        Vec4([
            self.0[0] + other.0[0],
            self.0[1] + other.0[1],
            self.0[2] + other.0[2],
            self.0[3] + other.0[3],
        ])
    }
}

impl Sub for Vec4 {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Vec4([
            self.0[0] - other.0[0],
            self.0[1] - other.0[1],
            self.0[2] - other.0[2],
            self.0[3] - other.0[3],
        ])
    }
}

impl Sub<&Vec4> for Vec4 {
    type Output = Self;

    #[inline]
    fn sub(self, other: &Self) -> Self {
        Vec4([
            self.0[0] - other.0[0],
            self.0[1] - other.0[1],
            self.0[2] - other.0[2],
            self.0[3] - other.0[3],
        ])
    }
}

impl Sub<Vec4> for &Vec4 {
    type Output = Vec4;

    #[inline]
    fn sub(self, other: Vec4) -> Vec4 {
        Vec4([
            self.0[0] - other.0[0],
            self.0[1] - other.0[1],
            self.0[2] - other.0[2],
            self.0[3] - other.0[3],
        ])
    }
}

impl Sub<&Vec4> for &Vec4 {
    type Output = Vec4;

    #[inline]
    fn sub(self, other: &Vec4) -> Vec4 {
        Vec4([
            self.0[0] - other.0[0],
            self.0[1] - other.0[1],
            self.0[2] - other.0[2],
            self.0[3] - other.0[3],
        ])
    }
}

impl Mul<f32> for Vec4 {
    type Output = Self;

    #[inline]
    fn mul(self, scalar: f32) -> Self {
        Vec4([
            self.0[0] * scalar,
            self.0[1] * scalar,
            self.0[2] * scalar,
            self.0[3] * scalar,
        ])
    }
}

impl Mul<Vec4> for Vec4 {
    type Output = Self;

    #[inline]
    fn mul(self, other: Self) -> Self {
        Vec4([
            self.0[0] * other.0[0],
            self.0[1] * other.0[1],
            self.0[2] * other.0[2],
            self.0[3] * other.0[3],
        ])
    }
}

impl Div<f32> for Vec4 {
    type Output = Self;

    #[inline]
    fn div(self, scalar: f32) -> Self {
        Vec4([
            self.0[0] / scalar,
            self.0[1] / scalar,
            self.0[2] / scalar,
            self.0[3] / scalar,
        ])
    }
}

impl Div<Vec4> for Vec4 {
    type Output = Self;

    #[inline]
    fn div(self, other: Self) -> Self {
        Vec4([
            self.0[0] / other.0[0],
            self.0[1] / other.0[1],
            self.0[2] / other.0[2],
            self.0[3] / other.0[3],
        ])
    }
}

impl Neg for Vec4 {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        Vec4([-self.0[0], -self.0[1], -self.0[2], -self.0[3]])
    }
}

impl Neg for &Vec4 {
    type Output = Vec4;

    #[inline]
    fn neg(self) -> Vec4 {
        Vec4([-self.0[0], -self.0[1], -self.0[2], -self.0[3]])
    }
}

// Assignment traits
impl AddAssign for Vec4 {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.0[0] += other.0[0];
        self.0[1] += other.0[1];
        self.0[2] += other.0[2];
        self.0[3] += other.0[3];
    }
}

impl SubAssign for Vec4 {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        self.0[0] -= other.0[0];
        self.0[1] -= other.0[1];
        self.0[2] -= other.0[2];
        self.0[3] -= other.0[3];
    }
}

impl MulAssign<f32> for Vec4 {
    #[inline]
    fn mul_assign(&mut self, scalar: f32) {
        self.0[0] *= scalar;
        self.0[1] *= scalar;
        self.0[2] *= scalar;
        self.0[3] *= scalar;
    }
}

impl DivAssign<f32> for Vec4 {
    #[inline]
    fn div_assign(&mut self, scalar: f32) {
        self.0[0] /= scalar;
        self.0[1] /= scalar;
        self.0[2] /= scalar;
        self.0[3] /= scalar;
    }
}
