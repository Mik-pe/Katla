//! SSE-accelerated implementation of Quat
//!
//! This uses SSE intrinsics for high-performance quaternion operations.
//! Only available on x86/x86_64 with SSE2 support.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;

use crate::{Mat3, Mat4, Vec3, Vec4};
use core::{f32, ops::Index, ops::Mul};

const QUAT_NORMALIZED_THRESHOLD: f32 = 0.001;

// SSE shuffle control masks - _mm_shuffle_ps(dest, src, mask)
// Mask format: [dest[3] dest[2] src[1] src[0]] where each nibble selects an element
const SHUFFLE_Y: i32 = 0b01_01_01_01; // Broadcast element 1 (y)
const SHUFFLE_Z: i32 = 0b10_10_10_10; // Broadcast element 2 (z)
const SHUFFLE_W: i32 = 0b11_11_11_11; // Broadcast element 3 (w)

/// Quaternion - SSE implementation
#[derive(Debug, Clone, Copy)]
#[repr(C, align(16))]
pub struct Quat(pub __m128);

impl Index<usize> for Quat {
    type Output = f32;

    #[inline]
    fn index(&self, index: usize) -> &f32 {
        // SAFETY: Quat is repr(C) and repr(align(16)), so we can treat it as a pointer to f32 array
        unsafe {
            assert!(index < 4, "INDEXING OUT_OF_BOUNDS in Quat");
            &*(self as *const Quat as *const f32).add(index)
        }
    }
}

impl Default for Quat {
    #[inline]
    fn default() -> Self {
        Self::identity()
    }
}

impl Quat {
    #[inline]
    pub fn identity() -> Quat {
        Quat(unsafe { _mm_set_ps(1.0, 0.0, 0.0, 0.0) }) // w, z, y, x
    }

    #[inline]
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Quat {
        Quat(unsafe { _mm_set_ps(w, z, y, x) })
    }

    #[inline]
    pub fn xyzw(&self) -> (f32, f32, f32, f32) {
        (self[0], self[1], self[2], self[3])
    }

    #[inline]
    fn x(&self) -> f32 {
        unsafe { _mm_cvtss_f32(self.0) }
    }

    #[inline]
    fn y(&self) -> f32 {
        unsafe {
            let swapped = _mm_shuffle_ps(self.0, self.0, SHUFFLE_Y);
            _mm_cvtss_f32(swapped)
        }
    }

    #[inline]
    fn z(&self) -> f32 {
        unsafe {
            let swapped = _mm_shuffle_ps(self.0, self.0, SHUFFLE_Z);
            _mm_cvtss_f32(swapped)
        }
    }

    #[inline]
    fn w(&self) -> f32 {
        unsafe {
            let swapped = _mm_shuffle_ps(self.0, self.0, SHUFFLE_W);
            _mm_cvtss_f32(swapped)
        }
    }

    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Quat {
        let axis = axis.normalize();
        let factor = f32::sin(angle / 2.0);

        let x = axis.x() * factor;
        let y = axis.y() * factor;
        let z = axis.z() * factor;
        let w = f32::cos(angle / 2.0);

        let mut quat = Quat::new(x, y, z, w);
        quat.normalize();

        quat
    }

    pub fn from_rotation_between(from: Vec3, to: Vec3) -> Quat {
        let from = from.normalize();
        let to = to.normalize();

        let dot = from.dot(to);
        if dot >= 0.99999 {
            return Quat::identity();
        }

        // Vectors are opposite - pick an arbitrary perpendicular axis
        if dot <= -0.99999 {
            let mut axis = Vec3::X_AXIS.cross(from);
            let len_sq = axis.x() * axis.x() + axis.y() * axis.y() + axis.z() * axis.z();

            if len_sq < 0.0001 {
                axis = Vec3::Y_AXIS.cross(from);
            }

            axis = axis.normalize();
            return Quat::new(axis.x(), axis.y(), axis.z(), 0.0);
        }

        let angle = f32::acos(dot);
        let axis = from.cross(to).normalize();
        Self::from_axis_angle(axis, angle)
    }

    pub fn new_from_yaw_pitch(yaw: f32, pitch: f32) -> Quat {
        let yaw_rotation = Quat::from_axis_angle(Vec3::Y_AXIS, yaw);
        let pitch_rotation = Quat::from_axis_angle(Vec3::X_AXIS, pitch);
        yaw_rotation * pitch_rotation
    }

