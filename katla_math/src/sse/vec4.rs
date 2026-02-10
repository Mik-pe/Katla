//! SSE-accelerated implementation of Vec4
//!
//! This uses SSE intrinsics for high-performance vector operations.
//! Only available on x86/x86_64 with SSE2 support.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;

use core::{
    f32,
    ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign},
};

/// 4-dimensional vector - SSE implementation
#[derive(Debug, Clone, Copy)]
#[repr(C, align(16))]
pub struct Vec4(pub __m128);

impl Vec4 {
    #[inline]
    pub fn zero() -> Vec4 {
        Vec4(unsafe { _mm_setzero_ps() })
    }

    #[inline]
    pub fn one() -> Vec4 {
        Vec4(unsafe { _mm_set_ps(1.0, 1.0, 1.0, 1.0) })
    }

    #[inline]
    pub fn x_axis() -> Vec4 {
        Vec4(unsafe { _mm_set_ps(0.0, 0.0, 0.0, 1.0) })
    }

    #[inline]
    pub fn y_axis() -> Vec4 {
        Vec4(unsafe { _mm_set_ps(0.0, 0.0, 1.0, 0.0) })
    }

    #[inline]
    pub fn z_axis() -> Vec4 {
        Vec4(unsafe { _mm_set_ps(0.0, 1.0, 0.0, 0.0) })
    }

    #[inline]
    pub fn w_axis() -> Vec4 {
        Vec4(unsafe { _mm_set_ps(1.0, 0.0, 0.0, 0.0) })
    }

    #[inline]
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
        Vec4(unsafe { _mm_set_ps(w, z, y, x) })
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
        unsafe { _mm_cvtss_f32(self.0) }
    }

    #[inline]
    pub fn y(&self) -> f32 {
        unsafe {
            let swapped = _mm_shuffle_ps(self.0, self.0, 0b01_01_01_01);
            _mm_cvtss_f32(swapped)
        }
    }

    #[inline]
    pub fn z(&self) -> f32 {
        unsafe {
            let swapped = _mm_shuffle_ps(self.0, self.0, 0b10_10_10_10);
            _mm_cvtss_f32(swapped)
        }
    }

    #[inline]
    pub fn w(&self) -> f32 {
        unsafe {
            let swapped = _mm_shuffle_ps(self.0, self.0, 0b11_11_11_11);
            _mm_cvtss_f32(swapped)
        }
    }

    #[inline]
    pub fn length_squared(&self) -> f32 {
        unsafe {
            let sq = _mm_mul_ps(self.0, self.0);
            // Horizontal add: [a, b, c, d] -> [a+b, c+d, a+b, c+d] -> [a+b+c+d, ...]
            let hadd1 = _mm_hadd_ps(sq, sq);
            let hadd2 = _mm_hadd_ps(hadd1, hadd1);
            _mm_cvtss_f32(hadd2)
        }
    }

    #[inline]
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn normalize(&self) -> Vec4 {
        let lensq = self.length_squared();
        if lensq == 0.0 {
            return Vec4::zero();
        }
        let len = lensq.sqrt();
        Vec4(unsafe { _mm_div_ps(self.0, _mm_set_ps1(len)) })
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
        // Simplified implementation using horizontal add
        unsafe {
            let mul = _mm_mul_ps(self.0, other.0);
            // Horizontal add: [a, b, c, d] -> [a+b, c+d, a+b, c+d] -> [a+b+c+d, ...]
            let hadd1 = _mm_hadd_ps(mul, mul);
            let hadd2 = _mm_hadd_ps(hadd1, hadd1);
            _mm_cvtss_f32(hadd2)
        }
    }

    #[inline]
    pub fn lerp(&self, other: &Vec4, t: f32) -> Vec4 {
        unsafe {
            let t_vec = _mm_set_ps1(t);
            let diff = _mm_sub_ps(other.0, self.0);
            Vec4(_mm_add_ps(self.0, _mm_mul_ps(diff, t_vec)))
        }
    }

    #[inline]
    pub fn xyz(&self) -> crate::Vec3 {
        crate::Vec3::new(self[0], self[1], self[2])
    }
}

impl Index<usize> for Vec4 {
    type Output = f32;

    #[inline]
    fn index(&self, index: usize) -> &f32 {
        // SAFETY: Vec4 is repr(C) and repr(align(16)), so we can treat it as a pointer to f32 array
        unsafe {
            assert!(index < 4);
            &*(self as *const Vec4 as *const f32).add(index)
        }
    }
}

