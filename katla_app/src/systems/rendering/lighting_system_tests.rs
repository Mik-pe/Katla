use crate::components::{DirectionalLight, PointLight, TransformComponent};
use crate::systems::{LightCollection, LightingSystem};
use katla_ecs::{System, World};
use katla_math::Vec3;

#[test]
fn test_lighting_system_collects_directional_lights() {
    let mut world = World::new();
    let mut system = LightingSystem;

    // Create a directional light
    let entity = world.create_entity();
    world.add_component(
        entity,
        DirectionalLight::new(Vec3::new(-1.0, -1.0, 0.0), [1.0, 1.0, 1.0], 1.0),
    );

    // Run system
    system.update(&mut world, 0.016);

    // Check that light was collected
    let lights = world.get_resource::<LightCollection>();
    assert!(lights.is_some());
    let lights = lights.unwrap();
    assert_eq!(lights.directional_lights.len(), 1);
    assert_eq!(lights.total_lights(), 1);
}

#[test]
fn test_lighting_system_collects_point_lights() {
    let mut world = World::new();
    let mut system = LightingSystem;

    // Create a point light at position
    let entity = world.create_entity();
    world.add_component(
        entity,
        TransformComponent {
            transform: katla_math::Transform::new_from_position(Vec3::new(5.0, 10.0, 3.0)),
        },
    );
    world.add_component(entity, PointLight::white(1.0, 20.0));

    // Run system
    system.update(&mut world, 0.016);

    // Check that light was collected with position
    let lights = world.get_resource::<LightCollection>().unwrap();
    assert_eq!(lights.point_lights.len(), 1);
    assert_eq!(lights.point_lights[0].position, [5.0, 10.0, 3.0]);
}

#[test]
fn test_lighting_system_collects_multiple_lights() {
    let mut world = World::new();
    let mut system = LightingSystem;

    // Create directional light
    let sun = world.create_entity();
    world.add_component(sun, DirectionalLight::white(Vec3::new(-0.5, -1.0, -0.5)));

    // Create point light
    let point = world.create_entity();
    world.add_component(
        point,
        TransformComponent {
            transform: katla_math::Transform::new_from_position(Vec3::new(0.0, 5.0, 0.0)),
        },
    );
    world.add_component(point, PointLight::white(1.0, 15.0));

    // Run system
    system.update(&mut world, 0.016);

    // Check both were collected
    let lights = world.get_resource::<LightCollection>().unwrap();
    assert_eq!(lights.directional_lights.len(), 1);
    assert_eq!(lights.point_lights.len(), 1);
    assert_eq!(lights.total_lights(), 2);
}

#[test]
fn test_lighting_system_respects_max_limits() {
    let mut world = World::new();
    let mut system = LightingSystem;

    // Create more lights than the maximum
    for i in 0..20 {
        let entity = world.create_entity();
        world.add_component(entity, PointLight::white(1.0, 10.0));
        if i < 10 {
            world.add_component(
                entity,
                TransformComponent {
                    transform: katla_math::Transform::new_from_position(Vec3::new(
                        i as f32, 0.0, 0.0,
                    )),
                },
            );
        }
    }

    // Run system
    system.update(&mut world, 0.016);

    // Check that only MAX_POINT_LIGHTS were collected
    let lights = world.get_resource::<LightCollection>().unwrap();
    assert!(lights.point_lights.len() <= LightCollection::MAX_POINT_LIGHTS);
}
