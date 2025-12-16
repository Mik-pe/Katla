//! ECS Framework Demo
//!
//! This example demonstrates the complete usage of the Katla ECS framework
//! with the new storage-based architecture for better cache locality.
//!
//! Run with: cargo run --example ecs_demo

use katla::components::{ActiveComponent, NameComponent, TagComponent, TransformComponent};
use katla_ecs::{Component, ComponentStorageManager, System, SystemExecutionOrder, World};
use katla_math::{Quat, Vec3};

// Custom component: Velocity
#[derive(Component)]
struct VelocityComponent {
    velocity: Vec3,
}

impl VelocityComponent {
    fn new(x: f32, y: f32, z: f32) -> Self {
        Self {
            velocity: Vec3::new(x, y, z),
        }
    }
}

// Custom component: Health
#[derive(Component)]
struct HealthComponent {
    current: f32,
    max: f32,
}

impl HealthComponent {
    fn new(max: f32) -> Self {
        Self { current: max, max }
    }
}

// System 1: Movement System
// Updates position based on velocity
struct MovementSystem;

impl System for MovementSystem {
    fn update(&mut self, storage: &mut ComponentStorageManager, delta_time: f32) {
        // Get entity IDs that have velocity
        let entities_with_velocity: Vec<_> =
            if let Some(velocities) = storage.get_storage::<VelocityComponent>() {
                velocities
                    .iter()
                    .map(|(id, vel)| (id, vel.velocity))
                    .collect()
            } else {
                return;
            };

        // Update transforms based on velocities
        if let Some(transforms) = storage.get_storage_mut::<TransformComponent>() {
            for (entity_id, velocity) in entities_with_velocity {
                if let Some(transform) = transforms.get_mut(entity_id) {
                    transform.transform.position.0[0] += velocity.0[0] * delta_time;
                    transform.transform.position.0[1] += velocity.0[1] * delta_time;
                    transform.transform.position.0[2] += velocity.0[2] * delta_time;
                }
            }
        }
    }

    fn name(&self) -> &str {
        "MovementSystem"
    }
}

// System 2: Rotation System
// Rotates entities around Y axis
struct RotationSystem {
    rotation_speed: f32,
}

impl RotationSystem {
    fn new(degrees_per_second: f32) -> Self {
        Self {
            rotation_speed: degrees_per_second,
        }
    }
}

impl System for RotationSystem {
    fn update(&mut self, storage: &mut ComponentStorageManager, delta_time: f32) {
        let rotation_radians = self.rotation_speed.to_radians() * delta_time;
        let axis = Vec3::new(0.0, 1.0, 0.0);

        if let Some(transforms) = storage.get_storage_mut::<TransformComponent>() {
            for (_entity_id, transform) in transforms.iter_mut() {
                let rotation_quat = Quat::from_axis_angle(axis, rotation_radians);
                transform.transform.rotation = rotation_quat * transform.transform.rotation;
            }
        }
    }

    fn name(&self) -> &str {
        "RotationSystem"
    }
}

// System 3: Health Regeneration System
// Regenerates health over time
struct HealthRegenSystem {
    regen_rate: f32,
}

impl HealthRegenSystem {
    fn new(regen_per_second: f32) -> Self {
        Self {
            regen_rate: regen_per_second,
        }
    }
}

impl System for HealthRegenSystem {
    fn update(&mut self, storage: &mut ComponentStorageManager, delta_time: f32) {
        if let Some(healths) = storage.get_storage_mut::<HealthComponent>() {
            for (_entity_id, health) in healths.iter_mut() {
                health.current = (health.current + self.regen_rate * delta_time).min(health.max);
            }
        }
    }

    fn name(&self) -> &str {
        "HealthRegenSystem"
    }
}

// System 4: Status Logger System
// Logs entity status periodically
struct StatusLoggerSystem {
    log_interval: f32,
    time_accumulator: f32,
}

impl StatusLoggerSystem {
    fn new(interval: f32) -> Self {
        Self {
            log_interval: interval,
            time_accumulator: 0.0,
        }
    }
}

impl System for StatusLoggerSystem {
    fn update(&mut self, storage: &mut ComponentStorageManager, delta_time: f32) {
        self.time_accumulator += delta_time;

        if self.time_accumulator >= self.log_interval {
            self.time_accumulator = 0.0;

            let transforms = storage.get_storage::<TransformComponent>();
            let names = storage.get_storage::<NameComponent>();
            let healths = storage.get_storage::<HealthComponent>();
            let tags = storage.get_storage::<TagComponent>();

            if let (Some(transforms), Some(names)) = (transforms, names) {
                println!("\n=== Entity Status Report ===");
                for (entity_id, name) in names.iter() {
                    if let Some(transform) = transforms.get(entity_id) {
                        let pos = &transform.transform.position;
                        print!(
                            "  {} - Position: ({:.2}, {:.2}, {:.2})",
                            name.name, pos.0[0], pos.0[1], pos.0[2]
                        );

                        // If entity has health, print it
                        if let Some(healths) = healths {
                            if let Some(health) = healths.get(entity_id) {
                                print!(" | Health: {:.1}/{:.1}", health.current, health.max);
                            }
                        }

                        // If entity has tag, print it
                        if let Some(tags) = tags {
                            if let Some(tag) = tags.get(entity_id) {
                                print!(" | Tag: {}", tag.tag);
                            }
                        }

                        println!();
                    }
                }
                println!("===========================\n");
            }
        }
    }

