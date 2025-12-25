//! Physics system for the ECS framework.
//!
//! This system handles physics simulation including:
//! - Force application (F = ma)
//! - Drag forces (opposing velocity)
//! - Velocity integration (v = v + a*dt)
//!
//! Position integration is handled by the velocity_system.
//!
//! # Example Usage
//!
//! ```ignore
//! use katla_ecs::{World, SystemExecutionOrder};
//! use katla_math::Vec3;
//! use physics_system::PhysicsSystem;
//! use components::{TransformComponent, VelocityComponent, ForceComponent, MassComponent};
//!
//! let mut world = World::new();
//! world.register_system(Box::new(PhysicsSystem::new()), SystemExecutionOrder::NORMAL);
//!
//! let entity = world.create_entity();
//! world.add_component(entity, TransformComponent::default());
//! world.add_component(entity, VelocityComponent::default());
//! world.add_component(entity, MassComponent { mass: 1.0 });
//! world.add_component(entity, ForceComponent { force: Vec3::new(0.0, -9.81, 0.0) });
//!
//! world.update(0.016); // Apply gravity and update velocity
//! ```

use katla_ecs::{ComponentStorageManager, System};

use crate::components::{DragComponent, ForceComponent, MassComponent, VelocityComponent};

pub struct PhysicsSystem;

impl PhysicsSystem {
    pub fn new() -> Self {
        PhysicsSystem
    }
}

impl System for PhysicsSystem {
    fn update(&mut self, storage: &mut ComponentStorageManager, delta_time: f32) {
        // Step 1: Apply drag forces (velocity-dependent forces)
        for (_entity, velocity, drag, force) in
            storage.query::<(&VelocityComponent, &DragComponent, &mut ForceComponent)>()
        {
            // Drag force opposes velocity: F_drag = -coefficient * |v|^2 * v_hat
            let speed_squared = velocity.velocity.distance_squared();
            if speed_squared > 0.0 {
                let speed = speed_squared.sqrt();
                let velocity_direction = velocity.velocity * (1.0 / speed);
                let drag_force = velocity_direction * (-drag.coefficient * speed_squared);
                force.force += drag_force;
            }
        }

        // Step 2: Apply forces to update accelerations (F = ma -> a = F/m)
        // Collect entities with their mass values to avoid borrow issues
        let entity_masses: Vec<_> = storage
            .query::<&MassComponent>()
            .map(|(entity, mass)| (entity, mass.mass))
            .collect();

        // Now update accelerations for all entities with forces
        for (entity, velocity, force) in
            storage.query::<(&mut VelocityComponent, &ForceComponent)>()
        {
            // Find mass for this entity, default to 1.0
            let mass = entity_masses
                .iter()
                .find(|(e, _)| *e == entity)
                .map(|(_, m)| *m)
                .unwrap_or(1.0);

            // Prevent division by zero
            if mass > 0.0 {
                velocity.acceleration = force.force * (1.0 / mass);
            }
        }

        // Step 3: Integrate velocity (v = v + a * dt)
        for (_entity, velocity) in storage.query::<&mut VelocityComponent>() {
            velocity.velocity += velocity.acceleration * delta_time;
        }

        // Step 4: Reset forces for next frame
        for (_entity, force) in storage.query::<&mut ForceComponent>() {
            force.force = katla_math::Vec3::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{TransformComponent, VelocityComponent};
    use crate::systems::VelocitySystem;
    use katla_ecs::{SystemExecutionOrder, World};
    use katla_math::{Transform, Vec3};

    fn create_test_world() -> World {
        let mut world = World::new();
        world.register_system(Box::new(PhysicsSystem::new()), SystemExecutionOrder::NORMAL);
        world.register_system(Box::new(VelocitySystem), SystemExecutionOrder::LATE);
        world
    }

    #[test]
    fn test_basic_velocity_integration() {
        let mut world = create_test_world();

        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::from_position(Vec3::new(0.0, 0.0, 0.0))),
        );
        world.add_component(
            entity,
            VelocityComponent::new(Vec3::new(10.0, 0.0, 0.0), Vec3::default()),
        );

        // Run for 1 second with dt=0.1
        for _ in 0..10 {
            world.update(0.1);
        }

        let transform = world.get_component::<TransformComponent>(entity).unwrap();
        // Should move 10 units in x direction
        assert!((transform.transform.position.x() - 10.0).abs() < 0.001);
        assert!((transform.transform.position.y()).abs() < 0.001);
    }

