use super::*;

#[test]
fn test_bulk_spawn_emits_ordered_lifecycle_events() {
    let mut world = World::new();
    let ids: Vec<_> = (0..1025)
        .map(|value| world.spawn((TestComponent { value },)))
        .collect();
    let spawned: Vec<_> = world
        .entity_events()
        .iter()
        .map(|event| match event {
            EntityEvent::Spawned(id) => *id,
            EntityEvent::Destroyed(_) => panic!("spawn emitted destruction"),
        })
        .collect();
    assert_eq!(spawned, ids);
    let added: Vec<_> = world
        .component_events()
        .iter()
        .map(|event| match event {
            ComponentEvent::Added(id, ty) => {
                assert_eq!(*ty, std::any::TypeId::of::<TestComponent>());
                *id
            }
            ComponentEvent::Removed(..) => panic!("spawn emitted removal"),
        })
        .collect();
    assert_eq!(added, ids);
    for (value, id) in ids.into_iter().enumerate() {
        assert_eq!(
            world.get_component::<TestComponent>(id).unwrap().value,
            value as i32
        );
    }
    assert!(world.validate().is_ok());
}

#[test]
fn test_bulk_mutable_access_marks_each_entity_once() {
    let mut world = World::new();
    let ids: Vec<_> = (0..1025)
        .map(|_| world.spawn((TestComponent::default(),)))
        .collect();
    world.clear_changed();
    for _ in 0..2 {
        for &id in &ids {
            world.get_component_mut::<TestComponent>(id).unwrap().value += 1;
        }
    }
    let changed: Vec<_> = world
        .query_changed::<&TestComponent>()
        .map(|(id, value)| {
            assert_eq!(value.value, 2);
            id
        })
        .collect();
    assert_eq!(changed, ids);
    world.clear_changed();
    assert_eq!(world.query_changed::<&TestComponent>().count(), 0);
    let selected = ids[1024];
    world
        .get_component_mut::<TestComponent>(selected)
        .unwrap()
        .value = 3;
    let changed: Vec<_> = world
        .query_changed::<&TestComponent>()
        .map(|(id, value)| (id, value.value))
        .collect();
    assert_eq!(changed, [(selected, 3)]);
}

#[test]
fn test_change_detection_only_dirty_entities() {
    // VAL-ECS-006: With many entities but few changed, only changed entities are returned.
    let mut world = World::new();

    // Create 1000 entities with components
    let mut ids = Vec::with_capacity(1000);
    for i in 0..1000 {
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: i });
        ids.push(id);
    }

    // Clear change detection from adds
    world.clear_changed();

    // Mutably access only 3 specific entities
    let _ = world.get_component_mut::<TestComponent>(ids[10]);
    let _ = world.get_component_mut::<TestComponent>(ids[500]);
    let _ = world.get_component_mut::<TestComponent>(ids[999]);

    let changed: std::collections::HashSet<EntityId> = world
        .query_changed::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    // Should return exactly the 3 dirty entities
    assert_eq!(changed.len(), 3);
    assert!(changed.contains(&ids[10]));
    assert!(changed.contains(&ids[500]));
    assert!(changed.contains(&ids[999]));

    // Verify no other entities leaked in
    for (i, id) in ids.iter().enumerate() {
        if i != 10 && i != 500 && i != 999 {
            assert!(!changed.contains(id));
        }
    }
}
