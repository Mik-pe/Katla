//! Rapier-based physics system that syncs ECS components with the Rapier simulation.

use katla_ecs::{ComponentAccess, EntityId, System, World};
use katla_math::Vec3;
use katla_physics::{
    BodyType, ColliderShape, CollisionFilter, Joint, PhysicsActive, PhysicsMaterial, PhysicsWorld,
    RigidBody, TriggerEvent, TriggerVolume,
};
use katla_script::{PendingPhysicsEvents, PhysicsCollisionEvent, PhysicsCollisionEventType};

use crate::components::TransformComponent;

/// System that synchronizes ECS physics components with the Rapier simulation.
///
/// Each frame:
/// 1. Cleans up Rapier bodies/colliders for destroyed ECS entities
/// 2. Discovers entities with `RigidBody` + `ColliderShape` that haven't been spawned yet
/// 3. Creates corresponding Rapier bodies/colliders in the `PhysicsWorld` resource
/// 4. Discovers `Joint` components and creates Rapier joints
/// 5. Syncs kinematic body transforms from ECS to Rapier
/// 6. Steps the Rapier simulation (only when `PhysicsActive` is true)
/// 7. Reads back transforms and velocities from Rapier to ECS components (dynamic only)
/// 8. Processes trigger volume overlap events
pub struct RapierPhysicsSystem;

impl System for RapierPhysicsSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        cleanup_destroyed_bodies(world);
        cleanup_destroyed_joints(world);
        spawn_new_bodies(world);
        spawn_new_joints(world);

        let active = world
            .get_resource::<PhysicsActive>()
            .map(|p| p.0)
            .unwrap_or(false);

        sync_kinematic_transforms(world);

        if active {
            step_simulation(world, delta_time);
            sync_transforms_back(world);
            process_trigger_events(world);
        }
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
            ComponentAccess::read::<CollisionFilter>(),
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
            ComponentAccess::read::<CollisionFilter>(),
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
        let rb_ref = world.get_component::<RigidBody>(entity);
        let gravity_scale = rb_ref.as_ref().map(|rb| rb.gravity_scale).unwrap_or(1.0);
        let ccd_enabled = rb_ref.as_ref().map(|rb| rb.ccd_enabled).unwrap_or(false);

        let collision_filter = world.get_component::<CollisionFilter>(entity).copied();

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
                gravity_scale,
                ccd_enabled,
                collision_filter.as_ref(),
            );

        if let Some(rb) = world.get_component_mut::<RigidBody>(entity) {
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
                .create_joint(&joint, ha, hb)
                .ok();

            // Find the Joint component for entity_b (joints are typically owned by entity_b)
            let target_entity = EntityId::from_raw(joint.entity_b);
            if let Some(j) = world.get_component_mut::<Joint>(target_entity) {
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
    if let Some(physics) = world.get_resource_mut::<PhysicsWorld>() {
        physics.step(delta_time);
    }
}

fn cleanup_destroyed_bodies(world: &mut World) {
    let active_ids: std::collections::HashSet<u64> = world
        .query::<&RigidBody>()
        .filter(|(_, rb)| rb.is_spawned())
        .map(|(entity, _)| entity.id())
        .collect();

    let orphaned = {
        let physics = match world.get_resource::<PhysicsWorld>() {
            Some(p) => p,
            None => return,
        };
        physics.find_orphaned_colliders(&active_ids)
    };

    if orphaned.is_empty() {
        return;
    }

    let physics = world.get_resource_mut::<PhysicsWorld>().unwrap();
    for (collider_handle, body_handle) in orphaned {
        if let Some(body) = body_handle {
            physics.remove_body(body, collider_handle);
        } else {
            physics.remove_static_collider(collider_handle);
        }
    }
}

fn cleanup_destroyed_joints(world: &mut World) {
    let active_ids: std::collections::HashSet<u64> = world
        .query::<&RigidBody>()
        .filter(|(_, rb)| rb.is_spawned())
        .map(|(entity, _)| entity.id())
        .collect();

    let stale_joints: Vec<_> = world
        .query::<&mut Joint>()
        .filter(|(_, joint)| joint.is_spawned())
        .filter(|(_, joint)| {
            !active_ids.contains(&joint.entity_a) || !active_ids.contains(&joint.entity_b)
        })
        .map(|(entity, _)| entity)
        .collect();

    if stale_joints.is_empty() {
        return;
    }

    for entity in stale_joints {
        if let Some(j) = world.get_component_mut::<Joint>(entity)
            && let Some(handle) = j.joint_handle.take()
            && let Some(physics) = world.get_resource_mut::<PhysicsWorld>()
        {
            physics.remove_joint(handle);
        }
    }
}

fn sync_kinematic_transforms(world: &mut World) {
    let kinematic_handles: Vec<_> = world
        .query::<(&RigidBody, &TransformComponent)>()
        .filter(|(_, rb, _)| rb.is_spawned() && rb.body_type == BodyType::Kinematic)
        .filter_map(|(entity, rb, tc)| {
            let handle = rb.body_handle?;
            Some((entity, handle, tc.transform))
        })
        .collect();

    if kinematic_handles.is_empty() {
        return;
    }

    let physics = match world.get_resource_mut::<PhysicsWorld>() {
        Some(p) => p,
        None => return,
    };

    for (_entity, handle, transform) in kinematic_handles {
        physics.set_kinematic_position(handle, &transform);
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
            let new_transform = physics.body_transform(handle).ok()?;
            let velocity = physics
                .body_velocity(handle)
                .unwrap_or_else(|_| Vec3::default());
            Some((entity, new_transform, velocity))
        })
        .collect();

    let _ = physics;

    for (entity, new_transform, velocity) in updates {
        if let Some(tc) = world.get_component_mut::<TransformComponent>(entity) {
            tc.transform = new_transform;
        }
        if let Some(rb) = world.get_component_mut::<RigidBody>(entity) {
            rb.linear_velocity = velocity;
        }
    }
}

