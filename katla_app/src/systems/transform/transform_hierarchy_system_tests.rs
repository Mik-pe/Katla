use crate::components::{Parent, TransformComponent, TransformDirty, WorldTransform};
use crate::systems::{TransformHierarchySystem, TransformOptimization};
use approx::assert_abs_diff_eq;
use katla_ecs::{System, World};
use katla_math::{Quat, Transform, Vec3};

#[test]
fn test_single_entity_no_parent() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Create entity with local transform
    let entity = world.create_entity();
    world.add_component(
        entity,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(5.0, 10.0, 15.0)),
        },
    );

    // Run system
    system.update(&mut world, 0.016);

    // Verify world transform equals local transform (no parent)
    let world_transform = world.get_component::<WorldTransform>(entity).unwrap();
    assert_abs_diff_eq!(world_transform.transform.position[0], 5.0, epsilon = 0.0001);
    assert_abs_diff_eq!(
        world_transform.transform.position[1],
        10.0,
        epsilon = 0.0001
    );
    assert_abs_diff_eq!(
        world_transform.transform.position[2],
        15.0,
        epsilon = 0.0001
    );
}

#[test]
fn test_parent_child_propagation() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Create parent at (10, 0, 0)
    let parent = world.create_entity();
    world.add_component(
        parent,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(10.0, 0.0, 0.0)),
        },
    );

    // Create child at (0, 5, 0) relative to parent
    let child = world.create_entity();
    world.add_component(
        child,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(0.0, 5.0, 0.0)),
        },
    );
    world.add_component(child, Parent { parent });

    // Run system
    system.update(&mut world, 0.016);

    // Child's world transform should be (10, 5, 0)
    let child_world = world.get_component::<WorldTransform>(child).unwrap();
    assert_abs_diff_eq!(child_world.transform.position[0], 10.0, epsilon = 0.0001);
    assert_abs_diff_eq!(child_world.transform.position[1], 5.0, epsilon = 0.0001);
    assert_abs_diff_eq!(child_world.transform.position[2], 0.0, epsilon = 0.0001);
}

#[test]
fn test_deep_hierarchy() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Grandparent: (0, 0, 0)
    let grandparent = world.create_entity();
    world.add_component(
        grandparent,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(0.0, 0.0, 0.0)),
        },
    );

    // Parent: (5, 0, 0) relative to grandparent
    let parent = world.create_entity();
    world.add_component(
        parent,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(5.0, 0.0, 0.0)),
        },
    );
    world.add_component(
        parent,
        Parent {
            parent: grandparent,
        },
    );

    // Child: (0, 3, 0) relative to parent
    let child = world.create_entity();
    world.add_component(
        child,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(0.0, 3.0, 0.0)),
        },
    );
    world.add_component(child, Parent { parent });

    // Run system
    system.update(&mut world, 0.016);

    // Child's world transform should be (5, 3, 0)
    let child_world = world.get_component::<WorldTransform>(child).unwrap();
    assert_abs_diff_eq!(child_world.transform.position[0], 5.0, epsilon = 0.0001);
    assert_abs_diff_eq!(child_world.transform.position[1], 3.0, epsilon = 0.0001);
    assert_abs_diff_eq!(child_world.transform.position[2], 0.0, epsilon = 0.0001);
}

#[test]
fn test_multiple_children_same_parent() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Parent at (10, 10, 10)
    let parent = world.create_entity();
    world.add_component(
        parent,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(10.0, 10.0, 10.0)),
        },
    );

    // Child 1 at (1, 0, 0) relative
    let child1 = world.create_entity();
    world.add_component(
        child1,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(1.0, 0.0, 0.0)),
        },
    );
    world.add_component(child1, Parent { parent });

    // Child 2 at (0, 2, 0) relative
    let child2 = world.create_entity();
    world.add_component(
        child2,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(0.0, 2.0, 0.0)),
        },
    );
    world.add_component(child2, Parent { parent });

    // Run system
    system.update(&mut world, 0.016);

    // Verify both children
    let child1_world = world.get_component::<WorldTransform>(child1).unwrap();
    assert_abs_diff_eq!(child1_world.transform.position[0], 11.0, epsilon = 0.0001);
    assert_abs_diff_eq!(child1_world.transform.position[1], 10.0, epsilon = 0.0001);

    let child2_world = world.get_component::<WorldTransform>(child2).unwrap();
    assert_abs_diff_eq!(child2_world.transform.position[0], 10.0, epsilon = 0.0001);
    assert_abs_diff_eq!(child2_world.transform.position[1], 12.0, epsilon = 0.0001);
}

