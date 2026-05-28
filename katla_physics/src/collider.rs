//! ECS components for collision detection.

use katla_ecs::Component;
use katla_math::{AABB, Transform};
use serde::{Deserialize, Serialize};

use crate::shape::SphereShape;

/// A collision shape attached to an entity.
///
/// Defines the local-space geometry used for collision detection.
/// The shape is transformed to world space using the entity's `TransformComponent`.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub enum ColliderShape {
    Sphere(SphereShape),
    Box(crate::shape::BoxShape),
    Capsule(crate::shape::CapsuleShape),
}

impl ColliderShape {
    /// Compute the world-space AABB for this shape transformed by the given transform.
    pub fn world_aabb(&self, transform: &Transform) -> AABB {
        self.local_aabb().transform(&transform.make_mat4())
    }

    /// Compute the local-space AABB for this shape.
    ///
    /// The AABB is centered at the origin with extents matching the shape bounds.
    pub fn local_aabb(&self) -> AABB {
        match self {
            ColliderShape::Sphere(s) => AABB::from_min_max(
                katla_math::Vec3::new(-s.radius, -s.radius, -s.radius),
                katla_math::Vec3::new(s.radius, s.radius, s.radius),
            ),
            ColliderShape::Box(b) => {
                let he = b.half_extents_vec();
                AABB::from_min_max(-he, he)
            }
            ColliderShape::Capsule(c) => AABB::from_min_max(
                katla_math::Vec3::new(-c.radius, -c.half_height - c.radius, -c.radius),
                katla_math::Vec3::new(c.radius, c.half_height + c.radius, c.radius),
            ),
        }
    }
}

/// Collision layer flags for filtering collision pairs.
///
/// Entities on matching layers will be tested against each other during
/// broadphase. Layers are combined with bitwise AND: two entities collide
/// only if `(a.layers & b.mask) != 0 && (b.layers & a.mask) != 0`.
#[derive(Component, Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CollisionFilter {
    /// Which collision layers this entity belongs to.
    pub layers: u32,
    /// Which collision layers this entity collides with.
    pub mask: u32,
}

impl CollisionFilter {
    pub fn new(layers: u32, mask: u32) -> Self {
        Self { layers, mask }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::{BoxShape, CapsuleShape};

    #[test]
    fn test_sphere_local_aabb() {
        let shape = ColliderShape::Sphere(SphereShape::new(2.5));
        let aabb = shape.local_aabb();
        assert_eq!(aabb.min(), katla_math::Vec3::new(-2.5, -2.5, -2.5));
        assert_eq!(aabb.max(), katla_math::Vec3::new(2.5, 2.5, 2.5));
    }

    #[test]
    fn test_box_local_aabb() {
        let shape = ColliderShape::Box(BoxShape::from_extents(2.0, 4.0, 6.0));
        let aabb = shape.local_aabb();
        assert_eq!(aabb.min(), katla_math::Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(aabb.max(), katla_math::Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn test_capsule_local_aabb() {
        let shape = ColliderShape::Capsule(CapsuleShape::new(1.0, 0.5));
        let aabb = shape.local_aabb();
        assert_eq!(aabb.min(), katla_math::Vec3::new(-0.5, -1.5, -0.5));
        assert_eq!(aabb.max(), katla_math::Vec3::new(0.5, 1.5, 0.5));
    }

    #[test]
    fn test_sphere_world_aabb_identity() {
        let shape = ColliderShape::Sphere(SphereShape::new(2.0));
        let transform = katla_math::Transform::default();
        let aabb = shape.world_aabb(&transform);
        assert_eq!(aabb.min(), katla_math::Vec3::new(-2.0, -2.0, -2.0));
        assert_eq!(aabb.max(), katla_math::Vec3::new(2.0, 2.0, 2.0));
    }

    #[test]
    fn test_sphere_world_aabb_translated() {
        let shape = ColliderShape::Sphere(SphereShape::new(1.0));
        let transform =
            katla_math::Transform::new_from_position(katla_math::Vec3::new(5.0, 10.0, 15.0));
        let aabb = shape.world_aabb(&transform);
        assert_eq!(aabb.min(), katla_math::Vec3::new(4.0, 9.0, 14.0));
        assert_eq!(aabb.max(), katla_math::Vec3::new(6.0, 11.0, 16.0));
    }

    #[test]
    fn test_box_world_aabb_scaled() {
        let shape = ColliderShape::Box(BoxShape::from_extents(2.0, 2.0, 2.0));
        let transform = katla_math::Transform::new_from_scale(katla_math::Vec3::new(2.0, 3.0, 4.0));
        let aabb = shape.world_aabb(&transform);
        assert_eq!(aabb.min(), katla_math::Vec3::new(-2.0, -3.0, -4.0));
        assert_eq!(aabb.max(), katla_math::Vec3::new(2.0, 3.0, 4.0));
    }
}
