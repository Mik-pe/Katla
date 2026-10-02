//! Rapier-based physics system that syncs ECS components with the Rapier simulation.

use katla_ecs::{EntityId, System, World};
use katla_math::Vec3;
use katla_physics::{
    BodyType, ColliderShape, CollisionFilter, Joint, MeshColliderData, PhysicsActive,
    PhysicsMaterial, PhysicsWorld, RigidBody, TriggerEvent, TriggerVolume,
};
use katla_script::{PendingPhysicsEvents, PhysicsCollisionEvent, PhysicsCollisionEventType};

use crate::components::{Parent, TransformComponent, WorldTransform};

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
        } else {
            crate::events::runtime::reset(world);
            if let Some(physics) = world.get_resource_mut::<PhysicsWorld>() {
                physics.reset_trigger_overlaps();
            }
        }
    }

    fn name(&self) -> &str {
        "RapierPhysicsSystem"
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

    let poses = crate::systems::resolve_world_transforms(world);
    for (entity, shape, body_type) in to_spawn {
        let pose = poses.get(&entity).copied().unwrap_or_default();
        let transform = pose.transform;

        let mat = world.get_component::<PhysicsMaterial>(entity).copied();
        let is_sensor = world.get_component::<TriggerVolume>(entity).is_some();

        let entity_id = entity.id();
        let rb_ref = world.get_component::<RigidBody>(entity);
        let gravity_scale = rb_ref.as_ref().map(|rb| rb.gravity_scale).unwrap_or(1.0);
        let ccd_enabled = rb_ref.as_ref().map(|rb| rb.ccd_enabled).unwrap_or(false);

        let linear_velocity = rb_ref.map(|rb| rb.linear_velocity).unwrap_or_default();
        let collision_filter = world.get_component::<CollisionFilter>(entity).copied();

        let mesh_data = resolve_mesh_data(&shape, world, &pose);

        let (body_handle, collider_handle) = {
            let Some(physics) = world.get_resource_mut::<PhysicsWorld>() else {
                continue;
            };
            let handles = physics.create_body_ex(
                &shape,
                mesh_data.as_ref(),
                &transform,
                body_type,
                mat.as_ref(),
                entity_id,
                is_sensor,
                gravity_scale,
                ccd_enabled,
                collision_filter.as_ref(),
            );
            if body_type != BodyType::Static
                && let Err(error) = physics.set_body_velocity(handles.0, linear_velocity)
            {
                log::error!("Cannot initialize scene body velocity: {error}");
            }
            handles
        };

        if let Some(rb) = world.get_component_mut::<RigidBody>(entity) {
            rb.body_handle = Some(body_handle);
            rb.collider_handle = Some(collider_handle);
        }
    }
}

