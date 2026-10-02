use super::*;

#[test]
fn test_destroy_entity() {
    let mut world = World::new();
    let id = world.create_entity();

    assert_eq!(world.entity_count(), 1);
    assert!(world.destroy_entity(id));
    assert_eq!(world.entity_count(), 0);
    assert!(!world.entity_exists(id));
}

#[test]
fn test_get_component_mut() {
    let mut world = World::new();
    let id = world.create_entity();

    world.add_component(id, TestComponent::default());

    if let Some(test) = world.get_component_mut::<TestComponent>(id) {
        test.value = 5;
    }

    let transform = world.get_component::<TestComponent>(id).unwrap();
    assert_eq!(transform.value, 5);
}

#[test]
fn test_clear_entities() {
    let mut world = World::new();
    world.create_entity();
    world.create_entity();
    world.create_entity();

    assert_eq!(world.entity_count(), 3);
    world.clear_entities();
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn test_destroy_entity_removes_components() {
    let mut world = World::new();
    let id = world.create_entity();

    world.add_component(id, TestComponent::default());
    assert!(world.get_component::<TestComponent>(id).is_some());

    world.destroy_entity(id);

    // Component should be removed when entity is destroyed
    assert!(world.get_component::<TestComponent>(id).is_none());
}

#[test]
fn test_stale_entity_reference() {
    let mut world = World::new();

    // Create and destroy entity
    let id1 = world.create_entity();
    world.add_component(id1, TestComponent { value: 42 });

    world.destroy_entity(id1);

    // Old ID should no longer be valid
    assert!(!world.entity_exists(id1));

    // Create new entity (should reuse slot with incremented generation)
    let id2 = world.create_entity();

    // The old ID should still be invalid
    assert!(!world.entity_exists(id1));

    // The new ID should be valid
    assert!(world.entity_exists(id2));
}

#[test]
fn test_component_access_invalid_entity() {
    let mut world = World::new();
    let id = world.create_entity();
    world.destroy_entity(id);

    // Should return None for invalid entity
    assert!(world.get_component::<TestComponent>(id).is_none());
    assert!(world.get_component_mut::<TestComponent>(id).is_none());

    // add_component should be a no-op for invalid entity
    world.add_component(id, TestComponent::default());
    assert!(world.get_component::<TestComponent>(id).is_none());
}

#[test]
fn test_entity_double_destroy_no_panic() {
    let mut world = World::new();
    let id = world.create_entity();

    world.add_component(id, TestComponent { value: 42 });

    // First destroy should succeed
    assert!(world.destroy_entity(id));
    // Second destroy should return false but not panic
    assert!(!world.destroy_entity(id));
}

#[test]
fn test_query_returns_correct_data_not_just_count() {
    let mut world = World::new();

    let id1 = world.create_entity();
    world.add_component(id1, TestComponent { value: 10 });

    let id2 = world.create_entity();
    world.add_component(id2, TestComponent { value: 20 });

    let id3 = world.create_entity();
    world.add_component(id3, TestComponent { value: 30 });

    let mut values: Vec<i32> = world
        .query::<&TestComponent>()
        .map(|(_, comp)| comp.value)
        .collect();
    values.sort();

    assert_eq!(values, vec![10, 20, 30]);
}

#[test]
fn test_entity_destroy_then_spawn_reuses_slot() {
    let mut world = World::new();

    let id1 = world.create_entity();
    let original_index = id1.index();

    world.destroy_entity(id1);

    let id2 = world.create_entity();

    // The new entity should reuse the same slot index
    assert_eq!(id2.index(), original_index);
    // But should have a different generation
    assert_ne!(id2.generation(), id1.generation());
}

#[test]
fn test_destroy_entity_returns_false_for_never_created() {
    let mut world = World::new();

    // Try to destroy an entity that was never created
    let fake_id = EntityId::test_new(999);
    assert!(!world.destroy_entity(fake_id));
}

#[test]
fn test_clear_entities_allows_reuse() {
    let mut world = World::new();

    for _ in 0..10 {
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 1 });
    }

    assert_eq!(world.entity_count(), 10);
    world.clear_entities();
    assert_eq!(world.entity_count(), 0);

    // Should be able to create new entities after clear
    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 99 });
    assert_eq!(world.entity_count(), 1);
    assert_eq!(world.get_component::<TestComponent>(id).unwrap().value, 99);
}
