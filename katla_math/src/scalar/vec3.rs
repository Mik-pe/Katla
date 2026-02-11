//! Scalar (fallback) implementation of Vec3
//!
//! This is used when SSE intrinsics are not available or when the scalar
//! implementation is explicitly preferred.

use core::{
    f32,
    ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign},
};

/// 3-dimensional vector - scalar implementation
#[derive(Debug, Clone, Copy)]
#[repr(C, align(16))]
pub struct Vec3(pub [f32; 4]); // Use 4 elements for consistent alignment

impl Vec3 {
    pub const X_AXIS: Vec3 = Vec3([1.0, 0.0, 0.0, 0.0]);
    pub const Y_AXIS: Vec3 = Vec3([0.0, 1.0, 0.0, 0.0]);
    pub const Z_AXIS: Vec3 = Vec3([0.0, 0.0, 1.0, 0.0]);

    #[inline]
    pub fn new(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3([x, y, z, 0.0])
    }

    #[inline]
    pub fn zero() -> Vec3 {
        Vec3([0.0, 0.0, 0.0, 0.0])
    }

    #[inline]
    pub fn x_axis() -> Vec3 {
        Vec3::X_AXIS
    }

    #[inline]
    pub fn y_axis() -> Vec3 {
        Vec3::Y_AXIS
    }

    #[inline]
    pub fn z_axis() -> Vec3 {
        Vec3::Z_AXIS
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
    pub fn add(&self, _rhs: &Vec3) -> Vec3 {
        Vec3([
            self.0[0] + _rhs.0[0],
            self.0[1] + _rhs.0[1],
            self.0[2] + _rhs.0[2],
            0.0,
        ])
    }

    #[inline]
    pub fn mul(&self, _rhs: f32) -> Vec3 {
        Vec3([self.0[0] * _rhs, self.0[1] * _rhs, self.0[2] * _rhs, 0.0])
    }

    #[inline]
    pub fn normalize(&self) -> Vec3 {
        let lensq = self.length_squared();
        if lensq == 0.0 {
            return Vec3([0.0, 0.0, 0.0, 0.0]);
        }
        let lenroot = f32::sqrt(lensq);
        Vec3([
            self.0[0] / lenroot,
            self.0[1] / lenroot,
            self.0[2] / lenroot,
            0.0,
        ])
    }

    #[inline]
    pub fn is_normalized(&self) -> bool {
        let lensq = self.length_squared();
        f32::abs(lensq - 1.0) < f32::EPSILON
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0[0] == 0.0 && self.0[1] == 0.0 && self.0[2] == 0.0
    }

    #[inline]
    pub fn length_squared(&self) -> f32 {
        self.0[0] * self.0[0] + self.0[1] * self.0[1] + self.0[2] * self.0[2]
    }

    #[inline]
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn dot(&self, b: Vec3) -> f32 {
        self.0[0] * b.0[0] + self.0[1] * b.0[1] + self.0[2] * b.0[2]
    }

    #[inline]
    pub fn cross(&self, b: Vec3) -> Self {
        Vec3([
            self.0[1] * b.0[2] - self.0[2] * b.0[1],
            self.0[2] * b.0[0] - self.0[0] * b.0[2],
            self.0[0] * b.0[1] - self.0[1] * b.0[0],
            0.0,
        ])
    }

    #[inline]
    pub fn lerp(a: Vec3, b: Vec3, ratio: f32) -> Self {
        a + ((b - a) * ratio)
    }

    #[inline]
    pub fn reflect(&self, normal: Vec3) -> Vec3 {
        *self - normal * 2.0 * self.dot(normal)
    }

    #[inline]
    pub fn project(&self, onto: Vec3) -> Vec3 {
        onto * (self.dot(onto) / onto.dot(onto))
    }

    #[inline]
    pub fn reject(&self, from: Vec3) -> Vec3 {
        *self - self.project(from)
    }

    #[inline]
    pub fn distance(&self, other: &Vec3) -> f32 {
        (*self - *other).length()
    }

    #[inline]
    pub fn distance_squared(&self, other: &Vec3) -> f32 {
        (*self - *other).length_squared()
    }

    #[inline]
    pub fn angle_between(&self, other: &Vec3) -> f32 {
        let dot = self.dot(*other);
        let cross = self.cross(*other);
        let cross_len = cross.length();
        f32::atan2(cross_len, dot)
    }

    #[inline]
    pub fn clamp_length(&self, max: f32) -> Vec3 {
        if max < 0.0 {
            return Vec3::new(0.0, 0.0, 0.0);
        }
        let len = self.length();
        if len > max {
            *self * (max / len)
        } else {
            *self
        }
    }

    #[inline]
    pub fn clamp_length_min_max(&self, min: f32, max: f32) -> Vec3 {
        if max < min {
            return Vec3::new(0.0, 0.0, 0.0);
        }
        let len = self.length();
        if len < min {
            *self * (if len > 0.0 { min / len } else { 0.0 })
        } else if len > max {
            *self * (max / len)
        } else {
            *self
        }
    }

    #[inline]
    pub fn from_spherical(phi: f32, theta: f32) -> Vec3 {
        let sin_phi = f32::sin(phi);
        Vec3::new(
            sin_phi * f32::cos(theta),
            f32::cos(phi),
            sin_phi * f32::sin(theta),
        )
    }

    /// Convert to [f32; 3] array (excluding padding element)
    #[inline]
    pub fn to_array(&self) -> [f32; 3] {
        [self.0[0], self.0[1], self.0[2]]
    }

    /// Convert to [f32; 4] array (including padding)
    #[inline]
    pub fn to_array_4(&self) -> [f32; 4] {
        self.0
    }
}

impl Index<usize> for Vec3 {
    type Output = f32;

