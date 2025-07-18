use std::ops::Mul;

use crate::{Mat4, Quat, Vec3, Vec4};

pub struct Transform {
    pub position: Vec3,
    pub scale: Vec3,
    pub rotation: Quat,
}

impl Default for Transform {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform {
    pub fn new() -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: Quat::new(),
        }
    }

    pub fn new_from_rotation(rotation: Quat) -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation,
        }
    }

    pub fn new_from_position(position: Vec3) -> Self {
        Self {
            position,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: Quat::new(),
        }
    }

    pub fn new_from_scale(scale: Vec3) -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 0.0),
            scale,
            rotation: Quat::new(),
        }
    }

    pub fn make_mat4(&self) -> Mat4 {
        let scale_mat = Mat4([
            Vec4([self.scale[0], 0.0, 0.0, 0.0]),
            Vec4([0.0, self.scale[1], 0.0, 0.0]),
            Vec4([0.0, 0.0, self.scale[2], 0.0]),
            Vec4([0.0, 0.0, 0.0, 1.0]),
        ]);
        let rot_mat = self.rotation.make_mat4();
        let pos_mat = Mat4::from_translation(self.position.0);
        pos_mat.mul(&scale_mat.mul(&rot_mat))
    }
}

impl Mul for Transform {
    type Output = Transform;

    fn mul(self, rhs: Self) -> Self::Output {
        let out_pos = rhs.rotation * (rhs.scale * self.position) + rhs.position;
        let out_rot = rhs.rotation * self.rotation;
        let out_scale = self.scale * rhs.scale;
        Self::Output {
            position: out_pos,
            scale: out_scale,
            rotation: out_rot,
        }
    }
}

impl Mul<Vec3> for Transform {
    type Output = Vec3;

    fn mul(self, v: Vec3) -> Self::Output {
        
        self.position + (self.scale * (self.rotation * v))
    }
}