    #[inline]
    fn length_squared(&self) -> f32 {
        unsafe {
            let sq = _mm_mul_ps(self.0, self.0);
            let hadd1 = _mm_hadd_ps(sq, sq);
            let hadd2 = _mm_hadd_ps(hadd1, hadd1);
            _mm_cvtss_f32(hadd2)
        }
    }

    pub fn is_normalized(&self) -> bool {
        let len_sq = self.length_squared();
        f32::abs(1.0 - len_sq) < QUAT_NORMALIZED_THRESHOLD
    }

    pub fn normalize(&mut self) {
        let len_sq = self.length_squared();
        let len = f32::sqrt(len_sq);
        if len > 0.0 {
            unsafe {
                self.0 = _mm_div_ps(self.0, _mm_set_ps1(len));
            }
        }
    }

    pub fn inverse(&self) -> Self {
        self.conjugate()
    }

    pub fn conjugate(&self) -> Self {
        unsafe {
            // Negate x, y, z but keep w positive
            let mask = _mm_set_ps(0.0, -0.0, -0.0, -0.0); // w, z, y, x
            Quat(_mm_xor_ps(self.0, mask))
        }
    }

    pub fn dot(&self, rhs: Quat) -> f32 {
        unsafe {
            let mul = _mm_mul_ps(self.0, rhs.0);
            let hadd1 = _mm_hadd_ps(mul, mul);
            let hadd2 = _mm_hadd_ps(hadd1, hadd1);
            _mm_cvtss_f32(hadd2)
        }
    }

    pub fn rotate_vec3(&self, v: Vec3) -> Vec3 {
        let u = Vec3::new(self[0], self[1], self[2]);
        let s = self[3];
        2.0 * u.dot(v) * u + (s * s - u.dot(u)) * v + 2.0 * s * u.cross(v)
    }

    pub fn slerp(mut a: Quat, mut b: Quat, ratio: f32) -> Self {
        a.normalize();
        b.normalize();

        let cs = a.dot(b);
        let angle = f32::acos(cs);
        let mut out = Self::identity();

        if f32::abs(angle) >= 0.001 {
            let inv_sin = 1.0f32 / f32::sin(angle);
            let t_angle = ratio * angle;
            let coeff0 = f32::sin(angle - t_angle) * inv_sin;
            let coeff1 = f32::sin(t_angle) * inv_sin;

            unsafe {
                let a_scaled = _mm_mul_ps(a.0, _mm_set_ps1(coeff0));
                let b_scaled = _mm_mul_ps(b.0, _mm_set_ps1(coeff1));
                out.0 = _mm_add_ps(a_scaled, b_scaled);
            }
        } else {
            out = a;
        }

        out.normalize();
        out
    }

