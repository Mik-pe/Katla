//! Scalar (fallback) implementation of Quat
//!
//! This is used when SSE intrinsics are not available or when the scalar
//! implementation is explicitly preferred.

use crate::{Mat3, Mat4, Vec3, Vec4};
use core::ops::Index;
use std::ops::Mul;

const QUAT_NORMALIZED_THRESHOLD: f32 = 0.001;

/// Quaternion - scalar implementation
#[derive(Debug, Clone, Copy)]
#[repr(C, align(16))]
pub struct Quat {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

impl Index<usize> for Quat {
    type Output = f32;
    fn index(&self, index: usize) -> &f32 {
        match index {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            3 => &self.w,
            _ => panic!("INDEXING OUT_OF_BOUNDS in Quat"),
        }
    }
}

impl Default for Quat {
    fn default() -> Self {
        Self::identity()
    }
}

impl Quat {
    #[inline]
    pub fn identity() -> Quat {
        Quat {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        }
    }

    /// Create a quaternion from XYZW components
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Quat {
        Quat { x, y, z, w }
    }

    /// Get the XYZW components as a tuple
    pub fn xyzw(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.z, self.w)
    }

    #[inline]
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Quat {
        let axis = axis.normalize();
        let factor = f32::sin(angle / 2.0);

        let x = axis.x() * factor;
        let y = axis.y() * factor;
        let z = axis.z() * factor;
        let w = f32::cos(angle / 2.0);

        let mut quat = Quat { x, y, z, w };
        quat.normalize();

        quat
    }

    pub fn from_rotation_between(from: Vec3, to: Vec3) -> Quat {
        let from = from.normalize();
        let to = to.normalize();

        let dot = from.dot(to);
        if dot >= 0.99999 {
            return Self {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            };
        }

        // Vectors are opposite - pick an arbitrary perpendicular axis
        if dot <= -0.99999 {
            let mut axis = Vec3::X_AXIS.cross(from);
            let len_sq = axis.x() * axis.x() + axis.y() * axis.y() + axis.z() * axis.z();

            if len_sq < 0.0001 {
                axis = Vec3::Y_AXIS.cross(from);
            }

            axis = axis.normalize();
            return Self {
                x: axis.x(),
                y: axis.y(),
                z: axis.z(),
                w: 0.0,
            };
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

    fn length_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w
    }

    pub fn is_normalized(&self) -> bool {
        let len_sq = self.length_squared();
        f32::abs(1.0 - len_sq) < QUAT_NORMALIZED_THRESHOLD
    }

    pub fn normalize(&mut self) {
        let len_sq = self.length_squared();
        let len = f32::sqrt(len_sq);
        if len > 0.0 {
            self.x /= len;
            self.y /= len;
            self.z /= len;
            self.w /= len;
        }
    }

    /// Returns the conjugate. Correct inverse only for unit quaternions.
    pub fn conjugate_unit(&self) -> Self {
        self.conjugate()
    }

    pub fn conjugate(&self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
            w: self.w,
        }
    }

    pub fn dot(&self, rhs: Quat) -> f32 {
        let q1_v = Vec3::new(self.x, self.y, self.z);
        let q2_v = Vec3::new(rhs.x, rhs.y, rhs.z);
        let scalar_dot = q1_v.dot(q2_v);
        scalar_dot + self.w * rhs.w
    }

    pub fn rotate_vec3(&self, v: Vec3) -> Vec3 {
        let u = Vec3::new(self.x, self.y, self.z);
        let s = self.w;

        2.0 * u.dot(v) * u + (s * s - u.dot(u)) * v + 2.0 * s * u.cross(v)
    }

    //Spherical interpolation, a slow version
    pub fn slerp(mut a: Quat, mut b: Quat, ratio: f32) -> Self {
        a.normalize();
        b.normalize();

        let mut cs = a.dot(b);
        if cs < 0.0 {
            b = Quat {
                x: -b.x,
                y: -b.y,
                z: -b.z,
                w: -b.w,
            };
            cs = -cs;
        }

        let angle = f32::acos(cs);
        let mut out = Self::identity();
        if f32::abs(angle) >= 0.001 {
            let inv_sin = 1.0f32 / f32::sin(angle);
            let t_angle = ratio * angle;
            let coeff0 = f32::sin(angle - t_angle) * inv_sin;
            let coeff1 = f32::sin(t_angle) * inv_sin;
            out.x = coeff0 * a.x + coeff1 * b.x;
            out.y = coeff0 * a.y + coeff1 * b.y;
            out.z = coeff0 * a.z + coeff1 * b.z;
            out.w = coeff0 * a.w + coeff1 * b.w;
        } else {
            out.x = a.x;
            out.y = a.y;
            out.z = a.z;
            out.w = a.w;
        }

        out.normalize(); // be safe
        out
    }