fn process_trigger_events(world: &mut World) {
    let events: Vec<TriggerEvent> = match world.get_resource_mut::<PhysicsWorld>() {
        Some(physics) => physics.drain_collision_events(),
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
        if let Some(tv) = world.get_component_mut::<TriggerVolume>(entity) {
            tv.overlapping_entities = overlapping;
        }
    }

    if !script_events.is_empty()
        && let Some(pending) = world.get_resource_mut::<PendingPhysicsEvents>()
    {
        pending.0.extend(script_events);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_ecs::World;
    use katla_math::{Transform, Vec3};
    use katla_physics::{ColliderShape, Joint, PhysicsActive, SphereShape};

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
        world.insert_resource(PhysicsActive(true));

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

    #[test]
    fn test_play_mode_gating() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsActive(false));

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
        assert_eq!(
            tc.transform.position.y(),
            10.0,
            "Body should not move when physics is inactive"
        );
    }

    #[test]
    fn test_entity_destruction_cleanup() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());

        let entity = world.create_entity();
        world.add_component(entity, TransformComponent::default());
        world.add_component(entity, ColliderShape::Sphere(SphereShape::new(1.0)));
        world.add_component(entity, RigidBody::dynamic());

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);

        let physics = world.get_resource::<PhysicsWorld>().unwrap();
        assert_eq!(physics.collider_count(), 1);

        drop(physics);
        world.destroy_entity(entity);
        system.update(&mut world, 1.0 / 60.0);

        let physics = world.get_resource::<PhysicsWorld>().unwrap();
        assert_eq!(
            physics.collider_count(),
            0,
            "Orphaned collider should be cleaned up"
        );
    }

    #[test]
    fn test_static_body_spawn_tracking() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());

        let entity = world.create_entity();
        world.add_component(entity, TransformComponent::default());
        world.add_component(entity, ColliderShape::Sphere(SphereShape::new(1.0)));
        world.add_component(entity, RigidBody::static_body());

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);

        let rb = world.get_component::<RigidBody>(entity).unwrap();
        assert!(rb.is_spawned(), "Static body should be marked as spawned");
        assert!(
            rb.collider_handle.is_some(),
            "Static body should have a collider handle"
        );
        assert!(
            rb.body_handle.is_some(),
            "Static body should have a body handle slot (even if invalid)"
        );

        let physics = world.get_resource::<PhysicsWorld>().unwrap();
        assert_eq!(physics.collider_count(), 1);
        assert!(
            physics.body_transform(rb.body_handle.unwrap()).is_err(),
            "Static body should have no actual Rapier rigid body"
        );
    }

    #[test]
    fn test_joint_spawning() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());

        let entity_a = world.create_entity();
        world.add_component(
            entity_a,
            TransformComponent::new(Transform::new_from_position(Vec3::new(-2.0, 0.0, 0.0))),
        );
        world.add_component(entity_a, ColliderShape::Sphere(SphereShape::new(0.5)));
        world.add_component(entity_a, RigidBody::dynamic());

        let entity_b = world.create_entity();
        world.add_component(
            entity_b,
            TransformComponent::new(Transform::new_from_position(Vec3::new(2.0, 0.0, 0.0))),
        );
        world.add_component(entity_b, ColliderShape::Sphere(SphereShape::new(0.5)));
        world.add_component(entity_b, RigidBody::dynamic());

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);

        let rb_a = world.get_component::<RigidBody>(entity_a).unwrap();
        let rb_b = world.get_component::<RigidBody>(entity_b).unwrap();
        assert!(rb_a.is_spawned());
        assert!(rb_b.is_spawned());

        world.add_component(
            entity_b,
            Joint::point_to_point(
                entity_a.id(),
                entity_b.id(),
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        );

        system.update(&mut world, 1.0 / 60.0);

        let joint = world.get_component::<Joint>(entity_b).unwrap();
        assert!(
            joint.is_spawned(),
            "Joint should have a handle after system update"
        );
    }

    #[test]
    fn test_kinematic_body_sync() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsActive(true));

        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::new_from_position(Vec3::new(0.0, 0.0, 0.0))),
        );
        world.add_component(entity, ColliderShape::Sphere(SphereShape::new(0.5)));
        world.add_component(entity, RigidBody::kinematic());

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);

        let rb = world.get_component::<RigidBody>(entity).unwrap();
        assert!(rb.is_spawned());
        drop(rb);

        let new_pos = Vec3::new(5.0, 10.0, 3.0);
        {
            let mut tc = world
                .get_component_mut::<TransformComponent>(entity)
                .unwrap();
            tc.transform = Transform::new_from_position(new_pos);
        }

        system.update(&mut world, 1.0 / 60.0);

        let rb = world.get_component::<RigidBody>(entity).unwrap();
        let body_handle = rb.body_handle.unwrap();
        let physics = world.get_resource::<PhysicsWorld>().unwrap();
        let body_transform = physics.body_transform(body_handle).unwrap();
        let pos = body_transform.position;
        assert!((pos.x() - new_pos.x()).abs() < 0.01);
        assert!((pos.y() - new_pos.y()).abs() < 0.01);
        assert!((pos.z() - new_pos.z()).abs() < 0.01);
    }

    #[test]
    fn test_apply_force_through_ecs() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsActive(true));

        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::new_from_position(Vec3::new(0.0, 0.0, 0.0))),
        );
        world.add_component(entity, ColliderShape::Sphere(SphereShape::new(0.5)));
        world.add_component(entity, RigidBody::dynamic());

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);

        let rb = world.get_component::<RigidBody>(entity).unwrap();
        let body_handle = rb.body_handle.unwrap();
        drop(rb);

        {
            let mut physics = world.get_resource_mut::<PhysicsWorld>().unwrap();
            physics.apply_force(body_handle, Vec3::new(0.0, 1000.0, 0.0));
        }

        for _ in 0..10 {
            system.update(&mut world, 1.0 / 60.0);
        }

        let tc = world.get_component::<TransformComponent>(entity).unwrap();
        assert!(
            tc.transform.position.y() > 0.0,
            "Body should have moved upward after upward force"
        );

        let rb = world.get_component::<RigidBody>(entity).unwrap();
        assert!(
            rb.linear_velocity.y() > 0.0,
            "Body should have upward velocity after force"
        );
    }

    #[test]
    fn test_apply_impulse_through_ecs() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsActive(true));

        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::new_from_position(Vec3::new(0.0, 0.0, 0.0))),
        );
        world.add_component(entity, ColliderShape::Sphere(SphereShape::new(0.5)));
        world.add_component(entity, RigidBody::dynamic());

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);

        let rb = world.get_component::<RigidBody>(entity).unwrap();
        let body_handle = rb.body_handle.unwrap();
        drop(rb);

        {
            let mut physics = world.get_resource_mut::<PhysicsWorld>().unwrap();
            physics.apply_impulse(body_handle, Vec3::new(0.0, 10.0, 0.0));
        }

        system.update(&mut world, 1.0 / 60.0);

        let rb = world.get_component::<RigidBody>(entity).unwrap();
        assert!(
            rb.linear_velocity.y() > 0.0,
            "Body should have upward velocity after impulse"
        );
    }

    #[test]
    fn test_stress_many_dynamic_bodies() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsActive(true));

        let count = 100;
        let mut entities = Vec::with_capacity(count);

        for i in 0..count {
            let x = (i as f32 % 10.0) * 2.0;
            let y = (i as f32 / 10.0) * 2.0 + 1.0;
            let entity = world.create_entity();
            world.add_component(
                entity,
                TransformComponent::new(Transform::new_from_position(Vec3::new(x, y, 0.0))),
            );
            world.add_component(entity, ColliderShape::Sphere(SphereShape::new(0.5)));
            world.add_component(entity, RigidBody::dynamic());
            entities.push(entity);
        }

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);

        for entity in &entities {
            let rb = world.get_component::<RigidBody>(*entity).unwrap();
            assert!(rb.is_spawned());
        }

        let physics = world.get_resource::<PhysicsWorld>().unwrap();
        assert_eq!(physics.collider_count(), count);
        drop(physics);

        for _ in 0..60 {
            system.update(&mut world, 1.0 / 60.0);
        }

        let physics = world.get_resource::<PhysicsWorld>().unwrap();
        assert_eq!(physics.collider_count(), count);
    }

    #[test]
    fn test_joint_cleanup_on_entity_destruction() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsActive(true));

        let entity_a = world.create_entity();
        world.add_component(
            entity_a,
            TransformComponent::new(Transform::new_from_position(Vec3::new(-2.0, 0.0, 0.0))),
        );
        world.add_component(entity_a, ColliderShape::Sphere(SphereShape::new(0.5)));
        world.add_component(entity_a, RigidBody::dynamic());

        let entity_b = world.create_entity();
        world.add_component(
            entity_b,
            TransformComponent::new(Transform::new_from_position(Vec3::new(2.0, 0.0, 0.0))),
        );
        world.add_component(entity_b, ColliderShape::Sphere(SphereShape::new(0.5)));
        world.add_component(entity_b, RigidBody::dynamic());

        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);

        world.add_component(
            entity_b,
            Joint::point_to_point(
                entity_a.id(),
                entity_b.id(),
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        );

        system.update(&mut world, 1.0 / 60.0);

        let joint = world.get_component::<Joint>(entity_b).unwrap();
        assert!(joint.is_spawned());
        drop(joint);

        world.destroy_entity(entity_a);
        system.update(&mut world, 1.0 / 60.0);

        let joint = world.get_component::<Joint>(entity_b).unwrap();
        assert!(
            !joint.is_spawned(),
            "Joint should be cleaned up when referenced entity is destroyed"
        );
    }
}