    pub fn make_mat4(&self) -> Mat4 {
        let (m00, m01, m02, m10, m11, m12, m20, m21, m22) = self.rotation_matrix_elements();

        Mat4([
            Vec4::new(m00, m01, m02, 0.0),
            Vec4::new(m10, m11, m12, 0.0),
            Vec4::new(m20, m21, m22, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ])
    }

    pub fn to_mat3(self) -> Mat3 {
        let (m00, m01, m02, m10, m11, m12, m20, m21, m22) = self.rotation_matrix_elements();

        Mat3::from_elements(m00, m01, m02, m10, m11, m12, m20, m21, m22)
    }

    /// Compute the 9 elements of the 3x3 rotation matrix from this quaternion.
    /// Returns (m00, m01, m02, m10, m11, m12, m20, m21, m22).
    fn rotation_matrix_elements(&self) -> (f32, f32, f32, f32, f32, f32, f32, f32, f32) {
        let x = self.x();
        let y = self.y();
        let z = self.z();
        let w = self.w();

        let x2 = x + x;
        let y2 = y + y;
        let z2 = z + z;

        let xx = x * x2;
        let xy = x * y2;
        let xz = x * z2;
        let yy = y * y2;
        let yz = y * z2;
        let zz = z * z2;
        let wx = w * x2;
        let wy = w * y2;
        let wz = w * z2;

        let m00 = 1.0 - (yy + zz);
        let m01 = xy + wz;
        let m02 = xz - wy;
        let m10 = xy - wz;
        let m11 = 1.0 - (xx + zz);
        let m12 = yz + wx;
        let m20 = xz + wy;
        let m21 = yz - wx;
        let m22 = 1.0 - (xx + yy);

        (m00, m01, m02, m10, m11, m12, m20, m21, m22)
    }

    pub fn from_euler(pitch: f32, yaw: f32, roll: f32) -> Quat {
        let pitch_rotation = Quat::from_axis_angle(Vec3::X_AXIS, pitch);
        let yaw_rotation = Quat::from_axis_angle(Vec3::Y_AXIS, yaw);
        let roll_rotation = Quat::from_axis_angle(Vec3::Z_AXIS, roll);
        yaw_rotation * pitch_rotation * roll_rotation
    }

    pub fn to_euler(self) -> (f32, f32, f32) {
        let x = self[0];
        let y = self[1];
        let z = self[2];
        let w = self[3];

        // Roll (x-axis rotation)
        let sinr_cosp = 2.0 * (w * x + y * z);
        let cosr_cosp = 1.0 - 2.0 * (x * x + y * y);
        let roll = f32::atan2(sinr_cosp, cosr_cosp);

        // Pitch (y-axis rotation)
        let sinp = 2.0 * (w * y - z * x);
        let pitch = if f32::abs(sinp) >= 1.0 {
            core::f32::consts::PI / 2.0 * sinp.copysign(1.0)
        } else {
            f32::asin(sinp)
        };

        // Yaw (z-axis rotation)
        let siny_cosp = 2.0 * (w * z + x * y);
        let cosy_cosp = 1.0 - 2.0 * (y * y + z * z);
        let yaw = f32::atan2(siny_cosp, cosy_cosp);

        (pitch, yaw, roll)
    }
}

impl From<Mat3> for Quat {
    fn from(m: Mat3) -> Self {
        let trace = m[0][0] + m[1][1] + m[2][2];

        let (x, y, z, w) = if trace > 0.0 {
            let s = f32::sqrt(trace + 1.0) * 2.0;
            let w = 0.25 * s;
            let x = (m[1][2] - m[2][1]) / s;
            let y = (m[2][0] - m[0][2]) / s;
            let z = (m[0][1] - m[1][0]) / s;
            (x, y, z, w)
        } else if (m[0][0] > m[1][1]) && (m[0][0] > m[2][2]) {
            let s = f32::sqrt(1.0 + m[0][0] - m[1][1] - m[2][2]) * 2.0;
            let w = (m[1][2] - m[2][1]) / s;
            let x = 0.25 * s;
            let y = (m[1][0] + m[0][1]) / s;
            let z = (m[2][0] + m[0][2]) / s;
            (x, y, z, w)
        } else if m[1][1] > m[2][2] {
            let s = f32::sqrt(1.0 + m[1][1] - m[0][0] - m[2][2]) * 2.0;
            let w = (m[2][0] - m[0][2]) / s;
            let x = (m[1][0] + m[0][1]) / s;
            let y = 0.25 * s;
            let z = (m[2][1] + m[1][2]) / s;
            (x, y, z, w)
        } else {
            let s = f32::sqrt(1.0 + m[2][2] - m[0][0] - m[1][1]) * 2.0;
            let w = (m[0][1] - m[1][0]) / s;
            let x = (m[2][0] + m[0][2]) / s;
            let y = (m[2][1] + m[1][2]) / s;
            let z = 0.25 * s;
            (x, y, z, w)
        };

        let mut q = Quat::new(x, y, z, w);
        q.normalize();
        q
    }
}

impl From<Mat4> for Quat {
    fn from(m: Mat4) -> Self {
        let mat3 = m.to_mat3();
        Quat::from(mat3)
    }
}

impl Mul for Quat {
    type Output = Quat;

    fn mul(self, other: Quat) -> Self::Output {
        assert!(self.is_normalized());
        assert!(other.is_normalized());

        // Quaternion multiplication - using scalar approach for clarity
        // result.w = w1*w2 - x1*x2 - y1*y2 - z1*z2
        // result.x = w1*x2 + x1*w2 + y1*z2 - z1*y2
        // result.y = w1*y2 - x1*z2 + y1*w2 + z1*x2
        // result.z = w1*z2 + x1*y2 - y1*x2 + z1*w2

        let x1 = self[0];
        let y1 = self[1];
        let z1 = self[2];
        let w1 = self[3];
        let x2 = other[0];
        let y2 = other[1];
        let z2 = other[2];
        let w2 = other[3];

        Quat::new(
            w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
            w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
            w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
            w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
        )
    }
}

impl Mul<Vec3> for Quat {
    type Output = Vec3;

    fn mul(self, v: Vec3) -> Self::Output {
        let q = Vec3::new(self[0], self[1], self[2]);
        let t = 2.0 * q.cross(v);
        v + (self[3] * t) + q.cross(t)
    }
}