    #[inline]
    pub fn nlerp(mut a: Quat, mut b: Quat, t: f32) -> Self {
        a.normalize();
        b.normalize();

        if a.dot(b) < 0.0 {
            b = Quat {
                x: -b.x,
                y: -b.y,
                z: -b.z,
                w: -b.w,
            };
        }

        let mut out = Quat {
            x: (1.0 - t) * a.x + t * b.x,
            y: (1.0 - t) * a.y + t * b.y,
            z: (1.0 - t) * a.z + t * b.z,
            w: (1.0 - t) * a.w + t * b.w,
        };
        out.normalize();
        out
    }

    #[inline]
    pub fn make_mat4(&self) -> Mat4 {
        let (m00, m01, m02, m10, m11, m12, m20, m21, m22) = self.rotation_matrix_elements();

        Mat4([
            Vec4::new(m00, m01, m02, 0.0f32),
            Vec4::new(m10, m11, m12, 0.0f32),
            Vec4::new(m20, m21, m22, 0.0f32),
            Vec4::new(0.0f32, 0.0f32, 0.0f32, 1.0f32),
        ])
    }

    pub fn to_mat3(self) -> Mat3 {
        let (m00, m01, m02, m10, m11, m12, m20, m21, m22) = self.rotation_matrix_elements();

        Mat3::from_columns(
            Vec3::new(m00, m01, m02),
            Vec3::new(m10, m11, m12),
            Vec3::new(m20, m21, m22),
        )
    }

    /// Compute the 9 elements of the 3x3 rotation matrix from this quaternion.
    /// Returns (m00, m01, m02, m10, m11, m12, m20, m21, m22).
    fn rotation_matrix_elements(&self) -> (f32, f32, f32, f32, f32, f32, f32, f32, f32) {
        let x2 = self.x + self.x;
        let y2 = self.y + self.y;
        let z2 = self.z + self.z;

        let xx = self.x * x2;
        let xy = self.x * y2;
        let xz = self.x * z2;

        let yy = self.y * y2;
        let yz = self.y * z2;
        let zz = self.z * z2;

        let wx = self.w * x2;
        let wy = self.w * y2;
        let wz = self.w * z2;

        let m00 = 1.0f32 - (yy + zz);
        let m01 = xy + wz;
        let m02 = xz - wy;

        let m10 = xy - wz;
        let m11 = 1.0f32 - (xx + zz);
        let m12 = yz + wx;

        let m20 = xz + wy;
        let m21 = yz - wx;
        let m22 = 1.0f32 - (xx + yy);

        (m00, m01, m02, m10, m11, m12, m20, m21, m22)
    }

    pub fn from_euler(pitch: f32, yaw: f32, roll: f32) -> Quat {
        let pitch_rotation = Quat::from_axis_angle(Vec3::X_AXIS, pitch);
        let yaw_rotation = Quat::from_axis_angle(Vec3::Y_AXIS, yaw);
        let roll_rotation = Quat::from_axis_angle(Vec3::Z_AXIS, roll);

        yaw_rotation * pitch_rotation * roll_rotation
    }

    pub fn to_euler(self) -> (f32, f32, f32) {
        // Roll (x-axis rotation)
        let sinr_cosp = 2.0 * (self.w * self.x + self.y * self.z);
        let cosr_cosp = 1.0 - 2.0 * (self.x * self.x + self.y * self.y);
        let roll = f32::atan2(sinr_cosp, cosr_cosp);

        // Pitch (y-axis rotation)
        let sinp = 2.0 * (self.w * self.y - self.z * self.x);
        let pitch = if f32::abs(sinp) >= 1.0 {
            core::f32::consts::PI / 2.0 * sinp.copysign(1.0)
        } else {
            f32::asin(sinp)
        };

        // Yaw (z-axis rotation)
        let siny_cosp = 2.0 * (self.w * self.z + self.x * self.y);
        let cosy_cosp = 1.0 - 2.0 * (self.y * self.y + self.z * self.z);
        let yaw = f32::atan2(siny_cosp, cosy_cosp);

        (pitch, yaw, roll)
    }
}

