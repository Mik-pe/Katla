//! Rapier-based physics system that syncs ECS components with the Rapier simulation.

use katla_ecs::{ComponentAccess, System, World};
use katla_math::Vec3;
use katla_physics::{BodyType, ColliderShape, PhysicsMaterial, PhysicsWorld, RigidBody};

use crate::components::TransformComponent;

/// System that synchronizes ECS physics components with the Rapier simulation.
///
/// Each frame:
/// 1. Discovers entities with `RigidBody` + `ColliderShape` that haven't been spawned yet
/// 2. Creates corresponding Rapier bodies/colliders in the `PhysicsWorld` resource
/// 3. Steps the Rapier simulation
/// 4. Reads back transforms and velocities from Rapier to ECS components
pub struct RapierPhysicsSystem;

impl System for RapierPhysicsSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        spawn_new_bodies(world);
        step_simulation(world, delta_time);
        sync_transforms_back(world);
    }

    fn name(&self) -> &str {
        "RapierPhysicsSystem"
    }

    fn component_access() -> Vec<ComponentAccess>
    where
        Self: Sized,
    {
        vec![
            ComponentAccess::write::<RigidBody>(),
            ComponentAccess::read::<ColliderShape>(),
            ComponentAccess::read::<PhysicsMaterial>(),
            ComponentAccess::write::<TransformComponent>(),
        ]
    }

    fn component_access_dyn(&self) -> Vec<ComponentAccess> {
        vec![
            ComponentAccess::write::<RigidBody>(),
            ComponentAccess::read::<ColliderShape>(),
            ComponentAccess::read::<PhysicsMaterial>(),
            ComponentAccess::write::<TransformComponent>(),
        ]
    }
}

fn spawn_new_bodies(world: &mut World) {
    if world.get_resource::<PhysicsWorld>().is_none() {
        return;
    }

    let to_spawn: Vec<_> = world
        .query::<(&ColliderShape, &mut RigidBody)>()
        .filter(|(_, _, rb)| !rb.is_spawned())
        .map(|(entity, shape, rb)| (entity, shape.clone(), rb.body_type))
        .collect();

    if to_spawn.is_empty() {
        return;
    }

    for (entity, shape, body_type) in to_spawn {
        let transform = world
            .get_component::<TransformComponent>(entity)
            .map(|t| t.transform)
            .unwrap_or_default();

        let mat = world.get_component::<PhysicsMaterial>(entity).copied();

        let entity_id = entity.id();
        let (body_handle, collider_handle) = world
            .get_resource_mut::<PhysicsWorld>()
            .unwrap()
            .create_body(&shape, &transform, body_type, mat.as_ref(), entity_id);

        if let Some(mut rb) = world.get_component_mut::<RigidBody>(entity) {
            rb.body_handle = Some(body_handle);
            rb.collider_handle = Some(collider_handle);
        }
    }
}

fn step_simulation(world: &mut World, delta_time: f32) {
    if let Some(mut physics) = world.get_resource_mut::<PhysicsWorld>() {
        physics.step(delta_time);
    }
}

fn sync_transforms_back(world: &mut World) {
    let dynamic_handles: Vec<_> = world
        .query::<&RigidBody>()
        .filter(|(_, rb)| rb.is_spawned() && rb.body_type == BodyType::Dynamic)
        .filter_map(|(entity, rb)| {
            let handle = rb.body_handle?;
            Some((entity, handle))
        })
        .collect();

    let physics = match world.get_resource::<PhysicsWorld>() {
        Some(p) => p,
        None => return,
    };

    let updates: Vec<_> = dynamic_handles
        .into_iter()
        .filter_map(|(entity, handle)| {
            let new_transform = physics.body_transform(handle)?;
            let velocity = physics.body_velocity(handle).unwrap_or_else(Vec3::default);
            Some((entity, new_transform, velocity))
        })
        .collect();

    let _ = physics;

    for (entity, new_transform, velocity) in updates {
        if let Some(mut tc) = world.get_component_mut::<TransformComponent>(entity) {
            tc.transform = new_transform;
        }
        if let Some(mut rb) = world.get_component_mut::<RigidBody>(entity) {
            rb.linear_velocity = velocity;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_ecs::World;
    use katla_math::{Transform, Vec3};
    use katla_physics::{ColliderShape, SphereShape};

    #[test]
    fn test_spawn_dynamic_body() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());

        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::new_from_position(Vec3::new(0.0, 10.0, 0.0))),
        );
        world.add_component(entity, ColliderShape::Sphere(SphereShape::new(0.5)));
        world.add_component(entity, RigidBody::dynamic());

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);

        let rb = world.get_component::<RigidBody>(entity).unwrap();
        assert!(rb.is_spawned());
    }

    #[test]
    fn test_spawn_static_body() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());

        let entity = world.create_entity();
        world.add_component(entity, TransformComponent::new(Transform::default()));
        world.add_component(entity, ColliderShape::Sphere(SphereShape::new(1.0)));
        world.add_component(entity, RigidBody::static_body());

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);

        let rb = world.get_component::<RigidBody>(entity).unwrap();
        assert!(rb.is_spawned());
    }

    #[test]
    fn test_gravity_affects_dynamic() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());

        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::new_from_position(Vec3::new(0.0, 10.0, 0.0))),
        );
        world.add_component(entity, ColliderShape::Sphere(SphereShape::new(0.5)));
        world.add_component(entity, RigidBody::dynamic());

        let mut system = RapierPhysicsSystem;
        for _ in 0..60 {
            system.update(&mut world, 1.0 / 60.0);
        }

        let tc = world.get_component::<TransformComponent>(entity).unwrap();
        assert!(
            tc.transform.position.y() < 10.0,
            "Dynamic body should have fallen"
        );
    }

    #[test]
    fn test_no_physics_world_no_crash() {
        let mut world = World::new();
        let entity = world.create_entity();
        world.add_component(entity, ColliderShape::Sphere(SphereShape::new(1.0)));
        world.add_component(entity, RigidBody::dynamic());

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);
    }
}
