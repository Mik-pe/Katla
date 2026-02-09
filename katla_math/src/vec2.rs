use core::{
    f32,
    ops::{Add, Index, IndexMut, Sub},
};
use std::ops::{AddAssign, Div, DivAssign, Mul, MulAssign, Neg, SubAssign};

#[derive(Debug, Copy, Clone)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

//Make accessors for x/y/z
impl Vec2 {
    pub const X_AXIS: Vec2 = Vec2 { x: 1.0, y: 0.0 };
    pub const Y_AXIS: Vec2 = Vec2 { x: 0.0, y: 1.0 };
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    pub const ONE: Vec2 = Vec2 { x: 1.0, y: 1.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }

    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn length_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn normalize(&self) -> Vec2 {
        let len = self.length();
        if len == 0.0 {
            return Vec2::ZERO;
        }
        Vec2 {
            x: self.x / len,
            y: self.y / len,
        }
    }

    pub fn is_normalized(&self) -> bool {
        (self.length_squared() - 1.0).abs() < f32::EPSILON
    }

    pub fn is_zero(&self) -> bool {
        self.x == 0.0 && self.y == 0.0
    }

    pub fn dot(&self, other: &Vec2) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn lerp(&self, other: &Vec2, t: f32) -> Vec2 {
        *self + (*other - *self) * t
    }

    /// 2D cross product returns scalar (z-component of 3D cross product)
    pub fn cross(&self, other: &Vec2) -> f32 {
        self.x * other.y - self.y * other.x
    }

    /// Perpendicular vector (rotated 90 degrees counter-clockwise)
    pub fn perpendicular(&self) -> Vec2 {
        Vec2 {
            x: -self.y,
            y: self.x,
        }
    }

    /// Angle from +X axis in radians
    pub fn angle(&self) -> f32 {
        f32::atan2(self.y, self.x)
    }

    /// Create a unit vector at the given angle from +X axis
    pub fn from_angle(angle: f32) -> Vec2 {
        Vec2 {
            x: f32::cos(angle),
            y: f32::sin(angle),
        }
    }

    pub fn distance(&self, other: &Vec2) -> f32 {
        (*self - *other).length()
    }

    pub fn distance_squared(&self, other: &Vec2) -> f32 {
        (*self - *other).length_squared()
    }

    /// Swizzle operations
    pub fn xx(&self) -> Vec2 {
        Vec2 { x: self.x, y: self.x }
    }

    pub fn yx(&self) -> Vec2 {
        Vec2 { x: self.y, y: self.x }
    }

    pub fn yy(&self) -> Vec2 {
        Vec2 { x: self.y, y: self.y }
    }
}

impl Index<usize> for Vec2 {
    type Output = f32;
    fn index(&self, index: usize) -> &f32 {
        match index {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("INDEXING OUT_OF_BOUNDS in Vec2"),
        }
    }
}

impl IndexMut<usize> for Vec2 {
    fn index_mut(&mut self, index: usize) -> &mut f32 {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => panic!("INDEXING OUT_OF_BOUNDS in Vec2"),
        }
    }
}

impl Default for Vec2 {
    fn default() -> Self {
        Vec2 { x: 0.0, y: 0.0 }
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, other: Self) {
        self.x += other.x;
        self.y += other.y;
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, other: Self) {
        self.x -= other.x;
        self.y -= other.y;
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;

    fn mul(self, scalar: f32) -> Self {
        Self {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
}

impl Div<f32> for Vec2 {
    type Output = Self;

    fn div(self, scalar: f32) -> Self {
        Self {
            x: self.x / scalar,
            y: self.y / scalar,
        }
    }
}

impl Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

impl MulAssign<f32> for Vec2 {
    fn mul_assign(&mut self, scalar: f32) {
        self.x *= scalar;
        self.y *= scalar;
    }
}

impl DivAssign<f32> for Vec2 {
    fn div_assign(&mut self, scalar: f32) {
        self.x /= scalar;
        self.y /= scalar;
    }
}

impl PartialEq for Vec2 {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }
}

impl Eq for Vec2 {}

impl PartialOrd for Vec2 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.length().partial_cmp(&other.length())
    }
}
