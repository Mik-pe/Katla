//! Bounding volume components for frustum culling.

use katla_ecs::Component;
use katla_math::AABB;

/// Bounding volume for frustum culling.
///
/// Stores a local-space AABB that is transformed by the entity's
/// world transform to get the world-space bounds.
#[derive(Component, Clone, Debug)]
pub struct BoundingVolume {
    /// Local-space axis-aligned bounding box.
    pub local_aabb: AABB,
}

impl BoundingVolume {
    /// Create a new bounding volume from a local-space AABB.
    pub fn new(local_aabb: AABB) -> Self {
        Self { local_aabb }
    }

    /// Create a bounding volume from min/max points in local space.
    pub fn from_min_max(min: katla_math::Vec3, max: katla_math::Vec3) -> Self {
        let extent = (max - min).mul(0.5);
        let center = min + extent;
        Self {
            local_aabb: AABB { center, extent },
        }
    }

    /// Create a bounding volume from a center and half-extent.
    pub fn from_center_extent(center: katla_math::Vec3, extent: katla_math::Vec3) -> Self {
        Self {
            local_aabb: AABB { center, extent },
        }
    }

    /// Create a spherical bounding volume (approximated as AABB).
    pub fn from_sphere(center: katla_math::Vec3, radius: f32) -> Self {
        Self {
            local_aabb: AABB {
                center,
                extent: katla_math::Vec3::new(radius, radius, radius),
            },
        }
    }

    /// Get the world-space AABB by transforming the local AABB.
    ///
    /// Note: This is an approximation that expands the AABB to ensure
    /// it always contains the rotated local AABB.
    pub fn world_bounds(&self, world_position: katla_math::Vec3) -> AABB {
        // Simple translation for now - could be expanded to handle rotation/scale
        AABB {
            center: self.local_aabb.center + world_position,
            extent: self.local_aabb.extent,
        }
    }
}

/// Marker component indicating an entity is visible this frame.
///
/// This is set by the CullingSystem based on frustum culling.
/// Render systems should query for this to only render visible entities.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Visible;
