//! ECS components for collision detection.

use katla_ecs::Component;
use katla_gfx::MeshHandle;
use katla_math::{AABB, Transform};
use serde::{Deserialize, Serialize};

use crate::shape::SphereShape;

/// A collision shape attached to an entity.
///
/// Defines the local-space geometry used for collision detection.
/// The shape is transformed to world space using the entity's `TransformComponent`.
#[derive(Component, Debug, Clone)]
pub enum ColliderShape {
    Sphere(SphereShape),
    Box(crate::shape::BoxShape),
    Capsule(crate::shape::CapsuleShape),
    Trimesh(MeshHandle),
    ConvexHull(MeshHandle),
    Heightfield(crate::shape::HeightfieldShape),
}

impl Serialize for ColliderShape {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(tag = "type")]
        enum ShapeRepr<'a> {
            Sphere {
                radius: f32,
            },
            Box {
                half_extents: [f32; 3],
            },
            Capsule {
                half_height: f32,
                radius: f32,
            },
            Trimesh {
                mesh_handle_index: u32,
            },
            #[serde(rename = "ConvexHull")]
            ConvexHull {
                mesh_handle_index: u32,
            },
            Heightfield {
                rows: u32,
                cols: u32,
                heights: &'a [f32],
            },
        }

        match self {
            ColliderShape::Sphere(s) => ShapeRepr::Sphere { radius: s.radius },
            ColliderShape::Box(b) => ShapeRepr::Box {
                half_extents: b.half_extents,
            },
            ColliderShape::Capsule(c) => ShapeRepr::Capsule {
                half_height: c.half_height,
                radius: c.radius,
            },
            ColliderShape::Trimesh(h) => ShapeRepr::Trimesh {
                mesh_handle_index: h.index(),
            },
            ColliderShape::ConvexHull(h) => ShapeRepr::ConvexHull {
                mesh_handle_index: h.index(),
            },
            ColliderShape::Heightfield(h) => ShapeRepr::Heightfield {
                rows: h.rows,
                cols: h.cols,
                heights: &h.heights,
            },
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ColliderShape {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "type")]
        enum ShapeRepr {
            Sphere {
                radius: f32,
            },
            Box {
                half_extents: [f32; 3],
            },
            Capsule {
                half_height: f32,
                radius: f32,
            },
            Trimesh {
                mesh_handle_index: u32,
            },
            #[serde(rename = "ConvexHull")]
            ConvexHull {
                mesh_handle_index: u32,
            },
            Heightfield {
                rows: u32,
                cols: u32,
                heights: Vec<f32>,
            },
        }

        match ShapeRepr::deserialize(deserializer)? {
            ShapeRepr::Sphere { radius } => Ok(ColliderShape::Sphere(SphereShape::new(radius))),
            ShapeRepr::Box { half_extents } => {
                Ok(ColliderShape::Box(crate::shape::BoxShape { half_extents }))
            }
            ShapeRepr::Capsule {
                half_height,
                radius,
            } => Ok(ColliderShape::Capsule(crate::shape::CapsuleShape::new(
                half_height,
                radius,
            ))),
            ShapeRepr::Trimesh { mesh_handle_index } => {
                Ok(ColliderShape::Trimesh(MeshHandle::new(mesh_handle_index)))
            }
            ShapeRepr::ConvexHull { mesh_handle_index } => Ok(ColliderShape::ConvexHull(
                MeshHandle::new(mesh_handle_index),
            )),
            ShapeRepr::Heightfield {
                rows,
                cols,
                heights,
            } => Ok(ColliderShape::Heightfield(
                crate::shape::HeightfieldShape::new(rows, cols, heights),
            )),
        }
    }
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
            ColliderShape::Trimesh(_) | ColliderShape::ConvexHull(_) => AABB::from_min_max(
                katla_math::Vec3::new(0.0, 0.0, 0.0),
                katla_math::Vec3::new(0.0, 0.0, 0.0),
            ),
            ColliderShape::Heightfield(h) => {
                let min_h = h.heights.iter().copied().fold(f32::INFINITY, f32::min);
                let max_h = h.heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                let half_rows = h.rows as f32 * 0.5;
                let half_cols = h.cols as f32 * 0.5;
                AABB::from_min_max(
                    katla_math::Vec3::new(-half_cols, min_h, -half_rows),
                    katla_math::Vec3::new(half_cols, max_h, half_rows),
                )
            }
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
