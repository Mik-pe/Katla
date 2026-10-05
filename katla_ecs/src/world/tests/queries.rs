use super::*;

#[test]
fn test_query_matches_manual_loop() {
    let mut world = World::new();

    // Create entities: some with TestComponent, some without
    let with_component: Vec<EntityId> = (0..5)
        .map(|i| {
            let id = world.create_entity();
            world.add_component(id, TestComponent { value: i });
            id
        })
        .collect();

    // Create entities without the component
    for _ in 0..3 {
        world.create_entity();
    }

    // Collect via query
    let query_ids: std::collections::HashSet<EntityId> = world
        .query::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    // Collect via manual entity_ids loop
    let manual_ids: std::collections::HashSet<EntityId> = world
        .entity_ids()
        .filter(|id| world.get_component::<TestComponent>(*id).is_some())
        .collect();

    assert_eq!(query_ids, manual_ids);
    assert_eq!(query_ids.len(), 5);
    for id in &with_component {
        assert!(query_ids.contains(id));
    }
}

#[test]
fn test_query_mut_propagates() {
    let mut world = World::new();

    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 10 });

    // Mutate via query
    for (_entity, comp) in world.query::<&mut TestComponent>() {
        comp.value = 42;
    }

    // Verify mutation is visible via get_component
    let comp = world.get_component::<TestComponent>(id).unwrap();
    assert_eq!(comp.value, 42);
}

#[test]
fn test_query_empty_world() {
    let mut world = World::new();

    let results: Vec<EntityId> = world
        .query::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    assert!(results.is_empty());
}

#[test]
fn test_query_filters_missing_components() {
    #[derive(Component, Default)]
    struct CompA {
        _x: i32,
    }
    #[derive(Component, Default)]
    struct CompB {
        _y: f32,
    }

    let mut world = World::new();

    // Entity with both A and B
    let id_both = world.create_entity();
    world.add_component(id_both, CompA::default());
    world.add_component(id_both, CompB::default());

    // Entity with only A
    let id_only_a = world.create_entity();
    world.add_component(id_only_a, CompA::default());

    // Entity with only B
    let id_only_b = world.create_entity();
    world.add_component(id_only_b, CompB::default());

    // Entity with neither
    let id_neither = world.create_entity();

    // Query for (A, B) should only return entity with both
    let results: std::collections::HashSet<EntityId> = world
        .query::<(&CompA, &CompB)>()
        .map(|(eid, _, _)| eid)
        .collect();

    assert_eq!(results.len(), 1);
    assert!(results.contains(&id_both));
    assert!(!results.contains(&id_only_a));
    assert!(!results.contains(&id_only_b));
    assert!(!results.contains(&id_neither));
}

#[test]
fn test_query_ref_matches_query() {
    let mut world = World::new();

    for i in 0..5 {
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: i });
    }

    // query and query_ref should produce same entity set
    let mut_query: std::collections::HashSet<EntityId> = world
        .query::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    let ref_query: std::collections::HashSet<EntityId> = world
        .query_ref::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    assert_eq!(mut_query, ref_query);
}

#[test]
fn test_query_ref_immutable_single() {
    let mut world = World::new();

    let id1 = world.create_entity();
    world.add_component(id1, TestComponent { value: 10 });
    let id2 = world.create_entity();
    world.add_component(id2, TestComponent { value: 20 });

    let world_ref: &World = &world;
    let mut values: Vec<i32> = world_ref
        .query_ref::<&TestComponent>()
        .map(|(_, comp)| comp.value)
        .collect();
    values.sort();

    assert_eq!(values, vec![10, 20]);
}

#[test]
fn test_query_ref_immutable_tuple() {
    #[derive(Component, Default)]
    struct CompA {
        x: i32,
    }
    #[derive(Component, Default)]
    struct CompB {
        y: f32,
    }

    let mut world = World::new();

    let id_both = world.create_entity();
    world.add_component(id_both, CompA { x: 42 });
    world.add_component(
        id_both,
        CompB {
            y: std::f32::consts::PI,
        },
    );

    let id_only_a = world.create_entity();
    world.add_component(id_only_a, CompA { x: 99 });

    let world_ref: &World = &world;
    let results: Vec<(EntityId, i32, f32)> = world_ref
        .query_ref::<(&CompA, &CompB)>()
        .map(|(eid, a, b)| (eid, a.x, b.y))
        .collect();

    assert_eq!(results.len(), 1);
    let (eid, x, y) = results[0];
    assert_eq!(eid, id_both);
    assert_eq!(x, 42);
    assert!((y - std::f32::consts::PI).abs() < 1e-6);
}
