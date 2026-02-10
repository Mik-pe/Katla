//! Scalar (fallback) implementation of Vec2
//!
//! This is used when SSE intrinsics are not available or when the scalar
//! implementation is explicitly preferred.

use core::{
    f32,
    ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign},
};
use std::cmp::Ordering;

/// 2-dimensional vector - scalar implementation
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C, align(16))]
pub struct Vec2(pub [f32; 4]); // Use 4 elements for alignment consistency

impl Vec2 {
    pub const X_AXIS: Vec2 = Vec2([1.0, 0.0, 0.0, 0.0]);
    pub const Y_AXIS: Vec2 = Vec2([0.0, 1.0, 0.0, 0.0]);
    pub const ZERO: Vec2 = Vec2([0.0, 0.0, 0.0, 0.0]);
    pub const ONE: Vec2 = Vec2([1.0, 1.0, 0.0, 0.0]);

    #[inline]
    pub fn new(x: f32, y: f32) -> Self {
        Vec2([x, y, 0.0, 0.0])
    }

    #[inline]
    pub fn zero() -> Vec2 {
        Vec2::ZERO
    }

    #[inline]
    pub fn one() -> Vec2 {
        Vec2::ONE
    }

    #[inline]
    pub fn x_axis() -> Vec2 {
        Vec2::X_AXIS
    }

    #[inline]
    pub fn y_axis() -> Vec2 {
        Vec2::Y_AXIS
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
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn length_squared(&self) -> f32 {
        self.0[0] * self.0[0] + self.0[1] * self.0[1]
    }

    #[inline]
    pub fn normalize(&self) -> Vec2 {
        let len = self.length();
        if len == 0.0 {
            return Vec2::ZERO;
        }
        Vec2::new(self.0[0] / len, self.0[1] / len)
    }

    #[inline]
    pub fn is_normalized(&self) -> bool {
        f32::abs(self.length_squared() - 1.0) < f32::EPSILON
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0[0] == 0.0 && self.0[1] == 0.0
    }

    #[inline]
    pub fn dot(&self, other: &Vec2) -> f32 {
        self.0[0] * other.0[0] + self.0[1] * other.0[1]
    }

    #[inline]
    pub fn lerp(&self, other: &Vec2, t: f32) -> Vec2 {
        Vec2::new(
            self.0[0] + (other.0[0] - self.0[0]) * t,
            self.0[1] + (other.0[1] - self.0[1]) * t,
        )
    }

    #[inline]
    pub fn cross(&self, other: &Vec2) -> f32 {
        self.0[0] * other.0[1] - self.0[1] * other.0[0]
    }

    #[inline]
    pub fn perpendicular(&self) -> Vec2 {
        Vec2::new(-self.0[1], self.0[0])
    }

    #[inline]
    pub fn angle(&self) -> f32 {
        f32::atan2(self.0[1], self.0[0])
    }

    #[inline]
    pub fn from_angle(angle: f32) -> Vec2 {
        Vec2::new(f32::cos(angle), f32::sin(angle))
    }

    #[inline]
    pub fn distance(&self, other: &Vec2) -> f32 {
        (*self - *other).length()
    }

    #[inline]
    pub fn distance_squared(&self, other: &Vec2) -> f32 {
        (*self - *other).length_squared()
    }

    #[inline]
    pub fn xx(&self) -> Vec2 {
        Vec2::new(self.0[0], self.0[0])
    }

    #[inline]
    pub fn yx(&self) -> Vec2 {
        Vec2::new(self.0[1], self.0[0])
    }

    #[inline]
    pub fn yy(&self) -> Vec2 {
        Vec2::new(self.0[1], self.0[1])
    }
}

impl Index<usize> for Vec2 {
    type Output = f32;

    #[inline]
    fn index(&self, index: usize) -> &f32 {
        &self.0[index]
    }
}

impl IndexMut<usize> for Vec2 {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl Default for Vec2 {
    #[inline]
    fn default() -> Self {
        Vec2::new(0.0, 0.0)
    }
}

impl Add for Vec2 {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Vec2::new(self.0[0] + other.0[0], self.0[1] + other.0[1])
    }
}

impl AddAssign for Vec2 {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.0[0] += other.0[0];
        self.0[1] += other.0[1];
    }
}

impl Sub for Vec2 {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Vec2::new(self.0[0] - other.0[0], self.0[1] - other.0[1])
    }
}

impl SubAssign for Vec2 {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        self.0[0] -= other.0[0];
        self.0[1] -= other.0[1];
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;

    #[inline]
    fn mul(self, scalar: f32) -> Self {
        Vec2::new(self.0[0] * scalar, self.0[1] * scalar)
    }
}

impl Div<f32> for Vec2 {
    type Output = Self;

    #[inline]
    fn div(self, scalar: f32) -> Self {
        Vec2::new(self.0[0] / scalar, self.0[1] / scalar)
    }
}

impl Neg for Vec2 {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        Vec2::new(-self.0[0], -self.0[1])
    }
}

impl MulAssign<f32> for Vec2 {
    #[inline]
    fn mul_assign(&mut self, scalar: f32) {
        self.0[0] *= scalar;
        self.0[1] *= scalar;
    }
}

impl DivAssign<f32> for Vec2 {
    #[inline]
    fn div_assign(&mut self, scalar: f32) {
        self.0[0] /= scalar;
        self.0[1] /= scalar;
    }
}

impl PartialOrd for Vec2 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.length().partial_cmp(&other.length())
    }
}