    fn name(&self) -> &str {
        "StatusLoggerSystem"
    }
}

fn main() {
    println!("Katla ECS Framework Demo \n");
    println!("This demo showcases:");
    println!("  - Creating entities with components");
    println!("  - Implementing custom systems");
    println!("  - Running the ECS update loop");
    println!("  - Direct component storage iteration for performance\n");

    // Create the world
    let mut world = World::new();

    // Create Player entity
    println!("Creating Player entity...");
    let player = world.create_entity();
    world.add_component(player, TransformComponent::default());
    world.add_component(player, VelocityComponent::new(1.0, 0.0, 0.5));
    world.add_component(player, HealthComponent::new(100.0));
    world.add_component(player, NameComponent::new("Player"));
    world.add_component(player, TagComponent::new("Player"));
    world.add_component(player, ActiveComponent::new());

    // Damage the player a bit so we can see health regen
    if let Some(health) = world.get_component_mut::<HealthComponent>(player) {
        health.current = 50.0;
    }

    // Create Enemy entities
    println!("Creating Enemy entities...");
    for i in 0..3 {
        let enemy = world.create_entity();
        let mut transform = TransformComponent::default();
        transform.transform.position = Vec3::new((i as f32) * 3.0, 0.0, 5.0);

        world.add_component(enemy, transform);
        world.add_component(enemy, VelocityComponent::new(-0.5, 0.0, 0.2));
        world.add_component(enemy, HealthComponent::new(50.0));
        world.add_component(enemy, NameComponent::new(format!("Enemy_{}", i)));
        world.add_component(enemy, TagComponent::new("Enemy"));
        world.add_component(enemy, ActiveComponent::new());
    }

    // Create a rotating platform (no health, no velocity)
    println!("Creating Platform entity...");
    let platform = world.create_entity();
    let mut platform_transform = TransformComponent::default();
    platform_transform.transform.position = Vec3::new(0.0, -2.0, 0.0);
    platform_transform.transform.scale = Vec3::new(5.0, 0.5, 5.0);

    world.add_component(platform, platform_transform);
    world.add_component(platform, NameComponent::new("Platform"));
    world.add_component(platform, TagComponent::new("Platform"));
    world.add_component(platform, ActiveComponent::new());

    println!("\nTotal entities created: {}", world.entity_count());

    // Register systems
    println!("\nRegistering systems...");
    world.register_system(Box::new(MovementSystem), SystemExecutionOrder::EARLY);

    world.register_system(
        Box::new(RotationSystem::new(45.0)), // 45 degrees per second
        SystemExecutionOrder::NORMAL,
    );

    world.register_system(
        Box::new(HealthRegenSystem::new(5.0)), // 5 HP per second
        SystemExecutionOrder::NORMAL,
    );

    world.register_system(
        Box::new(StatusLoggerSystem::new(2.0)), // Log every 2 seconds
        SystemExecutionOrder::LATE,
    );

    println!("Systems registered: {}", world.system_count());

    // Simulate game loop
    println!("\nStarting simulation (running for 10 seconds)...\n");

    let delta_time = 0.016; // 60 FPS
    let total_frames = (10.0 / delta_time) as i32;

    for frame in 0..total_frames {
        world.update(delta_time);

        // Every 60 frames (1 second), print a progress indicator
        if frame % 60 == 0 {
            let seconds = frame as f32 * delta_time;
            println!("Simulation time: {:.1}s", seconds);
        }
    }

    println!("\n=== Final Statistics ===");
    println!("Total entities: {}", world.entity_count());
    println!("Total systems: {}", world.system_count());
    println!("Total frames simulated: {}", total_frames);

    // Print final positions using component storage directly
    println!("\n=== Final Entity States ===");
    let storage = world.storage();

    if let (Some(names), Some(transforms)) = (
        storage.get_storage::<NameComponent>(),
        storage.get_storage::<TransformComponent>(),
    ) {
        for (entity_id, name) in names.iter() {
            if let Some(transform) = transforms.get(entity_id) {
                let pos = &transform.transform.position;
                println!(
                    "{}: Position ({:.2}, {:.2}, {:.2})",
                    name.name, pos.0[0], pos.0[1], pos.0[2]
                );

                if let Some(healths) = storage.get_storage::<HealthComponent>() {
                    if let Some(health) = healths.get(entity_id) {
                        println!("  Health: {:.1}/{:.1}", health.current, health.max);
                    }
                }
            }
        }
    }

    println!("\n=== Demo Complete ===");
}
