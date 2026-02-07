//! Integration tests for katla_app ECS functionality.
//!
//! These tests verify that the application layer components, systems,
//! and entities work correctly without requiring a window or Vulkan.

use katla::components::{DirectionalLight, PointLight, TransformComponent};
use katla::entities::Camera;
use katla_ecs::{System, SystemExecutionOrder, World};
use katla_math::{Transform, Vec3};

/// Test basic world creation and entity management.
#[test]
fn test_world_creation() {
    let mut world = World::new();

    // Create a simple entity
    let entity = world.create_entity();
    assert!(world.entity_exists(entity));

    // Add a transform component
    let transform = TransformComponent {
        transform: Transform::new_from_position(Vec3::new(1.0, 2.0, 3.0)),
    };
    world.add_component(entity, transform);

    // Verify component was added
    let result = world.get_component::<TransformComponent>(entity);
    assert!(result.is_some());
    let retrieved_transform = result.unwrap();
    assert_eq!(
        retrieved_transform.transform.position,
        Vec3::new(1.0, 2.0, 3.0)
    );
}

/// Test camera entity creation.
#[test]
fn test_camera_creation() {
    let mut world = World::new();
    let _camera = Camera::new(&mut world);

    // Camera creates its own entities internally
    // Verify that entities were created by checking entity count
    // (Camera should create at least one entity for itself)
    assert!(world.entity_count() >= 1);
}

/// Test lighting components.
#[test]
fn test_lighting_components() {
    let mut world = World::new();

    // Create directional light
    let sun_entity = world.create_entity();
    world.add_component(
        sun_entity,
        DirectionalLight::new(
            Vec3::new(-0.3, -1.0, -0.2),
            [1.0, 0.95, 0.8],
            1.0,
        ),
    );

    // Verify directional light was added
    let light_result = world.get_component::<DirectionalLight>(sun_entity);
    assert!(light_result.is_some());

    // Create point light with transform
    let point_light_entity = world.create_entity();
    world.add_component(
        point_light_entity,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(0.0, 5.0, 0.0)),
        },
    );
    world.add_component(
        point_light_entity,
        PointLight::new([1.0, 0.5, 0.3], 2.0, 15.0),
    );

    // Verify both components were added
    let transform_result = world.get_component::<TransformComponent>(point_light_entity);
    let point_light_result = world.get_component::<PointLight>(point_light_entity);

    assert!(transform_result.is_some());
    assert!(point_light_result.is_some());

    // Verify point light properties
    let point_light = point_light_result.unwrap();
    assert_eq!(point_light.color, [1.0, 0.5, 0.3]);
    assert_eq!(point_light.intensity, 2.0);
    assert_eq!(point_light.range, 15.0);
}

/// Test system execution.
#[test]
fn test_system_execution() {
    // Create a simple test system
    struct TestSystem {
        update_count: std::cell::RefCell<usize>,
    }

    impl System for TestSystem {
        fn update(&mut self, _world: &mut World, _delta_time: f32) {
            *self.update_count.borrow_mut() += 1;
        }

        fn initialize(&mut self) {}
        fn shutdown(&mut self) {}
    }

    let mut world = World::new();
    let system = TestSystem {
        update_count: std::cell::RefCell::new(0),
    };

    // Register system
    world.register_system(Box::new(system), SystemExecutionOrder::default());

    // Update world multiple times
    world.update(0.016);
    world.update(0.016);
    world.update(0.016);

    // Note: We can't easily verify the update count without exposing it
    // The important part is that the system ran without panicking
}

/// Test component queries.
#[test]
fn test_component_queries() {
    let mut world = World::new();

    // Create entities with different component combinations
    let entity1 = world.create_entity();
    world.add_component(
        entity1,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(0.0, 0.0, 0.0)),
        },
    );
    world.add_component(
        entity1,
        DirectionalLight::new(Vec3::new(0.0, -1.0, 0.0), [1.0, 1.0, 1.0], 1.0),
    );

    let entity2 = world.create_entity();
    world.add_component(
        entity2,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(1.0, 0.0, 0.0)),
        },
    );

    let entity3 = world.create_entity();
    world.add_component(
        entity3,
        DirectionalLight::new(Vec3::new(0.0, -1.0, 0.0), [0.8, 0.8, 0.8], 0.8),
    );

    // Query for entities with TransformComponent
    let transform_count = world.query::<&TransformComponent>().count();
    assert_eq!(transform_count, 2);

    // Query for entities with DirectionalLight
    let light_count = world.query::<&DirectionalLight>().count();
    assert_eq!(light_count, 2);

    // Query for entities with both components
    let both_count = world
        .query::<(&TransformComponent, &DirectionalLight)>()
        .count();
    assert_eq!(both_count, 1);
}

/// Test world resource management.
#[test]
fn test_world_resources() {
    use katla::components::AmbientLight;

    let mut world = World::new();

    // Insert a resource
    world.insert_resource(AmbientLight::gray(0.2));

    // Retrieve the resource
    let ambient = world.get_resource::<AmbientLight>();
    assert!(ambient.is_some());

    let ambient_light = ambient.unwrap();
    assert_eq!(ambient_light.color, [0.2, 0.2, 0.2]);
}

/// Test entity lifecycle.
#[test]
fn test_entity_lifecycle() {
    let mut world = World::new();

    // Create entity
    let entity = world.create_entity();
    assert!(world.entity_exists(entity));

    // Add component
    world.add_component(
        entity,
        TransformComponent {
            transform: Transform::default(),
        },
    );

    // Destroy entity
    let destroyed = world.destroy_entity(entity);
    assert!(destroyed);

    // Verify entity is no longer alive
    assert!(!world.entity_exists(entity));

    // Verify component is gone
    let result = world.get_component::<TransformComponent>(entity);
    assert!(result.is_none());
}

/// Test multiple entity creation and counting.
#[test]
fn test_multiple_entities() {
    let mut world = World::new();

    // Create multiple entities
    let e1 = world.create_entity();
    let e2 = world.create_entity();
    let e3 = world.create_entity();

    // Verify all entities exist
    assert!(world.entity_exists(e1));
    assert!(world.entity_exists(e2));
    assert!(world.entity_exists(e3));

    // Verify entity count
    assert_eq!(world.entity_count(), 3);
}

/// Test component removal.
#[test]
fn test_component_removal() {
    let mut world = World::new();

    // Create entity with component
    let entity = world.create_entity();
    world.add_component(
        entity,
        TransformComponent {
            transform: Transform::default(),
        },
    );

    // Verify component exists
    assert!(world
        .get_component::<TransformComponent>(entity)
        .is_some());

    // Remove component
    let removed = world.remove_component::<TransformComponent>(entity);
    assert!(removed);

    // Verify component is gone
    assert!(world
        .get_component::<TransformComponent>(entity)
        .is_none());
}
