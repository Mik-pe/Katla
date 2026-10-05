use super::*;

#[test]
fn test_validate_empty_world() {
    let world = World::new();
    assert!(world.validate().is_ok());
}

#[test]
fn test_validate_with_entities_and_components() {
    let mut world = World::new();

    let id1 = world.create_entity();
    world.add_component(id1, TestComponent { value: 1 });

    let id2 = world.create_entity();
    world.add_component(id2, TestComponent { value: 2 });

    assert!(world.validate().is_ok());
}

#[test]
fn test_validate_after_entity_destruction() {
    let mut world = World::new();

    let id1 = world.create_entity();
    world.add_component(id1, TestComponent { value: 1 });

    let id2 = world.create_entity();
    world.add_component(id2, TestComponent { value: 2 });

    world.destroy_entity(id1);

    assert!(world.validate().is_ok());
    assert!(!world.entity_exists(id1));
    assert!(world.entity_exists(id2));
}

#[test]
fn test_validate_after_clear_entities() {
    let mut world = World::new();

    for i in 0..20 {
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: i });
    }

    world.clear_entities();
    assert!(world.validate().is_ok());
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn test_validate_after_stress_create_destroy() {
    let mut world = World::new();
    let mut ids = Vec::new();

    for i in 0..200 {
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: i });
        ids.push(id);
    }

    // Destroy every other entity
    for (i, id) in ids.iter().enumerate() {
        if i % 2 == 0 {
            world.destroy_entity(*id);
        }
    }

    assert!(world.validate().is_ok());
}

#[test]
fn test_validate_entities_all_valid() {
    let mut world = World::new();

    let id1 = world.create_entity();
    let id2 = world.create_entity();
    let id3 = world.create_entity();

    assert!(world.validate_entities(&[id1, id2, id3]));
}

#[test]
fn test_validate_entities_some_invalid() {
    let mut world = World::new();

    let id1 = world.create_entity();
    let id2 = world.create_entity();
    world.destroy_entity(id2);

    assert!(!world.validate_entities(&[id1, id2]));
}

#[test]
fn test_validate_entities_empty_slice() {
    let world = World::new();
    assert!(world.validate_entities(&[]));
}

#[test]
fn test_validate_entities_all_destroyed() {
    let mut world = World::new();

    let id1 = world.create_entity();
    let id2 = world.create_entity();
    world.destroy_entity(id1);
    world.destroy_entity(id2);

    assert!(!world.validate_entities(&[id1, id2]));
}

#[test]
fn test_validate_after_component_operations() {
    let mut world = World::new();

    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 1 });
    assert!(world.validate().is_ok());

    world.remove_component::<TestComponent>(id);
    assert!(world.validate().is_ok());

    world.add_component(id, TestComponent { value: 2 });
    assert!(world.validate().is_ok());

    world.destroy_entity(id);
    assert!(world.validate().is_ok());
}