fn resolve_mesh_data(
    shape: &ColliderShape,
    world: &World,
    pose: &WorldTransform,
) -> Option<MeshColliderData> {
    let handle = match shape {
        ColliderShape::Trimesh(h) | ColliderShape::ConvexHull(h) => *h,
        _ => return None,
    };

    let cache = world.get_resource::<crate::geometry_cache::GeometryCache>()?;
    let data = cache.get(handle)?;
    Some(MeshColliderData {
        // Rapier consumes a rigid pose. Bake the remaining affine deformation
        // into collider vertices, preserving scale, reflection and hierarchy shear.
        positions: data
            .positions
            .iter()
            .map(|p| {
                let point = pose.matrix * katla_math::Vec4::new(p[0], p[1], p[2], 1.0);
                let local = pose.transform.rotation.conjugate_unit()
                    * (Vec3::new(point.x(), point.y(), point.z()) - pose.transform.position);
                local.to_array()
            })
            .collect(),
        triangles: data.triangles.clone(),
    })
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

    let bodies: std::collections::HashMap<_, _> = world
        .query_ref::<&RigidBody>()
        .filter_map(|(entity, body)| body.body_handle.map(|handle| (entity.id(), handle)))
        .collect();
    for (entity, joint) in to_spawn {
        let (Some(&body_a), Some(&body_b)) =
            (bodies.get(&joint.entity_a), bodies.get(&joint.entity_b))
        else {
            continue;
        };
        let result = match world.get_resource_mut::<PhysicsWorld>() {
            Some(physics) => physics.create_joint(&joint, body_a, body_b),
            None => continue,
        };
        match result {
            Ok(handle) => {
                if let Some(component) = world.get_component_mut::<Joint>(entity) {
                    component.joint_handle = Some(handle);
                }
            }
            Err(error) => log::warn!("Cannot create joint owned by entity {entity}: {error}"),
        }
    }
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

    let Some(physics) = world.get_resource_mut::<PhysicsWorld>() else {
        return;
    };
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
    let poses = crate::systems::resolve_world_transforms(world);
    let handles: Vec<_> = world
        .query_ref::<&RigidBody>()
        .filter(|(_, rb)| rb.is_spawned() && rb.body_type != BodyType::Dynamic)
        .filter_map(|(entity, rb)| {
            Some((
                rb.body_type,
                rb.body_handle,
                rb.collider_handle,
                poses.get(&entity)?.transform,
            ))
        })
        .collect();
    let Some(physics) = world.get_resource_mut::<PhysicsWorld>() else {
        return;
    };
    for (kind, body, collider, transform) in handles {
        if kind == BodyType::Static {
            if let Some(collider) = collider {
                physics.set_static_position(collider, &transform);
            }
        } else if let Some(body) = body {
            physics.set_kinematic_position(body, &transform);
        }
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

    let mut poses = crate::systems::resolve_world_transforms(world);
    // A dynamic parent has already moved in Rapier. Use its new pose when
    // converting a child's world result back to local space in this same step.
    for (entity, new_transform, _) in &updates {
        if let Some(pose) = poses.get_mut(entity) {
            let old_rigid = katla_math::Transform::from_position_rotation_scale(
                pose.transform.position,
                pose.transform.rotation,
                Vec3::new(1.0, 1.0, 1.0),
            );
            if let Some(inverse) = old_rigid.make_mat4().inverse() {
                pose.matrix = new_transform.make_mat4() * inverse * pose.matrix;
                pose.transform.position = new_transform.position;
                pose.transform.rotation = new_transform.rotation;
            }
        }
    }
    let updated_poses = updates
        .iter()
        .filter_map(|(id, _, _)| poses.get(id).copied().map(|pose| (*id, pose)))
        .collect();
    let poses = crate::systems::resolve_with_world_poses(world, updated_poses);
    for (entity, new_transform, velocity) in updates {
        let parent = world
            .get_component::<Parent>(entity)
            .and_then(|p| poses.get(&p.parent));
        let local = if let Some(parent) = parent {
            let Some(inverse) = parent.matrix.inverse() else {
                log::warn!("Cannot synchronize physics below a singular parent transform");
                continue;
            };
            let p = inverse
                * katla_math::Vec4::new(
                    new_transform.position.x(),
                    new_transform.position.y(),
                    new_transform.position.z(),
                    1.0,
                );
            (
                Vec3::new(p.x(), p.y(), p.z()),
                parent.transform.rotation.conjugate_unit() * new_transform.rotation,
            )
        } else {
            (new_transform.position, new_transform.rotation)
        };
        if let Some(tc) = world.get_component_mut::<TransformComponent>(entity) {
            tc.transform.position = local.0;
            tc.transform.rotation = local.1;
        }
        if let Some(rb) = world.get_component_mut::<RigidBody>(entity) {
            rb.linear_velocity = velocity;
        }
    }
}

fn process_trigger_events(world: &mut World) {
    if let Some(pending) = world.get_resource_mut::<PendingPhysicsEvents>() {
        pending.0.clear();
    }
    let events: Vec<TriggerEvent> = match world.get_resource_mut::<PhysicsWorld>() {
        Some(physics) => physics.drain_trigger_events(),
        None => return,
    };

    for event in events {
        let (trigger_entity, other_entity, entering) = match event {
            TriggerEvent::Enter {
                trigger_entity,
                other_entity,
            } => (trigger_entity, other_entity, true),
            TriggerEvent::Exit {
                trigger_entity,
                other_entity,
            } => (trigger_entity, other_entity, false),
        };
        if let Some(volume) =
            world.get_component_mut::<TriggerVolume>(EntityId::from_raw(trigger_entity))
        {
            if entering {
                if !volume.overlapping_entities.contains(&other_entity) {
                    volume.overlapping_entities.push(other_entity);
                    volume.overlapping_entities.sort_unstable();
                }
            } else {
                volume.overlapping_entities.retain(|id| *id != other_entity);
            }
        }
        if let Some(pending) = world.get_resource_mut::<PendingPhysicsEvents>() {
            pending.0.push(PhysicsCollisionEvent {
                event_type: if entering {
                    PhysicsCollisionEventType::CollisionEnter
                } else {
                    PhysicsCollisionEventType::CollisionExit
                },
                entity_a: trigger_entity,
                entity_b: other_entity,
            });
        }
        crate::events::runtime::dispatch(world, event);
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
    fn test_scene_velocity_survives_native_body_creation() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        let velocity = Vec3::new(2.0, 3.0, 4.0);
        let mut body = RigidBody::dynamic();
        body.linear_velocity = velocity;
        let entity = world.spawn((
            TransformComponent::default(),
            ColliderShape::Sphere(SphereShape::new(0.5)),
            body,
        ));
        spawn_new_bodies(&mut world);
        let handle = world
            .get_component::<RigidBody>(entity)
            .unwrap()
            .body_handle
            .unwrap();
        assert_eq!(
            world
                .get_resource::<PhysicsWorld>()
                .unwrap()
                .body_velocity(handle)
                .unwrap(),
            velocity
        );
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

        let new_pos = Vec3::new(5.0, 10.0, 3.0);
        {
            let tc = world
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

        {
            let physics = world.get_resource_mut::<PhysicsWorld>().unwrap();
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

        {
            let physics = world.get_resource_mut::<PhysicsWorld>().unwrap();
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

        world.destroy_entity(entity_a);
        system.update(&mut world, 1.0 / 60.0);

        let joint = world.get_component::<Joint>(entity_b).unwrap();
        assert!(
            !joint.is_spawned(),
            "Joint should be cleaned up when referenced entity is destroyed"
        );
    }
}

#[cfg(test)]
mod joint_owner_tests {
    use super::*;

    #[test]
    fn test_joint_owned_by_separate_entity_is_spawned_once() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        let a = world.spawn((
            TransformComponent::default(),
            ColliderShape::Sphere(katla_physics::SphereShape::new(0.5)),
            RigidBody::dynamic(),
        ));
        let b = world.spawn((
            TransformComponent::default(),
            ColliderShape::Sphere(katla_physics::SphereShape::new(0.5)),
            RigidBody::dynamic(),
        ));
        let owner = world.spawn((Joint::fixed(a.id(), b.id(), [0.0; 3], [0.0; 3]),));
        let mut system = RapierPhysicsSystem;
        system.update(&mut world, 1.0 / 60.0);
        let handle = world.get_component::<Joint>(owner).unwrap().joint_handle;
        assert!(handle.is_some());
        assert!(world.get_component::<Joint>(a).is_none());
        assert!(world.get_component::<Joint>(b).is_none());
        system.update(&mut world, 1.0 / 60.0);
        assert_eq!(
            world.get_component::<Joint>(owner).unwrap().joint_handle,
            handle
        );
    }
}

#[cfg(test)]
mod hierarchy_tests {
    use super::*;
    use katla_math::{Quat, Transform};

    #[test]
    fn test_dynamic_world_pose_returns_to_parent_local_without_losing_scale() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsActive(true));
        let parent = world.spawn((TransformComponent::new(Transform {
            position: Vec3::new(4.0, 3.0, 2.0),
            rotation: Quat::from_axis_angle(Vec3::Y_AXIS, 0.6),
            scale: Vec3::new(2.0, 3.0, 4.0),
        }),));
        let local = Transform::new_from_position(Vec3::new(1.0, 2.0, 3.0))
            .with_scale(Vec3::new(0.7, 0.8, 0.9));
        let child = world.spawn((
            TransformComponent::new(local),
            Parent { parent },
            RigidBody::dynamic(),
            ColliderShape::Sphere(katla_physics::SphereShape::new(0.5)),
        ));
        let before = crate::systems::resolve_world_transforms(&world)[&child];
        RapierPhysicsSystem.update(&mut world, 1.0 / 60.0);
        let handle = world
            .get_component::<RigidBody>(child)
            .unwrap()
            .body_handle
            .unwrap();
        let physical = world
            .get_resource::<PhysicsWorld>()
            .unwrap()
            .body_transform(handle)
            .unwrap();
        let after = crate::systems::resolve_world_transforms(&world)[&child];
        assert!((after.transform.position - physical.position).length() < 1e-4);
        assert!(physical.position.y() < before.transform.position.y());
        assert_eq!(
            world
                .get_component::<TransformComponent>(child)
                .unwrap()
                .transform
                .scale,
            local.scale
        );
        RapierPhysicsSystem.update(&mut world, 1.0 / 60.0);
        let physical = world
            .get_resource::<PhysicsWorld>()
            .unwrap()
            .body_transform(handle)
            .unwrap();
        assert!(
            (crate::systems::resolve_world_transforms(&world)[&child]
                .transform
                .position
                - physical.position)
                .length()
                < 1e-4
        );
    }

    #[test]
    fn test_static_colliders_follow_moved_prefab_root() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        let parent = world.spawn((TransformComponent::from_position(Vec3::new(4.0, 0.0, 0.0)),));
        let child = world.spawn((
            TransformComponent::default(),
            Parent { parent },
            RigidBody::static_body(),
            ColliderShape::Sphere(katla_physics::SphereShape::new(0.5)),
        ));
        RapierPhysicsSystem.update(&mut world, 1.0 / 60.0);
        world
            .get_resource_mut::<PhysicsWorld>()
            .unwrap()
            .step(1.0 / 60.0);
        assert_eq!(
            world
                .get_resource::<PhysicsWorld>()
                .unwrap()
                .raycast(Vec3::new(4.0, 2.0, 0.0), -Vec3::Y_AXIS, 3.0)
                .unwrap()
                .entity,
            Some(child.id())
        );
        world
            .get_component_mut::<TransformComponent>(parent)
            .unwrap()
            .transform
            .position = Vec3::new(8.0, 0.0, 0.0);
        RapierPhysicsSystem.update(&mut world, 1.0 / 60.0);
        world
            .get_resource_mut::<PhysicsWorld>()
            .unwrap()
            .step(1.0 / 60.0);
        assert!(
            world
                .get_resource::<PhysicsWorld>()
                .unwrap()
                .raycast(Vec3::new(4.0, 2.0, 0.0), -Vec3::Y_AXIS, 3.0)
                .is_none()
        );
        assert_eq!(
            world
                .get_resource::<PhysicsWorld>()
                .unwrap()
                .raycast(Vec3::new(8.0, 2.0, 0.0), -Vec3::Y_AXIS, 3.0)
                .unwrap()
                .entity,
            Some(child.id())
        );
    }
}

