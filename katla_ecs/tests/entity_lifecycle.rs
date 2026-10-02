use katla_ecs::{Component, ComponentEvent, EntityEvent, World};

#[derive(Component, Debug, PartialEq)]
struct Value(u32);

#[test]
fn test_stale_entity_cannot_remove_reused_components_or_emit_events() {
    let mut world = World::new();
    let stale = world.spawn((Value(1),));
    assert!(world.destroy_entity(stale));
    let live = world.spawn((Value(2),));
    let before = world.component_events().len();
    assert!(!world.remove_component::<Value>(stale));
    assert!(!world.destroy_entity(stale));
    assert!(world.get_component::<Value>(stale).is_none());
    assert!(world.get_component_mut::<Value>(stale).is_none());
    world.add_component(stale, Value(3));
    assert_eq!(world.component_events().len(), before);
    assert_eq!(world.get_component::<Value>(live), Some(&Value(2)));
    assert!(
        world
            .component_events()
            .iter()
            .any(|event| { matches!(event, ComponentEvent::Removed(id, _) if *id == stale) })
    );
    assert_eq!(
        world
            .entity_events()
            .iter()
            .filter(|event| { matches!(event, EntityEvent::Destroyed(id) if *id == stale) })
            .count(),
        1
    );
}

#[test]
fn test_clear_permanently_invalidates_pre_clear_ids() {
    let mut world = World::new();
    let mut stale = Vec::new();
    for generation in 0..100 {
        stale.extend((0..32).map(|_| world.spawn((Value(generation),))));
        world.clear_entities();
        let live = world.spawn((Value(generation + 1),));
        assert!(stale.iter().all(|id| !world.entity_exists(*id)));
        assert!(
            stale
                .iter()
                .all(|id| world.get_component::<Value>(*id).is_none())
        );
        assert_eq!(
            world.get_component::<Value>(live),
            Some(&Value(generation + 1))
        );
        assert_eq!(world.entity_count(), 1);
        world.clear_entities();
    }
}
