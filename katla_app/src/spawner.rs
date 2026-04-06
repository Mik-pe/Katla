//! Spawner extension trait for creating entities with mesh+drawable bundles.
//!
//! [`Spawner`] provides a convenient way to spawn entities with the standard
//! `TransformComponent` + `DrawableComponent` bundle without needing access
//! to `&mut Application`. It operates directly on [`katla_ecs::World`].

use katla_ecs::{Component, EntityId, World};
use katla_gfx::{MaterialHandle, MeshHandle};
use katla_math::{Color, Transform, Vec3};

use crate::components::{DrawableComponent, TransformComponent};

/// Extension trait for spawning entities with standard transform+drawable bundles.
///
/// This trait enables entity creation from systems or callbacks that only have
/// access to `&mut World`, without requiring `&mut Application`.
///
/// # Example
///
/// ```ignore
/// use katla_app::spawner::Spawner;
///
/// let entity = world.spawn_primitive(
///     [0.0, 1.0, 0.0],
///     mesh_handle,
///     material_handle,
///     katla_math::Color::WHITE,
/// );
/// ```
pub trait Spawner {
    /// Spawn an entity with transform and drawable components.
    ///
    /// Creates an entity at the given position with the specified mesh and material.
    /// The drawable uses default PBR properties (metallic=0.0, roughness=0.5, ao=1.0).
    fn spawn_primitive(
        &mut self,
        position: [f32; 3],
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
        color: Color,
    ) -> EntityId;

    /// Spawn an entity with transform and drawable components using full PBR material values.
    ///
    /// Creates an entity at the given position with the specified mesh, material,
    /// and explicit PBR material properties.
    #[allow(clippy::too_many_arguments)]
    fn spawn_primitive_with_material(
        &mut self,
        position: [f32; 3],
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
        color: Option<Color>,
        metallic: f32,
        roughness: f32,
        ao: f32,
    ) -> EntityId;

    /// Spawn an entity with transform, drawable, and an additional component.
    ///
    /// Convenience method for spawning a primitive with an extra component
    /// (e.g., `PointLight`, `NameComponent`) in a single call.
    fn spawn_primitive_with<C: Component>(
        &mut self,
        position: [f32; 3],
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
        color: Color,
        extra: C,
    ) -> EntityId;
}

impl Spawner for World {
    fn spawn_primitive(
        &mut self,
        position: [f32; 3],
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
        color: Color,
    ) -> EntityId {
        self.spawn((
            TransformComponent {
                transform: Transform::new_from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, color),
        ))
    }

    fn spawn_primitive_with_material(
        &mut self,
        position: [f32; 3],
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
        color: Option<Color>,
        metallic: f32,
        roughness: f32,
        ao: f32,
    ) -> EntityId {
        self.spawn((
            TransformComponent {
                transform: Transform::new_from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_material(
                mesh_handle,
                material_handle,
                color,
                metallic,
                roughness,
                ao,
            ),
        ))
    }

    fn spawn_primitive_with<C: Component>(
        &mut self,
        position: [f32; 3],
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
        color: Color,
        extra: C,
    ) -> EntityId {
        self.spawn((
            TransformComponent {
                transform: Transform::new_from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, color),
            extra,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_primitive_creates_entity_with_components() {
        let mut world = World::new();

        let mesh = MeshHandle::new(1);
        let material = MaterialHandle::new(2);

        let entity = world.spawn_primitive([1.0, 2.0, 3.0], mesh, material, Color::WHITE);

        let transform = world.get_component::<TransformComponent>(entity);
        assert!(transform.is_some());
        let transform = transform.unwrap();
        assert_eq!(transform.transform.position, Vec3::new(1.0, 2.0, 3.0));

        let drawable = world.get_component::<DrawableComponent>(entity);
        assert!(drawable.is_some());
        let drawable = drawable.unwrap();
        assert_eq!(drawable.mesh_handle, mesh);
        assert_eq!(drawable.material_handle, material);
        assert_eq!(drawable.color, Some(Color::WHITE));
    }

    #[test]
    fn test_spawn_primitive_with_material() {
        let mut world = World::new();

        let mesh = MeshHandle::new(1);
        let material = MaterialHandle::new(2);

        let entity = world.spawn_primitive_with_material(
            [0.0, 0.0, 0.0],
            mesh,
            material,
            Some(Color::RED),
            0.8,
            0.2,
            0.5,
        );

        let drawable = world.get_component::<DrawableComponent>(entity).unwrap();
        assert_eq!(drawable.metallic, 0.8);
        assert_eq!(drawable.roughness, 0.2);
        assert_eq!(drawable.ao, 0.5);
        assert_eq!(drawable.color, Some(Color::RED));
    }

    #[test]
    fn test_spawn_primitive_with_extra_component() {
        use crate::components::scene::NameComponent;

        let mut world = World::new();

        let mesh = MeshHandle::new(1);
        let material = MaterialHandle::new(2);

        let entity = world.spawn_primitive_with(
            [5.0, 0.0, 0.0],
            mesh,
            material,
            Color::WHITE,
            NameComponent::new("test_entity"),
        );

        let transform = world.get_component::<TransformComponent>(entity);
        assert!(transform.is_some());
        assert_eq!(
            transform.unwrap().transform.position,
            Vec3::new(5.0, 0.0, 0.0)
        );

        let name = world.get_component::<NameComponent>(entity);
        assert!(name.is_some());
        assert_eq!(name.unwrap().name, "test_entity");
    }

    #[test]
    fn test_spawn_primitive_default_pbr_values() {
        let mut world = World::new();

        let entity = world.spawn_primitive(
            [0.0, 0.0, 0.0],
            MeshHandle::new(1),
            MaterialHandle::new(2),
            Color::WHITE,
        );

        let drawable = world.get_component::<DrawableComponent>(entity).unwrap();
        assert_eq!(drawable.metallic, 0.0);
        assert_eq!(drawable.roughness, 0.5);
        assert_eq!(drawable.ao, 1.0);
        assert_eq!(drawable.skeleton_handle, katla_gfx::SkeletonHandle::NONE);
    }
}