#[cfg(test)]
mod dynamic_ancestor_tests {
    use super::*;

    #[test]
    fn test_dynamic_ancestor_updates_through_nonphysical_intermediate_parent() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsActive(true));
        let root = world.spawn((
            TransformComponent::from_position(Vec3::new(10.0, 2.0, 0.0)),
            RigidBody::dynamic(),
            ColliderShape::Sphere(katla_physics::SphereShape::new(0.2)),
        ));
        world
            .get_component_mut::<RigidBody>(root)
            .unwrap()
            .linear_velocity = Vec3::X_AXIS;
        let middle = world.spawn((
            TransformComponent::from_position(Vec3::new(2.0, 0.0, 0.0)),
            Parent { parent: root },
        ));
        let child = world.spawn((
            TransformComponent::from_position(Vec3::new(2.0, 0.0, 0.0)),
            Parent { parent: middle },
            RigidBody::dynamic(),
            ColliderShape::Sphere(katla_physics::SphereShape::new(0.2)),
        ));
        for _ in 0..3 {
            RapierPhysicsSystem.update(&mut world, 1.0 / 60.0);
            let body = world
                .get_component::<RigidBody>(child)
                .unwrap()
                .body_handle
                .unwrap();
            let physics_pose = world
                .get_resource::<PhysicsWorld>()
                .unwrap()
                .body_transform(body)
                .unwrap();
            let rendered = crate::systems::resolve_world_transforms(&world)[&child]
                .transform
                .position;
            assert!((rendered - physics_pose.position).length() < 1e-4);
        }
        assert!(
            world
                .get_component::<TransformComponent>(child)
                .unwrap()
                .transform
                .position
                .x()
                < 2.0
        );
    }
}
