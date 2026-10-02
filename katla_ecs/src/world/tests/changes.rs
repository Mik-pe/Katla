use super::*;

#[test]
fn test_add_component_marks_changed() {
    let mut world = World::new();

    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 42 });

    let changed: Vec<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    assert!(changed.contains(&id));
}

#[test]
fn test_get_component_mut_marks_changed() {
    let mut world = World::new();

    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 1 });

    // Clear change detection from the add
    world.clear_changed();

    // get_component_mut should mark as changed even without mutation
    let _comp = world.get_component_mut::<TestComponent>(id);

    let changed: Vec<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    assert!(changed.contains(&id));
}

#[test]
fn test_query_changed_returns_only_changed() {
    let mut world = World::new();

    let mut ids = Vec::new();
    for i in 0..10 {
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: i });
        ids.push(id);
    }

    // Clear change detection from adds
    world.clear_changed();

    // Mutably access only entities 2, 5, 7
    for idx in [2, 5, 7] {
        let _comp = world.get_component_mut::<TestComponent>(ids[idx]);
    }

    let changed: std::collections::HashSet<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    assert_eq!(changed.len(), 3);
    assert!(changed.contains(&ids[2]));
    assert!(changed.contains(&ids[5]));
    assert!(changed.contains(&ids[7]));
    assert!(!changed.contains(&ids[0]));
    assert!(!changed.contains(&ids[1]));
}

#[test]
fn test_clear_changed_resets_detection() {
    let mut world = World::new();

    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 42 });

    // Entity should be changed
    let changed: Vec<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();
    assert!(!changed.is_empty());

    // Clear should reset
    world.clear_changed();

    let changed: Vec<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();
    assert!(changed.is_empty());
}

#[test]
fn test_query_changed_is_subset() {
    let mut world = World::new();

    for i in 0..5 {
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: i });
    }

    let all: std::collections::HashSet<EntityId> = world
        .query::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    let changed: std::collections::HashSet<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    // Every changed entity must be in the full query set
    for id in &changed {
        assert!(all.contains(id), "query_changed entity not in query set");
    }
}

#[test]
fn test_immutable_get_no_change() {
    let mut world = World::new();

    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 1 });

    // Clear change detection from the add
    world.clear_changed();

    // Immutable get should NOT mark as changed
    let _comp = world.get_component::<TestComponent>(id);

    let changed: Vec<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    assert!(changed.is_empty());
}

#[test]
fn test_query_changed_multi_component() {
    #[derive(Component, Default)]
    struct CompA {
        _x: i32,
    }
    #[derive(Component, Default)]
    struct CompB {
        _y: f32,
    }

    let mut world = World::new();

    // Entity 1: both A and B
    let id1 = world.create_entity();
    world.add_component(id1, CompA::default());
    world.add_component(id1, CompB::default());

    // Entity 2: both A and B
    let id2 = world.create_entity();
    world.add_component(id2, CompA::default());
    world.add_component(id2, CompB::default());

    // Clear from adds
    world.clear_changed();

    // Mutate only A on entity1
    let _ = world.get_component_mut::<CompA>(id1);

    // Mutate only B on entity2
    let _ = world.get_component_mut::<CompB>(id2);

    // query_changed for (A, B) should return union (both entities)
    let changed: std::collections::HashSet<EntityId> = world
        .query_changed::<(&CompA, &CompB)>()
        .map(|(eid, _, _)| eid)
        .collect();

    assert_eq!(changed.len(), 2);
    assert!(changed.contains(&id1));
    assert!(changed.contains(&id2));
}

#[test]
fn test_destroyed_entity_excluded() {
    let mut world = World::new();

    let id1 = world.create_entity();
    world.add_component(id1, TestComponent { value: 1 });

    let id2 = world.create_entity();
    world.add_component(id2, TestComponent { value: 2 });

    // Mutate both
    let _ = world.get_component_mut::<TestComponent>(id1);
    let _ = world.get_component_mut::<TestComponent>(id2);

    // Destroy entity1
    world.destroy_entity(id1);

    // query_changed should not include destroyed entity
    let changed: Vec<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    assert!(!changed.contains(&id1));
    assert!(changed.contains(&id2));
}

#[test]
fn test_clear_changed_called_on_update() {
    let mut world = World::new();

    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 1 });

    // Should be changed
    let changed: Vec<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();
    assert!(!changed.is_empty());

    // update() calls clear_changed internally
    world.update(0.016);

    // Should no longer be changed
    let changed: Vec<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();
    assert!(changed.is_empty());
}