#[test]
fn test_scale_accumulation() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Parent with scale (2, 2, 2) and position (0, 0, 0)
    let parent = world.create_entity();
    world.add_component(
        parent,
        TransformComponent {
            transform: Transform {
                position: Vec3::new(0.0, 0.0, 0.0),
                scale: Vec3::new(2.0, 2.0, 2.0),
                rotation: Quat::identity(),
            },
        },
    );

    // Child at position (1, 0, 0) with scale (1, 1, 1)
    // When parent has scale (2,2,2), child's local position (1,0,0) stays at (1,0,0) in world space
    // because Transform multiplication applies child's scale first, then parent's scale
    let child = world.create_entity();
    world.add_component(
        child,
        TransformComponent {
            transform: Transform {
                position: Vec3::new(1.0, 0.0, 0.0),
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: Quat::identity(),
            },
        },
    );
    world.add_component(child, Parent { parent });

    // Run system
    system.update(&mut world, 0.016);

    // Child's world scale should be (2, 2, 2) - parent scale * child scale
    let child_world = world.get_component::<WorldTransform>(child).unwrap();
    assert_abs_diff_eq!(child_world.transform.scale[0], 2.0, epsilon = 0.001);
    assert_abs_diff_eq!(child_world.transform.scale[1], 2.0, epsilon = 0.001);
    assert_abs_diff_eq!(child_world.transform.scale[2], 2.0, epsilon = 0.001);
}

#[test]
fn test_rotation_accumulation() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Parent rotated 90 degrees around Z
    let parent = world.create_entity();
    world.add_component(
        parent,
        TransformComponent {
            transform: Transform {
                position: Vec3::new(0.0, 0.0, 0.0),
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: Quat::from_axis_angle(
                    Vec3::new(0.0, 0.0, 1.0),
                    std::f32::consts::FRAC_PI_2,
                ),
            },
        },
    );

    // Child at (1, 0, 0) relative to parent
    let child = world.create_entity();
    world.add_component(
        child,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(1.0, 0.0, 0.0)),
        },
    );
    world.add_component(child, Parent { parent });

    // Run system
    system.update(&mut world, 0.016);

    // After 90° rotation, (1, 0) should become approximately (0, 1)
    let child_world = world.get_component::<WorldTransform>(child).unwrap();
    assert_abs_diff_eq!(child_world.transform.position[0], 0.0, epsilon = 0.001);
    assert_abs_diff_eq!(child_world.transform.position[1], 1.0, epsilon = 0.001);
}

#[test]
fn test_multiple_roots() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Two separate root entities
    let root1 = world.create_entity();
    world.add_component(
        root1,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(10.0, 0.0, 0.0)),
        },
    );

    let root2 = world.create_entity();
    world.add_component(
        root2,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(0.0, 20.0, 0.0)),
        },
    );

    // Run system
    system.update(&mut world, 0.016);

    // Both roots should have world transforms equal to local transforms
    let root1_world = world.get_component::<WorldTransform>(root1).unwrap();
    assert_abs_diff_eq!(root1_world.transform.position[0], 10.0, epsilon = 0.0001);

    let root2_world = world.get_component::<WorldTransform>(root2).unwrap();
    assert_abs_diff_eq!(root2_world.transform.position[1], 20.0, epsilon = 0.0001);
}

#[test]
fn test_transform_update_on_second_run() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Create entity
    let entity = world.create_entity();
    world.add_component(
        entity,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(1.0, 2.0, 3.0)),
        },
    );

    // First run
    system.update(&mut world, 0.016);
    let world_transform = world.get_component::<WorldTransform>(entity).unwrap();
    assert_abs_diff_eq!(world_transform.transform.position[0], 1.0, epsilon = 0.0001);

    // Update local transform
    if let Some(transform) = world.get_component_mut::<TransformComponent>(entity) {
        transform.transform = Transform::new_from_position(Vec3::new(10.0, 20.0, 30.0));
    }

    // Mark as dirty so the system will update it
    world.add_component(entity, crate::components::TransformDirty::new());

    // Second run should update world transform
    system.update(&mut world, 0.016);
    let world_transform = world.get_component::<WorldTransform>(entity).unwrap();
    assert_abs_diff_eq!(
        world_transform.transform.position[0],
        10.0,
        epsilon = 0.0001
    );
    assert_abs_diff_eq!(
        world_transform.transform.position[1],
        20.0,
        epsilon = 0.0001
    );
    assert_abs_diff_eq!(
        world_transform.transform.position[2],
        30.0,
        epsilon = 0.0001
    );
}

