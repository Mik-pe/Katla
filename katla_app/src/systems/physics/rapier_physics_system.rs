//! Rapier-based physics system that syncs ECS components with the Rapier simulation.

use katla_ecs::{ComponentAccess, EntityId, System, World};
use katla_math::Vec3;
use katla_physics::{
    BodyType, ColliderShape, Joint, PhysicsMaterial, PhysicsWorld, RigidBody, TriggerEvent,
    TriggerVolume,
};
use katla_script::{PendingPhysicsEvents, PhysicsCollisionEvent, PhysicsCollisionEventType};

use crate::components::TransformComponent;

/// System that synchronizes ECS physics components with the Rapier simulation.
///
/// Each frame:
/// 1. Discovers entities with `RigidBody` + `ColliderShape` that haven't been spawned yet
/// 2. Creates corresponding Rapier bodies/colliders in the `PhysicsWorld` resource
/// 3. Discovers `Joint` components and creates Rapier joints
/// 4. Steps the Rapier simulation
/// 5. Reads back transforms and velocities from Rapier to ECS components
/// 6. Processes trigger volume overlap events
pub struct RapierPhysicsSystem;

impl System for RapierPhysicsSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        spawn_new_bodies(world);
        spawn_new_joints(world);
        step_simulation(world, delta_time);
        sync_transforms_back(world);
        process_trigger_events(world);
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
            ComponentAccess::write::<Joint>(),
            ComponentAccess::read::<TriggerVolume>(),
        ]
    }

    fn component_access_dyn(&self) -> Vec<ComponentAccess> {
        vec![
            ComponentAccess::write::<RigidBody>(),
            ComponentAccess::read::<ColliderShape>(),
            ComponentAccess::read::<PhysicsMaterial>(),
            ComponentAccess::write::<TransformComponent>(),
            ComponentAccess::write::<Joint>(),
            ComponentAccess::read::<TriggerVolume>(),
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
        let is_sensor = world.get_component::<TriggerVolume>(entity).is_some();

        let entity_id = entity.id();
        let (body_handle, collider_handle) = world
            .get_resource_mut::<PhysicsWorld>()
            .unwrap()
            .create_body_ex(
                &shape,
                &transform,
                body_type,
                mat.as_ref(),
                entity_id,
                is_sensor,
            );

        if let Some(mut rb) = world.get_component_mut::<RigidBody>(entity) {
            rb.body_handle = Some(body_handle);
            rb.collider_handle = Some(collider_handle);
        }
    }
}

fn spawn_new_joints(world: &mut World) {
    if world.get_resource::<PhysicsWorld>().is_none() {
        return;
    }

    let to_spawn: Vec<_> = world
        .query::<&mut Joint>()
        .filter(|(_, joint)| !joint.is_spawned())
        .map(|(entity, joint)| (entity, joint.clone()))
        .collect();

    if to_spawn.is_empty() {
        return;
    }

    for (_entity, joint) in to_spawn {
        let body_a = find_rigid_body_handle(world, joint.entity_a);
        let body_b = find_rigid_body_handle(world, joint.entity_b);

        if let (Some(ha), Some(hb)) = (body_a, body_b) {
            let joint_handle = world
                .get_resource_mut::<PhysicsWorld>()
                .unwrap()
                .create_joint(&joint, ha, hb);

            // Find the Joint component for entity_b (joints are typically owned by entity_b)
            let target_entity = EntityId::from_raw(joint.entity_b);
            if let Some(mut j) = world.get_component_mut::<Joint>(target_entity) {
                j.joint_handle = joint_handle;
            }
        }
    }
}

fn find_rigid_body_handle(world: &World, entity_id: u64) -> Option<katla_physics::RigidBodyHandle> {
    let entity = EntityId::from_raw(entity_id);
    world
        .get_component::<RigidBody>(entity)
        .and_then(|rb| rb.body_handle)
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

fn process_trigger_events(world: &mut World) {
    let events: Vec<TriggerEvent> = match world.get_resource_mut::<PhysicsWorld>() {
        Some(mut physics) => physics.drain_collision_events(),
        None => return,
    };

    if events.is_empty() {
        return;
    }

    let mut trigger_overlaps: std::collections::HashMap<u64, Vec<u64>> =
        std::collections::HashMap::new();

    let mut script_events = Vec::new();

    for event in events {
        match event {
            TriggerEvent::Enter {
                trigger_entity,
                other_entity,
            } => {
                trigger_overlaps
                    .entry(trigger_entity)
                    .or_default()
                    .push(other_entity);
                script_events.push(PhysicsCollisionEvent {
                    event_type: PhysicsCollisionEventType::CollisionEnter,
                    entity_a: trigger_entity,
                    entity_b: other_entity,
                });
            }
            TriggerEvent::Exit {
                trigger_entity,
                other_entity,
            } => {
                script_events.push(PhysicsCollisionEvent {
                    event_type: PhysicsCollisionEventType::CollisionExit,
                    entity_a: trigger_entity,
                    entity_b: other_entity,
                });
            }
        }
    }

    for (trigger_id, overlapping) in trigger_overlaps {
        let entity = EntityId::from_raw(trigger_id);
        if let Some(mut tv) = world.get_component_mut::<TriggerVolume>(entity) {
            tv.overlapping_entities = overlapping;
        }
    }

    if !script_events.is_empty() {
        if let Some(mut pending) = world.get_resource_mut::<PendingPhysicsEvents>() {
            pending.0.extend(script_events);
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