    #[inline]
    fn index(&self, index: usize) -> &f32 {
        assert!(index < 3, "INDEXING OUT_OF_BOUNDS in Vec3");
        &self.0[index]
    }
}

impl IndexMut<usize> for Vec3 {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        assert!(index < 3, "INDEXING OUT_OF_BOUNDS in Vec3");
        &mut self.0[index]
    }
}

impl From<[f32; 3]> for Vec3 {
    #[inline]
    fn from(val: [f32; 3]) -> Self {
        Vec3([val[0], val[1], val[2], 0.0])
    }
}

impl Sub for Vec3 {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Vec3([
            self.0[0] - other.0[0],
            self.0[1] - other.0[1],
            self.0[2] - other.0[2],
            0.0,
        ])
    }
}

impl Add for Vec3 {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Vec3([
            self.0[0] + other.0[0],
            self.0[1] + other.0[1],
            self.0[2] + other.0[2],
            0.0,
        ])
    }
}

impl AddAssign for Vec3 {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.0[0] += other.0[0];
        self.0[1] += other.0[1];
        self.0[2] += other.0[2];
    }
}

impl Mul for Vec3 {
    type Output = Vec3;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        Vec3([
            self.0[0] * rhs.0[0],
            self.0[1] * rhs.0[1],
            self.0[2] * rhs.0[2],
            0.0,
        ])
    }
}

impl Mul<Vec3> for f32 {
    type Output = Vec3;

    #[inline]
    fn mul(self, rhs: Vec3) -> Self::Output {
        Vec3([self * rhs.0[0], self * rhs.0[1], self * rhs.0[2], 0.0])
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;

    #[inline]
    fn mul(self, rhs: f32) -> Self::Output {
        Vec3([self.0[0] * rhs, self.0[1] * rhs, self.0[2] * rhs, 0.0])
    }
}

impl Div for Vec3 {
    type Output = Vec3;

    #[inline]
    fn div(self, rhs: Self) -> Self::Output {
        Vec3([
            self.0[0] / rhs.0[0],
            self.0[1] / rhs.0[1],
            self.0[2] / rhs.0[2],
            0.0,
        ])
    }
}

impl MulAssign for Vec3 {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        self.0[0] *= rhs.0[0];
        self.0[1] *= rhs.0[1];
        self.0[2] *= rhs.0[2];
    }
}

impl MulAssign<f32> for Vec3 {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        self.0[0] *= rhs;
        self.0[1] *= rhs;
        self.0[2] *= rhs;
    }
}

impl Div<f32> for Vec3 {
    type Output = Vec3;

    #[inline]
    fn div(self, rhs: f32) -> Self::Output {
        Vec3([self.0[0] / rhs, self.0[1] / rhs, self.0[2] / rhs, 0.0])
    }
}

impl DivAssign for Vec3 {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        self.0[0] /= rhs.0[0];
        self.0[1] /= rhs.0[1];
        self.0[2] /= rhs.0[2];
    }
}

impl DivAssign<f32> for Vec3 {
    #[inline]
    fn div_assign(&mut self, rhs: f32) {
        self.0[0] /= rhs;
        self.0[1] /= rhs;
        self.0[2] /= rhs;
    }
}

impl Default for Vec3 {
    #[inline]
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }
}

impl PartialEq for Vec3 {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0[0] == other.0[0] && self.0[1] == other.0[1] && self.0[2] == other.0[2]
    }
}

impl Neg for Vec3 {
    type Output = Vec3;

    #[inline]
    fn neg(self) -> Self::Output {
        Vec3([-self.0[0], -self.0[1], -self.0[2], 0.0])
    }
}

impl Neg for &Vec3 {
    type Output = Vec3;

    #[inline]
    fn neg(self) -> Self::Output {
        Vec3([-self.0[0], -self.0[1], -self.0[2], 0.0])
    }
}

impl SubAssign<f32> for Vec3 {
    #[inline]
    fn sub_assign(&mut self, rhs: f32) {
        self.0[0] -= rhs;
        self.0[1] -= rhs;
        self.0[2] -= rhs;
    }
}

impl SubAssign for Vec3 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0[0] -= rhs.0[0];
        self.0[1] -= rhs.0[1];
        self.0[2] -= rhs.0[2];
    }
}
