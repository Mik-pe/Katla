//! Frustum culling system for visibility determination.

use katla_ecs::{System, World};
use katla_math::{Frustum, Mat4};

use crate::components::{BoundingVolume, TransformComponent, Visible};

/// System that performs frustum culling to determine visible entities.
///
/// This system queries all entities with a BoundingVolume and TransformComponent,
/// then checks if they're inside the view frustum. Entities that are visible
/// get a `Visible` marker component added.
///
/// # Usage
/// ```ignore
/// let mut culling_system = CullingSystem::new();
///
/// // In render loop, update frustum from camera:
/// culling_system.set_frustum_from_camera(&view, &proj);
///
/// // Then run the system
/// world.update(dt); // CullingSystem runs here
/// ```
pub struct CullingSystem {
    frustum: Option<Frustum>,
}

impl CullingSystem {
    /// Create a new culling system with no frustum set.
    pub fn new() -> Self {
        Self { frustum: None }
    }

    /// Set the frustum from view and projection matrices.
    ///
    /// Call this each frame before running the system.
    pub fn set_frustum(&mut self, frustum: Frustum) {
        self.frustum = Some(frustum);
    }

    /// Set the frustum from view and projection matrices.
    pub fn set_frustum_from_matrices(&mut self, view: &Mat4, proj: &Mat4) {
        self.frustum = Some(Frustum::from_projection_view_matrix(proj, view));
    }
}

impl Default for CullingSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for CullingSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        let Some(frustum) = &self.frustum else {
            // No frustum set - skip culling, mark all as visible
            return;
        };

        // Query all entities with transform and bounding volume
        // Note: We need to iterate separately due to borrow rules
        let entities_to_cull: Vec<katla_ecs::EntityId> = world
            .query::<(&TransformComponent, &BoundingVolume)>()
            .map(|(entity, _, _)| entity)
            .collect();

        for entity in entities_to_cull {
            // Get components
            let Some(transform) = world.get_component::<TransformComponent>(entity) else {
                continue;
            };
            let Some(bounds) = world.get_component::<BoundingVolume>(entity) else {
                continue;
            };

            // Get world position from transform
            let world_pos = transform.transform.position;

            // Get world-space bounds
            let world_aabb = bounds.world_bounds(world_pos);

            // Check visibility
            if frustum.intersects_aabb(&world_aabb) {
                // Entity is visible - add Visible marker if not present
                if world.get_component::<Visible>(entity).is_none() {
                    world.add_component(entity, Visible);
                }
            } else {
                // Entity is not visible - remove Visible marker if present
                if world.get_component::<Visible>(entity).is_some() {
                    world.remove_component::<Visible>(entity);
                }
            }
        }
    }

    fn name(&self) -> &str {
        "CullingSystem"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::{Vec3, AABB};

    #[test]
    fn test_culling_system_visible() {
        let mut world = World::new();
        let mut system = CullingSystem::new();

        // Create frustum looking down -Z from (0, 0, 5)
        let view = Mat4::create_lookat(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        let proj = Mat4::create_proj(90.0, 1.0, 0.1, 100.0);
        system.set_frustum_from_matrices(&view, &proj);

        // Create entity at origin with bounding volume
        let entity = world.create_entity();
        let mut transform = TransformComponent::default();
        transform.transform.position = Vec3::new(0.0, 0.0, 0.0);
        world.add_component(entity, transform);
        world.add_component(
            entity,
            BoundingVolume::from_sphere(Vec3::new(0.0, 0.0, 0.0), 1.0),
        );

        // Run culling
        system.update(&mut world, 0.016);

        // Entity should be visible
        assert!(world.get_component::<Visible>(entity).is_some());
    }

    #[test]
    fn test_culling_system_culled() {
        let mut world = World::new();
        let mut system = CullingSystem::new();

        // Create frustum looking down -Z from (0, 0, 5)
        let view = Mat4::create_lookat(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        let proj = Mat4::create_proj(90.0, 1.0, 0.1, 100.0);
        system.set_frustum_from_matrices(&view, &proj);

        // Create entity far outside frustum
        let entity = world.create_entity();
        let mut transform = TransformComponent::default();
        transform.transform.position = Vec3::new(100.0, 100.0, 100.0);
        world.add_component(entity, transform);
        world.add_component(
            entity,
            BoundingVolume::from_sphere(Vec3::new(0.0, 0.0, 0.0), 1.0),
        );

        // Run culling
        system.update(&mut world, 0.016);

        // Entity should NOT be visible
        assert!(world.get_component::<Visible>(entity).is_none());
    }
}
