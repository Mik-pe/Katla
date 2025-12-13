use crate::{vec3::Vec3, Mat4, Vec4};
use core::ops::Index;
use std::ops::Mul;

const QUAT_NORMALIZED_THRESHOLD: f32 = 0.001;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
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
        Self::new()
    }
}

impl Quat {
    #[inline]
    #[allow(dead_code)]
    pub fn new() -> Quat {
        Quat {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        }
    }

    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Quat {
        let axis = axis.normalize();
        let factor = f32::sin(angle / 2.0);

        let x = axis[0] * factor;
        let y = axis[1] * factor;
        let z = axis[2] * factor;
        let w = f32::cos(angle / 2.0);

        let mut quat = Quat { x, y, z, w };
        quat.normalize();

        quat
    }

    pub fn new_from_yaw_pitch(yaw: f32, pitch: f32) -> Quat {
        let yaw_rotation = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), yaw);
        let pitch_rotation = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), pitch);

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
        self.x /= len_sq;
        self.y /= len_sq;
        self.z /= len_sq;
        self.w /= len_sq;
    }

    pub fn inverse(&self) -> Self {
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

        let cs = a.dot(b);

        let angle = f32::acos(cs);
        let mut out = Self::new();
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

    pub fn make_mat4(&self) -> Mat4 {
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
        let m03 = 0.0f32;

        let m10 = xy - wz;
        let m11 = 1.0f32 - (xx + zz);
        let m12 = yz + wx;
        let m13 = 0.0f32;

        let m20 = xz + wy;
        let m21 = yz - wx;
        let m22 = 1.0f32 - (xx + yy);
        let m23 = 0.0f32;

        let m30 = 0.0f32;
        let m31 = 0.0f32;
        let m32 = 0.0f32;
        let m33 = 1.0f32;
        Mat4([
            Vec4([m00, m01, m02, m03]),
            Vec4([m10, m11, m12, m13]),
            Vec4([m20, m21, m22, m23]),
            Vec4([m30, m31, m32, m33]),
        ])
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