impl IndexMut<usize> for Vec4 {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        // SAFETY: Vec4 is repr(C) and repr(align(16)), so we can treat it as a pointer to f32 array
        unsafe {
            assert!(index < 4);
            &mut *(self as *mut Vec4 as *mut f32).add(index)
        }
    }
}

impl From<Vec4> for [f32; 4] {
    #[inline]
    fn from(val: Vec4) -> Self {
        [val[0], val[1], val[2], val[3]]
    }
}

impl From<[f32; 4]> for Vec4 {
    #[inline]
    fn from(val: [f32; 4]) -> Self {
        Vec4(unsafe { _mm_set_ps(val[3], val[2], val[1], val[0]) })
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
        self[0] == other[0] && self[1] == other[1] && self[2] == other[2] && self[3] == other[3]
    }
}

// Arithmetic traits
impl Add for Vec4 {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Vec4(unsafe { _mm_add_ps(self.0, other.0) })
    }
}

impl Add<&Vec4> for Vec4 {
    type Output = Self;

    #[inline]
    fn add(self, other: &Self) -> Self {
        Vec4(unsafe { _mm_add_ps(self.0, other.0) })
    }
}

impl Add<Vec4> for &Vec4 {
    type Output = Vec4;

    #[inline]
    fn add(self, other: Vec4) -> Vec4 {
        Vec4(unsafe { _mm_add_ps(self.0, other.0) })
    }
}

impl Add<&Vec4> for &Vec4 {
    type Output = Vec4;

    #[inline]
    fn add(self, other: &Vec4) -> Vec4 {
        Vec4(unsafe { _mm_add_ps(self.0, other.0) })
    }
}

impl Sub for Vec4 {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Vec4(unsafe { _mm_sub_ps(self.0, other.0) })
    }
}

impl Sub<&Vec4> for Vec4 {
    type Output = Self;

    #[inline]
    fn sub(self, other: &Self) -> Self {
        Vec4(unsafe { _mm_sub_ps(self.0, other.0) })
    }
}

impl Sub<Vec4> for &Vec4 {
    type Output = Vec4;

    #[inline]
    fn sub(self, other: Vec4) -> Vec4 {
        Vec4(unsafe { _mm_sub_ps(self.0, other.0) })
    }
}

impl Sub<&Vec4> for &Vec4 {
    type Output = Vec4;

    #[inline]
    fn sub(self, other: &Vec4) -> Vec4 {
        Vec4(unsafe { _mm_sub_ps(self.0, other.0) })
    }
}

impl Mul<f32> for Vec4 {
    type Output = Self;

    #[inline]
    fn mul(self, scalar: f32) -> Self {
        Vec4(unsafe { _mm_mul_ps(self.0, _mm_set_ps1(scalar)) })
    }
}

impl Mul<Vec4> for Vec4 {
    type Output = Self;

    #[inline]
    fn mul(self, other: Self) -> Self {
        Vec4(unsafe { _mm_mul_ps(self.0, other.0) })
    }
}

impl Div<f32> for Vec4 {
    type Output = Self;

    #[inline]
    fn div(self, scalar: f32) -> Self {
        Vec4(unsafe { _mm_div_ps(self.0, _mm_set_ps1(scalar)) })
    }
}

impl Div<Vec4> for Vec4 {
    type Output = Self;

    #[inline]
    fn div(self, other: Self) -> Self {
        Vec4(unsafe { _mm_div_ps(self.0, other.0) })
    }
}

impl Neg for Vec4 {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        Vec4(unsafe { _mm_sub_ps(_mm_setzero_ps(), self.0) })
    }
}

impl Neg for &Vec4 {
    type Output = Vec4;

    #[inline]
    fn neg(self) -> Vec4 {
        Vec4(unsafe { _mm_sub_ps(_mm_setzero_ps(), self.0) })
    }
}

// Assignment traits
impl AddAssign for Vec4 {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        unsafe {
            self.0 = _mm_add_ps(self.0, other.0);
        }
    }
}

impl SubAssign for Vec4 {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        unsafe {
            self.0 = _mm_sub_ps(self.0, other.0);
        }
    }
}

impl MulAssign<f32> for Vec4 {
    #[inline]
    fn mul_assign(&mut self, scalar: f32) {
        unsafe {
            self.0 = _mm_mul_ps(self.0, _mm_set_ps1(scalar));
        }
    }
}

impl DivAssign<f32> for Vec4 {
    #[inline]
    fn div_assign(&mut self, scalar: f32) {
        unsafe {
            self.0 = _mm_div_ps(self.0, _mm_set_ps1(scalar));
        }
    }
}
