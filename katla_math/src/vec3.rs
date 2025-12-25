use core::{
    f32,
    ops::{Add, Index, IndexMut, Sub},
};
use std::ops::{AddAssign, Mul, Neg, SubAssign};

#[derive(Debug, Copy, Clone)]
pub struct Vec3(pub [f32; 3]);

impl Index<usize> for Vec3 {
    type Output = f32;
    fn index(&self, index: usize) -> &f32 {
        match index {
            0 => &self.0[0],
            1 => &self.0[1],
            2 => &self.0[2],
            _ => panic!("INDEXING OUT_OF_BOUNDS in Vec3"),
        }
    }
}

//Make accessors for x/y/z
impl Vec3 {
    pub fn x(&self) -> f32 {
        self.0[0]
    }

    pub fn y(&self) -> f32 {
        self.0[1]
    }

    pub fn z(&self) -> f32 {
        self.0[2]
    }
}

impl IndexMut<usize> for Vec3 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.0[0],
            1 => &mut self.0[1],
            2 => &mut self.0[2],
            _ => panic!("INDEXING OUT_OF_BOUNDS in Vec3"),
        }
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self([self[0] - other[0], self[1] - other[1], self[2] - other[2]])
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self([self[0] + other[0], self[1] + other[1], self[2] + other[2]])
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, other: Self) {
        self.0[0] += other[0];
        self.0[1] += other[1];
        self.0[2] += other[2];
    }
}

impl From<[f32; 3]> for Vec3 {
    fn from(val: [f32; 3]) -> Self {
        Vec3(val)
    }
}

impl<'a> From<&'a [f32; 3]> for &'a Vec3 {
    fn from(other: &'a [f32; 3]) -> &'a Vec3 {
        unsafe { std::mem::transmute(other) }
    }
}

impl Vec3 {
    #[inline]
    pub fn new(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3([x, y, z])
    }

    #[inline]
    pub fn add(&self, _rhs: &Vec3) -> Vec3 {
        Vec3([self[0] + _rhs[0], self[1] + _rhs[1], self[2] + _rhs[2]])
    }

    #[inline]
    pub fn mul(&self, _rhs: f32) -> Vec3 {
        Vec3([self[0] * _rhs, self[1] * _rhs, self[2] * _rhs])
    }

    #[inline]
    pub fn normalize(&self) -> Vec3 {
        let lensq = self[0] * self[0] + self[1] * self[1] + self[2] * self[2];
        if lensq == 0.0 {
            return Vec3([0.0, 0.0, 0.0]);
        }
        let lenroot = f32::sqrt(lensq);
        Vec3([self[0] / lenroot, self[1] / lenroot, self[2] / lenroot])
    }

    #[inline]
    pub fn is_normalized(&self) -> bool {
        let lensq = self[0] * self[0] + self[1] * self[1] + self[2] * self[2];
        f32::abs(lensq - 1.0) < f32::EPSILON
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self[0] == 0.0 && self[1] == 0.0 && self[2] == 0.0
    }

    pub fn distance_squared(&self) -> f32 {
        self[0] * self[0] + self[1] * self[1] + self[2] * self[2]
    }

    pub fn distance(&self) -> f32 {
        self.distance_squared().sqrt()
    }

    #[inline]
    pub fn dot(&self, b: Vec3) -> f32 {
        self[0] * b[0] + self[1] * b[1] + self[2] * b[2]
    }

    #[inline]
    pub fn cross(&self, b: Vec3) -> Self {
        Self([
            self[1] * b[2] - self[2] * b[1],
            self[2] * b[0] - self[0] * b[2],
            self[0] * b[1] - self[1] * b[0],
        ])
    }

    #[inline]
    //Linearly interpolate from a to b with a given ratio
    pub fn lerp(a: Vec3, b: Vec3, ratio: f32) -> Self {
        a + ((b - a) * ratio)
    }
}

impl Mul for Vec3 {
    type Output = Vec3;

    fn mul(self, rhs: Self) -> Self::Output {
        Vec3([self[0] * rhs[0], self[1] * rhs[1], self[2] * rhs[2]])
    }
}

impl Mul<Vec3> for f32 {
    type Output = Vec3;

    fn mul(self, rhs: Vec3) -> Self::Output {
        Vec3([self * rhs[0], self * rhs[1], self * rhs[2]])
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;

    fn mul(self, rhs: f32) -> Self::Output {
        Vec3([self[0] * rhs, self[1] * rhs, self[2] * rhs])
    }
}

impl Default for Vec3 {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }
}

impl PartialEq for Vec3 {
    fn eq(&self, other: &Self) -> bool {
        self[0] == other[0] && self[1] == other[1] && self[2] == other[2]
    }
}

impl Neg for Vec3 {
    type Output = Vec3;

    fn neg(self) -> Self::Output {
        Vec3([-self[0], -self[1], -self[2]])
    }
}

impl Neg for &Vec3 {
    type Output = Vec3;

    fn neg(self) -> Self::Output {
        Vec3([-self[0], -self[1], -self[2]])
    }
}

impl SubAssign<f32> for Vec3 {
    fn sub_assign(&mut self, rhs: f32) {
        self[0] -= rhs;
        self[1] -= rhs;
        self[2] -= rhs;
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        self[0] -= rhs[0];
        self[1] -= rhs[1];
        self[2] -= rhs[2];
    }
}
