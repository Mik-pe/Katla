use core::{
    f32,
    ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign},
};

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
    pub const X_AXIS: Vec3 = Vec3([1.0, 0.0, 0.0]);
    pub const Y_AXIS: Vec3 = Vec3([0.0, 1.0, 0.0]);
    pub const Z_AXIS: Vec3 = Vec3([0.0, 0.0, 1.0]);

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

    pub fn length_squared(&self) -> f32 {
        self[0] * self[0] + self[1] * self[1] + self[2] * self[2]
    }

    pub fn length(&self) -> f32 {
        self.length_squared().sqrt()
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

    /// Reflect this vector across a normal
    /// Returns the reflection direction for incident vector
    #[inline]
    pub fn reflect(&self, normal: Vec3) -> Vec3 {
        *self - normal * 2.0 * self.dot(normal)
    }

    /// Project this vector onto another vector
    #[inline]
    pub fn project(&self, onto: Vec3) -> Vec3 {
        onto * (self.dot(onto) / onto.dot(onto))
    }

    /// Get the perpendicular (rejection) component of this vector
    /// Returns the component of this vector that is perpendicular to 'from'
    #[inline]
    pub fn reject(&self, from: Vec3) -> Vec3 {
        *self - self.project(from)
    }

    /// Calculate the distance between this vector and another
    #[inline]
    pub fn distance(&self, other: &Vec3) -> f32 {
        (*self - *other).length()
    }

    /// Calculate the squared distance between this vector and another
    /// More efficient than distance() when you only need to compare distances
    #[inline]
    pub fn distance_squared(&self, other: &Vec3) -> f32 {
        (*self - *other).length_squared()
    }

    /// Calculate the angle between this vector and another in radians
    #[inline]
    pub fn angle_between(&self, other: &Vec3) -> f32 {
        let dot = self.dot(*other);
        let cross = self.cross(*other);
        let cross_len = cross.length();
        f32::atan2(cross_len, dot)
    }

    /// Clamp the length of this vector to a maximum value
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

    /// Clamp the length of this vector to a minimum and maximum value
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

    /// Create a direction vector from spherical coordinates
    /// phi: angle from +Y axis (0 to pi), theta: angle around Y axis (0 to 2pi)
    #[inline]
    pub fn from_spherical(phi: f32, theta: f32) -> Vec3 {
        let sin_phi = f32::sin(phi);
        Vec3::new(
            sin_phi * f32::cos(theta),
            f32::cos(phi),
            sin_phi * f32::sin(theta),
        )
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

impl Div for Vec3 {
    type Output = Vec3;

    fn div(self, rhs: Self) -> Self::Output {
        Vec3([self[0] / rhs[0], self[1] / rhs[1], self[2] / rhs[2]])
    }
}

impl MulAssign for Vec3 {
    fn mul_assign(&mut self, rhs: Self) {
        self.0[0] *= rhs.0[0];
        self.0[1] *= rhs.0[1];
        self.0[2] *= rhs.0[2];
    }
}

impl MulAssign<f32> for Vec3 {
    fn mul_assign(&mut self, rhs: f32) {
        self.0[0] *= rhs;
        self.0[1] *= rhs;
        self.0[2] *= rhs;
    }
}

impl Div<f32> for Vec3 {
    type Output = Vec3;

    fn div(self, rhs: f32) -> Self::Output {
        Vec3([self[0] / rhs, self[1] / rhs, self[2] / rhs])
    }
}

impl DivAssign for Vec3 {
    fn div_assign(&mut self, rhs: Self) {
        self.0[0] /= rhs.0[0];
        self.0[1] /= rhs.0[1];
        self.0[2] /= rhs.0[2];
    }
}

impl DivAssign<f32> for Vec3 {
    fn div_assign(&mut self, rhs: f32) {
        self.0[0] /= rhs;
        self.0[1] /= rhs;
        self.0[2] /= rhs;
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
