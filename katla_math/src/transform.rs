use std::ops::Mul;

use crate::{Mat4, Quat, Vec3, Vec4};

#[derive(Debug, Copy, Clone)]
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

    /// Create a transform from position, rotation, and scale
    pub fn from_position_rotation_scale(position: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            position,
            rotation,
            scale,
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

    pub fn from_position(position: Vec3) -> Self {
        Self::new_from_position(position)
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
        pos_mat.mul(&rot_mat.mul(&scale_mat))
    }

    /// Check if this transform is the identity transform
    pub fn is_identity(&self) -> bool {
        self.position.is_zero()
            && self.rotation.is_normalized()
            && (self.scale[0] - 1.0).abs() < f32::EPSILON
            && (self.scale[1] - 1.0).abs() < f32::EPSILON
            && (self.scale[2] - 1.0).abs() < f32::EPSILON
            && {
                let (x, y, z, w) = self.rotation.xyzw();
                (x - 0.0).abs() < f32::EPSILON
                    && (y - 0.0).abs() < f32::EPSILON
                    && (z - 0.0).abs() < f32::EPSILON
                    && (w - 1.0).abs() < f32::EPSILON
            }
    }

    /// Get the inverse of this transform
    pub fn inverse(&self) -> Self {
        let inv_rot = self.rotation.inverse();
        let inv_scale = Vec3::new(
            if self.scale[0] != 0.0 { 1.0 / self.scale[0] } else { 0.0 },
            if self.scale[1] != 0.0 { 1.0 / self.scale[1] } else { 0.0 },
            if self.scale[2] != 0.0 { 1.0 / self.scale[2] } else { 0.0 },
        );
        let inv_pos = inv_scale * (inv_rot * -self.position);

        Self {
            position: inv_pos,
            rotation: inv_rot,
            scale: inv_scale,
        }
    }

    /// Create a transform that looks at a target position
    /// The transform's position remains unchanged, only rotation is modified
    pub fn look_at(&self, target: Vec3, up: Vec3) -> Self {
        let forward = (target - self.position).normalize();
        Self::look_direction(forward, up)
    }

    /// Create a transform with rotation looking in a direction
    pub fn look_direction(direction: Vec3, up: Vec3) -> Self {
        let forward = direction.normalize();
        let right = forward.cross(up).normalize();
        let up_corrected = right.cross(forward).normalize();

        // Build rotation matrix from basis vectors
        let rot_mat = Mat4([
            Vec4([right[0], up_corrected[0], -forward[0], 0.0]),
            Vec4([right[1], up_corrected[1], -forward[1], 0.0]),
            Vec4([right[2], up_corrected[2], -forward[2], 0.0]),
            Vec4([0.0, 0.0, 0.0, 1.0]),
        ]);

        let rotation = Quat::from(rot_mat.to_mat3());

        Self {
            position: Vec3::new(0.0, 0.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation,
        }
    }

    /// Get the forward direction vector from this transform's rotation
    pub fn forward(&self) -> Vec3 {
        self.rotation * Vec3::new(0.0, 0.0, -1.0)
    }

    /// Get the up direction vector from this transform's rotation
    pub fn up(&self) -> Vec3 {
        self.rotation * Vec3::new(0.0, 1.0, 0.0)
    }

    /// Get the right direction vector from this transform's rotation
    pub fn right(&self) -> Vec3 {
        self.rotation * Vec3::new(1.0, 0.0, 0.0)
    }

    /// Linear interpolation between two transforms
    pub fn lerp(&self, other: &Transform, t: f32) -> Transform {
        Transform {
            position: self.position + ((other.position - self.position) * t),
            rotation: Quat::slerp(self.rotation, other.rotation, t),
            scale: self.scale + ((other.scale - self.scale) * t),
        }
    }

    /// Builder method: set position
    pub fn with_position(mut self, position: Vec3) -> Self {
        self.position = position;
        self
    }

    /// Builder method: set rotation
    pub fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    /// Builder method: set scale
    pub fn with_scale(mut self, scale: Vec3) -> Self {
        self.scale = scale;
        self
    }
}

impl Mul for Transform {
    type Output = Transform;

    /// Compose two transforms: `parent * child`
    ///
    /// When multiplying transforms, the left (parent) transform is applied first,
    /// then the right (child) transform. This matches standard matrix multiplication
    /// order and is used for hierarchical transform composition.
    ///
    /// For `parent * child`:
    /// - Position: child's local position transformed by parent's rotation/scale, then added to parent's position
    /// - Rotation: parent's rotation followed by child's rotation (quaternion multiplication)
    /// - Scale: combined scale (element-wise multiplication)
    fn mul(self, rhs: Self) -> Self::Output {
        // Apply parent's transform to child's position:
        // 1. Rotate child's position by parent's rotation
        // 2. Scale by parent's scale
        // 3. Add parent's position
        let out_pos = self.position + (self.scale * (self.rotation * rhs.position));
        // Parent rotation followed by child rotation
        let out_rot = self.rotation * rhs.rotation;
        // Combined scale
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