    #[test]
    fn test_force_acceleration_integration() {
        let mut world = create_test_world();

        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::from_position(Vec3::new(0.0, 0.0, 0.0))),
        );
        world.add_component(
            entity,
            VelocityComponent::new(Vec3::default(), Vec3::default()),
        );
        world.add_component(entity, MassComponent { mass: 2.0 });
        world.add_component(
            entity,
            ForceComponent {
                force: Vec3::new(20.0, 0.0, 0.0),
            },
        );

        // Apply force for 0.1 seconds
        world.update(0.1);

        let velocity = world.get_component::<VelocityComponent>(entity).unwrap();
        // a = F/m = 20/2 = 10 m/s^2
        // v = a * t = 10 * 0.1 = 1.0 m/s
        assert!((velocity.velocity.x() - 1.0).abs() < 0.001);

        // Force should be reset after update
        let force = world.get_component::<ForceComponent>(entity).unwrap();
        assert_eq!(force.force.x(), 0.0);
    }

    #[test]
    fn test_gravity_simulation() {
        let mut world = create_test_world();

        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::from_position(Vec3::new(0.0, 100.0, 0.0))),
        );
        world.add_component(
            entity,
            VelocityComponent::new(Vec3::default(), Vec3::default()),
        );
        world.add_component(entity, MassComponent { mass: 1.0 });

        // Simulate gravity for 1 second
        let gravity = Vec3::new(0.0, -9.81, 0.0);
        for _ in 0..10 {
            world.add_component(entity, ForceComponent { force: gravity });
            world.update(0.1);
        }

        let transform = world.get_component::<TransformComponent>(entity).unwrap();
        let velocity = world.get_component::<VelocityComponent>(entity).unwrap();

        // After 1 second with gravity: v = g*t = -9.81 m/s
        assert!((velocity.velocity.y() + 9.81).abs() < 0.01);

        // Position with semi-implicit Euler: should have fallen from 100m
        assert!(transform.transform.position.y() < 100.0);
        assert!(transform.transform.position.y() > 94.0);
    }

    #[test]
    fn test_drag_force() {
        let mut world = create_test_world();

        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::from_position(Vec3::new(0.0, 0.0, 0.0))),
        );
        world.add_component(
            entity,
            VelocityComponent::new(Vec3::new(10.0, 0.0, 0.0), Vec3::default()),
        );
        world.add_component(entity, MassComponent { mass: 1.0 });
        world.add_component(entity, DragComponent::new(0.1));
        world.add_component(
            entity,
            ForceComponent {
                force: Vec3::default(),
            },
        );

        let initial_velocity = world
            .get_component::<VelocityComponent>(entity)
            .unwrap()
            .velocity
            .x();

        // Run simulation - drag should slow down the entity
        for _ in 0..10 {
            world.update(0.1);
        }

        let final_velocity = world
            .get_component::<VelocityComponent>(entity)
            .unwrap()
            .velocity
            .x();

        // Velocity should have decreased due to drag
        assert!(final_velocity < initial_velocity);
        assert!(final_velocity > 0.0); // But not reversed
    }

    #[test]
    fn test_projectile_motion() {
        let mut world = create_test_world();

        let entity = world.create_entity();

        // Launch projectile at 45 degrees with speed 20 m/s
        let angle = std::f32::consts::PI / 4.0;
        let speed = 20.0;
        let vx = speed * angle.cos();
        let vy = speed * angle.sin();

        world.add_component(
            entity,
            TransformComponent::new(Transform::from_position(Vec3::new(0.0, 0.0, 0.0))),
        );
        world.add_component(
            entity,
            VelocityComponent::new(Vec3::new(vx, vy, 0.0), Vec3::default()),
        );
        world.add_component(entity, MassComponent { mass: 1.0 });

        // Simulate with gravity
        let gravity = Vec3::new(0.0, -9.81, 0.0);
        let dt = 0.01;
        let mut time = 0.0;
        let max_time = 3.0;

        while time < max_time {
            world.add_component(entity, ForceComponent { force: gravity });
            world.update(dt);
            time += dt;

            let transform = world.get_component::<TransformComponent>(entity).unwrap();
            if transform.transform.position.y() < 0.0 {
                break;
            }
        }

        // Projectile should have landed
        let transform = world.get_component::<TransformComponent>(entity).unwrap();
        assert!(transform.transform.position.y() <= 0.0);

        // Should have traveled horizontally (range = v^2 * sin(2*angle) / g)
        let expected_range = (speed * speed * (2.0 * angle).sin()) / 9.81;
        let actual_range = transform.transform.position.x();

        // Within 10% tolerance due to discrete time steps
        assert!((actual_range - expected_range).abs() / expected_range < 0.1);
    }

    #[test]
    fn test_multiple_entities() {
        let mut world = create_test_world();

        let entity1 = world.create_entity();
        let entity2 = world.create_entity();

        // Entity 1: moving right
        world.add_component(
            entity1,
            TransformComponent::new(Transform::from_position(Vec3::new(0.0, 0.0, 0.0))),
        );
        world.add_component(
            entity1,
            VelocityComponent::new(Vec3::new(5.0, 0.0, 0.0), Vec3::default()),
        );

        // Entity 2: moving left
        world.add_component(
            entity2,
            TransformComponent::new(Transform::from_position(Vec3::new(10.0, 0.0, 0.0))),
        );
        world.add_component(
            entity2,
            VelocityComponent::new(Vec3::new(-3.0, 0.0, 0.0), Vec3::default()),
        );

        world.update(1.0);

        let transform1 = world.get_component::<TransformComponent>(entity1).unwrap();
        let transform2 = world.get_component::<TransformComponent>(entity2).unwrap();

        assert!((transform1.transform.position.x() - 5.0).abs() < 0.001);
        assert!((transform2.transform.position.x() - 7.0).abs() < 0.001);
    }

    #[test]
    fn test_zero_mass_no_acceleration() {
        let mut world = create_test_world();

        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::from_position(Vec3::new(0.0, 0.0, 0.0))),
        );
        world.add_component(
            entity,
            VelocityComponent::new(Vec3::default(), Vec3::default()),
        );
        world.add_component(entity, MassComponent { mass: 0.0 });
        world.add_component(
            entity,
            ForceComponent {
                force: Vec3::new(100.0, 0.0, 0.0),
            },
        );

        world.update(0.1);

        let velocity = world.get_component::<VelocityComponent>(entity).unwrap();
        // Should not accelerate with zero mass
        assert_eq!(velocity.velocity.x(), 0.0);
    }

    #[test]
    fn test_no_transform_no_crash() {
        let mut world = create_test_world();

        let entity = world.create_entity();

        // Entity with velocity but no transform
        world.add_component(
            entity,
            VelocityComponent::new(Vec3::new(10.0, 0.0, 0.0), Vec3::default()),
        );

        // Should not crash
        world.update(0.1);

        let velocity = world.get_component::<VelocityComponent>(entity).unwrap();
        assert!((velocity.velocity.x() - 10.0).abs() < 0.001);
    }
}
