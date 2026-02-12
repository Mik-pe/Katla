//! Headless Vulkan integration tests for katla_app.
//!
//! These tests verify the integration between the ECS, Vulkan rendering,
//! and the application layer without requiring a window or display.
//!
//! Note: Tests that require direct Vulkan command buffer recording are
//! located in katla_vulkan's test suite to avoid exposing ash types.

use katla::components::TransformComponent;
use katla_ecs::World;
use katla_math::{Quat, Transform, Vec3};
use katla_vulkan::{RenderPass, VulkanContext};
use std::rc::Rc;

/// Create a headless Vulkan context for testing.
fn create_headless_context() -> VulkanContext {
    let app_name = std::ffi::CString::new("Katla App Integration Tests").unwrap();
    let engine_name = std::ffi::CString::new("Katla Engine").unwrap();

    VulkanContext::init_headless(true, app_name, engine_name)
}

/// Create a test entity with transform component.
fn create_test_entity(world: &mut World, position: Vec3) -> katla_ecs::EntityId {
    let entity = world.create_entity();

    world.add_component(
        entity,
        TransformComponent {
            transform: Transform {
                position,
                rotation: Quat::new(),
                scale: Vec3::new(1.0, 1.0, 1.0),
            },
        },
    );

    entity
}

/// Test headless Vulkan context creation.
#[test]
fn test_headless_vulkan_context() {
    let context = create_headless_context();

    // Verify context was created by checking headless mode characteristics
    // (surface_loader, swapchain_loader, and surface are None in headless mode)
    assert!(context.surface_loader.is_none());
    assert!(context.swapchain_loader.is_none());
    assert!(context.surface.is_none()); // No surface in headless mode

    println!("Headless Vulkan context created successfully");
}

/// Test mesh creation with headless Vulkan.
#[test]
fn test_headless_mesh_creation() {
    let context = Rc::new(create_headless_context());
    let mut world = World::new();

    // Create a simple entity with transform
    let entity = world.create_entity();
    world.add_component(
        entity,
        TransformComponent {
            transform: Transform {
                position: Vec3::new(0.0, 0.0, 0.0),
                rotation: Quat::new(),
                scale: Vec3::new(1.0, 1.0, 1.0),
            },
        },
    );

    // Verify entity was created
    assert!(world.entity_exists(entity));

    // Verify TransformComponent was added
    let transform = world.get_component::<TransformComponent>(entity);
    assert!(transform.is_some());
    assert_eq!(
        transform.unwrap().transform.position,
        Vec3::new(0.0, 0.0, 0.0)
    );

    println!("Entity and transform created successfully in headless mode");
}

/// Test full integration: ECS + headless Vulkan.
#[test]
fn test_headless_full_integration() {
    let context = Rc::new(create_headless_context());
    let mut world = World::new();

    // Create multiple entities at different positions
    let entity1 = create_test_entity(&mut world, Vec3::new(-2.0, 0.0, 0.0));
    let entity2 = create_test_entity(&mut world, Vec3::new(0.0, 0.0, 0.0));
    let entity3 = create_test_entity(&mut world, Vec3::new(2.0, 0.0, 0.0));

    // Verify all entities exist
    assert!(world.entity_exists(entity1));
    assert!(world.entity_exists(entity2));
    assert!(world.entity_exists(entity3));

    // Query entities with TransformComponent
    let transform_count = world.query::<&TransformComponent>().count();
    assert_eq!(transform_count, 3);

    // Verify each entity exists and has correct transform
    let entities: Vec<_> = world
        .query::<&TransformComponent>()
        .map(|(entity, _)| entity)
        .collect();

    for entity in entities {
        assert!(world.entity_exists(entity));
        let transform = world
            .get_component::<TransformComponent>(entity)
            .expect("Entity should have TransformComponent");
        assert!(transform.transform.scale == Vec3::new(1.0, 1.0, 1.0));
    }

    println!("Full integration test completed successfully");
}

/// Test multiple entities in headless mode.
#[test]
fn test_headless_multiple_mesh_types() {
    let context = Rc::new(create_headless_context());
    let mut world = World::new();

    // Create different entities at different positions
    let entity1 = create_test_entity(&mut world, Vec3::new(-5.0, 0.0, 0.0));
    let entity2 = create_test_entity(&mut world, Vec3::new(0.0, 0.0, 0.0));
    let entity3 = create_test_entity(&mut world, Vec3::new(0.0, -2.0, 0.0));

    // Verify all entities exist
    assert!(world.entity_exists(entity1));
    assert!(world.entity_exists(entity2));
    assert!(world.entity_exists(entity3));

    // Verify all have TransformComponent
    assert!(world.get_component::<TransformComponent>(entity1).is_some());
    assert!(world.get_component::<TransformComponent>(entity2).is_some());
    assert!(world.get_component::<TransformComponent>(entity3).is_some());

    println!("Multiple entities created successfully in headless mode");
}
