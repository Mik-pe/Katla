use core::{
    f32,
    ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec4(pub [f32; 4]);

impl Index<usize> for Vec4 {
    type Output = f32;

    fn index(&self, index: usize) -> &f32 {
        match index {
            0 => &self.0[0],
            1 => &self.0[1],
            2 => &self.0[2],
            3 => &self.0[3],
            _ => panic!("INDEXING OUT_OF_BOUNDS in Vec4"),
        }
    }
}

impl IndexMut<usize> for Vec4 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.0[0],
            1 => &mut self.0[1],
            2 => &mut self.0[2],
            3 => &mut self.0[3],
            _ => panic!("INDEXING OUT_OF_BOUNDS in Vec4"),
        }
    }
}

impl From<Vec4> for [f32; 4] {
    fn from(val: Vec4) -> Self {
        val.0
    }
}

impl From<[f32; 4]> for Vec4 {
    fn from(val: [f32; 4]) -> Self {
        Vec4(val)
    }
}

impl Default for Vec4 {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

// Arithmetic traits
impl Add for Vec4 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self([
            self[0] + other[0],
            self[1] + other[1],
            self[2] + other[2],
            self[3] + other[3],
        ])
    }
}

impl Add<&Vec4> for Vec4 {
    type Output = Self;

    fn add(self, other: &Self) -> Self {
        Self([
            self[0] + other[0],
            self[1] + other[1],
            self[2] + other[2],
            self[3] + other[3],
        ])
    }
}

impl Add<Vec4> for &Vec4 {
    type Output = Vec4;

    fn add(self, other: Vec4) -> Vec4 {
        Vec4([
            self[0] + other[0],
            self[1] + other[1],
            self[2] + other[2],
            self[3] + other[3],
        ])
    }
}

impl Add<&Vec4> for &Vec4 {
    type Output = Vec4;

    fn add(self, other: &Vec4) -> Vec4 {
        Vec4([
            self[0] + other[0],
            self[1] + other[1],
            self[2] + other[2],
            self[3] + other[3],
        ])
    }
}

impl Sub for Vec4 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self([
            self[0] - other[0],
            self[1] - other[1],
            self[2] - other[2],
            self[3] - other[3],
        ])
    }
}

impl Sub<&Vec4> for Vec4 {
    type Output = Self;

    fn sub(self, other: &Self) -> Self {
        Self([
            self[0] - other[0],
            self[1] - other[1],
            self[2] - other[2],
            self[3] - other[3],
        ])
    }
}

impl Sub<Vec4> for &Vec4 {
    type Output = Vec4;

    fn sub(self, other: Vec4) -> Vec4 {
        Vec4([
            self[0] - other[0],
            self[1] - other[1],
            self[2] - other[2],
            self[3] - other[3],
        ])
    }
}

impl Sub<&Vec4> for &Vec4 {
    type Output = Vec4;

    fn sub(self, other: &Vec4) -> Vec4 {
        Vec4([
            self[0] - other[0],
            self[1] - other[1],
            self[2] - other[2],
            self[3] - other[3],
        ])
    }
}

impl Mul<f32> for Vec4 {
    type Output = Self;

    fn mul(self, scalar: f32) -> Self {
        Self([
            self[0] * scalar,
            self[1] * scalar,
            self[2] * scalar,
            self[3] * scalar,
        ])
    }
}

impl Mul<Vec4> for Vec4 {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self([
            self[0] * other[0],
            self[1] * other[1],
            self[2] * other[2],
            self[3] * other[3],
        ])
    }
}

impl Div<f32> for Vec4 {
    type Output = Self;

    fn div(self, scalar: f32) -> Self {
        Self([
            self[0] / scalar,
            self[1] / scalar,
            self[2] / scalar,
            self[3] / scalar,
        ])
    }
}

impl Div<Vec4> for Vec4 {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        Self([
            self[0] / other[0],
            self[1] / other[1],
            self[2] / other[2],
            self[3] / other[3],
        ])
    }
}

impl Neg for Vec4 {
    type Output = Self;

    fn neg(self) -> Self {
        Self([-self[0], -self[1], -self[2], -self[3]])
    }
}

impl Neg for &Vec4 {
    type Output = Vec4;

    fn neg(self) -> Vec4 {
        Vec4([-self[0], -self[1], -self[2], -self[3]])
    }
}

// Assignment traits
impl AddAssign for Vec4 {
    fn add_assign(&mut self, other: Self) {
        self.0[0] += other.0[0];
        self.0[1] += other.0[1];
        self.0[2] += other.0[2];
        self.0[3] += other.0[3];
    }
}

impl SubAssign for Vec4 {
    fn sub_assign(&mut self, other: Self) {
        self.0[0] -= other.0[0];
        self.0[1] -= other.0[1];
        self.0[2] -= other.0[2];
        self.0[3] -= other.0[3];
    }
}

impl MulAssign<f32> for Vec4 {
    fn mul_assign(&mut self, scalar: f32) {
        self.0[0] *= scalar;
        self.0[1] *= scalar;
        self.0[2] *= scalar;
        self.0[3] *= scalar;
    }
}

impl DivAssign<f32> for Vec4 {
    fn div_assign(&mut self, scalar: f32) {
        self.0[0] /= scalar;
        self.0[1] /= scalar;
        self.0[2] /= scalar;
        self.0[3] /= scalar;
    }
}

impl Vec4 {
    // Constants
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
        Vec4([x, y, z, 1.0])
    }

    #[inline]
    pub fn from_xyzw(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
        Vec4([x, y, z, w])
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
        self[0] * self[0] + self[1] * self[1] + self[2] * self[2] + self[3] * self[3]
    }

    #[inline]
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn normalize(&self) -> Vec4 {
        let lensq = self.length_squared();
        if lensq == 0.0 {
            return Vec4([0.0, 0.0, 0.0, 0.0]);
        }
        let len = lensq.sqrt();
        Vec4([
            self[0] / len,
            self[1] / len,
            self[2] / len,
            self[3] / len,
        ])
    }

    #[inline]
    pub fn is_normalized(&self) -> bool {
        let lensq = self.length_squared();
        f32::abs(lensq - 1.0) < f32::EPSILON
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self[0] == 0.0 && self[1] == 0.0 && self[2] == 0.0 && self[3] == 0.0
    }

    #[inline]
    pub fn dot(&self, other: &Vec4) -> f32 {
        self[0] * other[0] + self[1] * other[1] + self[2] * other[2] + self[3] * other[3]
    }

    #[inline]
    pub fn lerp(&self, other: &Vec4, t: f32) -> Vec4 {
        *self + (*other - *self) * t
    }

    #[inline]
    pub fn xyz(&self) -> crate::Vec3 {
        crate::Vec3::new(self[0], self[1], self[2])
    }
}