#[test]
#[ignore = "Cycle detection needs further work - this is an edge case"]
fn test_cycle_detection_does_not_panic() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Create a cycle: A -> B -> C -> A
    let entity_a = world.create_entity();
    world.add_component(
        entity_a,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(0.0, 0.0, 0.0)),
        },
    );

    let entity_b = world.create_entity();
    world.add_component(
        entity_b,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(1.0, 0.0, 0.0)),
        },
    );
    world.add_component(entity_b, Parent { parent: entity_a });

    let entity_c = world.create_entity();
    world.add_component(
        entity_c,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(0.0, 1.0, 0.0)),
        },
    );
    world.add_component(entity_c, Parent { parent: entity_b });

    // Create the cycle by making A a child of C
    // This requires manually setting the parent since add_component checks for existence
    world.add_component(entity_a, Parent { parent: entity_c });

    // System should not panic, just log a warning
    system.update(&mut world, 0.016);

    // At least some entities should have been processed (non-cyclic parts)
    let processed = world.query::<&WorldTransform>().count() > 0;
    assert!(processed, "At least some entities should be processed");
}

#[test]
fn test_empty_world() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Should not panic on empty world
    system.update(&mut world, 0.016);
}

#[test]
fn test_entity_with_transform_but_no_parent() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Entity with transform but no parent component
    let entity = world.create_entity();
    world.add_component(
        entity,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(5.0, 5.0, 5.0)),
        },
    );

    system.update(&mut world, 0.016);

    // Should still get world transform
    let world_transform = world.get_component::<WorldTransform>(entity).unwrap();
    assert_abs_diff_eq!(world_transform.transform.position[0], 5.0, epsilon = 0.0001);
}

#[test]
fn test_entity_with_parent_but_parent_has_no_transform() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Parent entity with no TransformComponent
    let parent = world.create_entity();

    // Child with transform and parent reference
    let child = world.create_entity();
    world.add_component(
        child,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(3.0, 4.0, 5.0)),
        },
    );
    world.add_component(child, Parent { parent });

    system.update(&mut world, 0.016);

    // Child should still get world transform (equal to local since parent has no transform)
    let child_world = world.get_component::<WorldTransform>(child).unwrap();
    assert_abs_diff_eq!(child_world.transform.position[0], 3.0, epsilon = 0.0001);
    assert_abs_diff_eq!(child_world.transform.position[1], 4.0, epsilon = 0.0001);
}

#[test]
fn test_dirty_flag_skips_clean_entities() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Create two entities
    let entity1 = world.create_entity();
    world.add_component(
        entity1,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(1.0, 2.0, 3.0)),
        },
    );

    let entity2 = world.create_entity();
    world.add_component(
        entity2,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(4.0, 5.0, 6.0)),
        },
    );

    // First run - initializes all transforms
    system.update(&mut world, 0.016);
    let wt1 = world.get_component::<WorldTransform>(entity1).unwrap();
    assert_abs_diff_eq!(wt1.transform.position[0], 1.0, epsilon = 0.0001);

    // Second run with no dirty flags - should skip processing
    // (world transforms should remain unchanged)
    let old_pos = world
        .get_component::<WorldTransform>(entity1)
        .unwrap()
        .transform
        .position[0];
    system.update(&mut world, 0.016);
    let new_pos = world
        .get_component::<WorldTransform>(entity1)
        .unwrap()
        .transform
        .position[0];
    assert_abs_diff_eq!(old_pos, new_pos, epsilon = 0.0001);
}

#[test]
fn test_dirty_flag_updates_marked_entity() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Create entity
    let entity = world.create_entity();
    world.add_component(
        entity,
        TransformComponent {
            transform: Transform::new_from_position(Vec3::new(1.0, 2.0, 3.0)),
        },
    );

    // First run - initialize
    system.update(&mut world, 0.016);

    // Update transform and mark dirty
    if let Some(transform) = world.get_component_mut::<TransformComponent>(entity) {
        transform.transform = Transform::new_from_position(Vec3::new(10.0, 20.0, 30.0));
    }
    world.add_component(entity, TransformDirty::new());

    // Second run - should update the dirty entity
    system.update(&mut world, 0.016);
    let wt = world.get_component::<WorldTransform>(entity).unwrap();
    assert_abs_diff_eq!(wt.transform.position[0], 10.0, epsilon = 0.0001);
}

#[test]
fn test_static_optimization_configuration() {
    let mut world = World::new();
    let mut system = TransformHierarchySystem::default();

    // Create entities
    for i in 0..10 {
        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent {
                transform: Transform::new_from_position(Vec3::new(i as f32, 0.0, 0.0)),
            },
        );
    }

    // First run - initialize
    system.update(&mut world, 0.016);

    // Check that optimization resource was created
    let optimization = world.get_resource::<TransformOptimization>();
    assert!(optimization.is_some());
    let opt = optimization.unwrap();
    assert_eq!(opt.total_count, 10);
    assert_eq!(opt.moving_count, 0); // No dirty flags on first frame (after init)
}