impl From<Mat3> for Quat {
    fn from(m: Mat3) -> Self {
        // Convert rotation matrix to quaternion
        let trace = m[0][0] + m[1][1] + m[2][2];

        let mut q = if trace > 0.0 {
            let s = f32::sqrt(trace + 1.0) * 2.0;
            let w = 0.25 * s;
            let x = (m[1][2] - m[2][1]) / s;
            let y = (m[2][0] - m[0][2]) / s;
            let z = (m[0][1] - m[1][0]) / s;
            Quat { x, y, z, w }
        } else if (m[0][0] > m[1][1]) && (m[0][0] > m[2][2]) {
            let s = f32::sqrt(1.0 + m[0][0] - m[1][1] - m[2][2]) * 2.0;
            let w = (m[1][2] - m[2][1]) / s;
            let x = 0.25 * s;
            let y = (m[1][0] + m[0][1]) / s;
            let z = (m[2][0] + m[0][2]) / s;
            Quat { x, y, z, w }
        } else if m[1][1] > m[2][2] {
            let s = f32::sqrt(1.0 + m[1][1] - m[0][0] - m[2][2]) * 2.0;
            let w = (m[2][0] - m[0][2]) / s;
            let x = (m[1][0] + m[0][1]) / s;
            let y = 0.25 * s;
            let z = (m[2][1] + m[1][2]) / s;
            Quat { x, y, z, w }
        } else {
            let s = f32::sqrt(1.0 + m[2][2] - m[0][0] - m[1][1]) * 2.0;
            let w = (m[0][1] - m[1][0]) / s;
            let x = (m[2][0] + m[0][2]) / s;
            let y = (m[2][1] + m[1][2]) / s;
            let z = 0.25 * s;
            Quat { x, y, z, w }
        };

        // Normalize to handle numerical precision issues
        let len = q.length_squared().sqrt();
        if len > 0.0 {
            q.x /= len;
            q.y /= len;
            q.z /= len;
            q.w /= len;
        }

        q
    }
}

impl From<Mat4> for Quat {
    fn from(m: Mat4) -> Self {
        // Extract the 3x3 rotation matrix from the 4x4
        let mat3 = m.to_mat3();
        Quat::from(mat3)
    }
}

impl Mul for Quat {
    type Output = Quat;

    fn mul(self, other: Quat) -> Self::Output {
        assert!(self.is_normalized());
        assert!(other.is_normalized());

        Self {
            x: self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            y: self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            z: self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
            w: self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
        }
    }
}

impl Mul<Vec3> for Quat {
    type Output = Vec3;

    fn mul(self, v: Vec3) -> Self::Output {
        let q = Vec3::new(self.x, self.y, self.z);
        let t = 2.0 * q.cross(v);

        v + (self.w * t) + q.cross(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quat_approx_eq(a: Quat, b: Quat, epsilon: f32) -> bool {
        (a.x - b.x).abs() < epsilon
            && (a.y - b.y).abs() < epsilon
            && (a.z - b.z).abs() < epsilon
            && (a.w - b.w).abs() < epsilon
    }

    #[test]
    fn test_nlerp_identity() {
        let a = Quat::identity();
        let b = Quat::identity();
        let result = Quat::nlerp(a, b, 0.5);
        assert!(quat_approx_eq(result, Quat::identity(), 1e-5));
    }

    #[test]
    fn test_nlerp_t_zero() {
        let a = Quat::from_axis_angle(Vec3::Y_AXIS, 0.0);
        let b = Quat::from_axis_angle(Vec3::Y_AXIS, std::f32::consts::FRAC_PI_2);
        let result = Quat::nlerp(a, b, 0.0);
        assert!(quat_approx_eq(result, a, 1e-5));
    }

    #[test]
    fn test_nlerp_t_one() {
        let a = Quat::from_axis_angle(Vec3::Y_AXIS, 0.0);
        let b = Quat::from_axis_angle(Vec3::Y_AXIS, std::f32::consts::FRAC_PI_2);
        let result = Quat::nlerp(a, b, 1.0);
        assert!(quat_approx_eq(result, b, 1e-5));
    }

    #[test]
    fn test_nlerp_double_cover() {
        let a = Quat::identity();
        let b_neg = Quat::new(-0.0, -0.0, -0.0, -1.0);
        let result = Quat::nlerp(a, b_neg, 0.5);
        assert!(result.is_normalized());
    }

    #[test]
    fn test_nlerp_result_normalized() {
        let a = Quat::from_axis_angle(Vec3::X_AXIS, 0.3);
        let b = Quat::from_axis_angle(Vec3::Y_AXIS, 1.2);
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let result = Quat::nlerp(a, b, t);
            assert!(
                result.is_normalized(),
                "nlerp result not normalized at t={}",
                t
            );
        }
    }
}
