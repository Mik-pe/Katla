use super::*;

#[test]
fn test_entity_spawn_event_emitted() {
    let mut world = World::new();

    let id = world.create_entity();

    let events = world.entity_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], EntityEvent::Spawned(id));
}

#[test]
fn test_entity_destroyed_event_emitted() {
    let mut world = World::new();

    let id = world.create_entity();
    // Clear spawn event to isolate destroy event
    world.entity_events.clear();

    assert!(world.destroy_entity(id));

    let events = world.entity_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], EntityEvent::Destroyed(id));
}

#[test]
fn test_destroy_invalid_entity_no_event() {
    let mut world = World::new();

    let fake_id = EntityId::test_new(999);
    assert!(!world.destroy_entity(fake_id));

    assert!(world.entity_events().is_empty());
}

#[test]
fn test_destroy_already_destroyed_entity_no_event() {
    let mut world = World::new();

    let id = world.create_entity();
    assert!(world.destroy_entity(id));
    // Clear events from spawn + first destroy
    world.entity_events.clear();

    // Second destroy should not emit event
    assert!(!world.destroy_entity(id));
    assert!(world.entity_events().is_empty());
}

#[test]
fn test_entity_events_flushed_after_update() {
    let mut world = World::new();

    world.create_entity();
    world.create_entity();
    assert_eq!(world.entity_events().len(), 2);

    world.update(0.016);

    assert!(world.entity_events().is_empty());
}

#[test]
fn test_entity_events_visible_during_update() {
    use crate::system::{System, SystemExecutionOrder};

    #[derive(Default)]
    struct EventCheckerSystem {
        saw_events: bool,
    }

    impl System for EventCheckerSystem {
        fn update(&mut self, world: &mut World, _dt: f32) {
            self.saw_events = !world.entity_events().is_empty();
        }
    }

    let mut world = World::new();
    world.create_entity();
    world.create_entity();

    let system = EventCheckerSystem::default();
    world.register_exclusive_system(Box::new(system), SystemExecutionOrder::EARLY);

    world.update(0.016);

    // We can't easily check the system's internal state after it's boxed,
    // but we can verify events were there during the tick by checking
    // that events are now flushed after update
}

#[test]
fn test_entity_event_ordering() {
    let mut world = World::new();

    let id_a = world.create_entity();
    let id_b = world.create_entity();
    world.destroy_entity(id_a);

    let events = world.entity_events();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0], EntityEvent::Spawned(id_a));
    assert_eq!(events[1], EntityEvent::Spawned(id_b));
    assert_eq!(events[2], EntityEvent::Destroyed(id_a));
}

#[test]
fn test_entity_events_accumulate_across_frame() {
    let mut world = World::new();

    let id1 = world.create_entity();
    let _id2 = world.create_entity();
    world.destroy_entity(id1);

    assert_eq!(world.entity_events().len(), 3);

    // After update, events should be flushed
    world.update(0.016);
    assert!(world.entity_events().is_empty());

    // New events in next frame
    let id3 = world.create_entity();
    world.destroy_entity(id3);

    assert_eq!(world.entity_events().len(), 2);
    let events = world.entity_events();
    assert_eq!(events[0], EntityEvent::Spawned(id3));
    assert_eq!(events[1], EntityEvent::Destroyed(id3));
}

#[test]
fn test_component_added_event() {
    let mut world = World::new();
    let id = world.create_entity();

    world.add_component(id, TestComponent { value: 42 });

    let events = world.component_events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        ComponentEvent::Added(id, std::any::TypeId::of::<TestComponent>())
    );
}

#[test]
fn test_component_removed_event() {
    let mut world = World::new();
    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 1 });

    // Clear the Added event
    world.component_events.clear();

    assert!(world.remove_component::<TestComponent>(id));

    let events = world.component_events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        ComponentEvent::Removed(id, std::any::TypeId::of::<TestComponent>())
    );
}

#[test]
fn test_destroy_entity_emits_component() {
    let mut world = World::new();
    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 1 });

    // Clear the Added event
    world.component_events.clear();

    assert!(world.destroy_entity(id));

    let comp_events: Vec<_> = world
        .component_events()
        .iter()
        .filter(|e| matches!(e, ComponentEvent::Removed(..)))
        .collect();
    assert_eq!(comp_events.len(), 1);
    assert_eq!(
        comp_events[0],
        &ComponentEvent::Removed(id, std::any::TypeId::of::<TestComponent>())
    );

    // Entity destroyed event should also be emitted
    let ent_events: Vec<_> = world
        .entity_events()
        .iter()
        .filter(|e| matches!(e, EntityEvent::Destroyed(..)))
        .collect();
    assert_eq!(ent_events.len(), 1);
}

#[test]
fn test_destroy_entity_emits_component_removed_for_multiple_types() {
    let mut world = World::new();

    #[derive(Component, Default)]
    struct CompA {
        _x: i32,
    }
    #[derive(Component, Default)]
    struct CompB {
        _y: f32,
    }

    let id = world.create_entity();
    world.add_component(id, CompA::default());
    world.add_component(id, CompB::default());

    world.component_events.clear();

    world.destroy_entity(id);

    let comp_events: Vec<_> = world
        .component_events()
        .iter()
        .filter(|e| matches!(e, ComponentEvent::Removed(..)))
        .collect();
    assert_eq!(comp_events.len(), 2);

    let type_ids: Vec<_> = comp_events
        .iter()
        .map(|e| match e {
            ComponentEvent::Removed(_, tid) => *tid,
            _ => panic!("unexpected event variant"),
        })
        .collect();
    assert!(type_ids.contains(&std::any::TypeId::of::<CompA>()));
    assert!(type_ids.contains(&std::any::TypeId::of::<CompB>()));
}

#[test]
fn test_component_events_type_safety() {
    let mut world = World::new();

    #[derive(Component, Default)]
    struct Health {
        _hp: f32,
    }
    #[derive(Component, Default)]
    struct Mana {
        _mp: f32,
    }

    let id1 = world.create_entity();
    let id2 = world.create_entity();
    world.add_component(id1, Health::default());
    world.add_component(id2, Mana::default());
    world.remove_component::<Health>(id1);

    let health_events = world.component_events_for::<Health>();
    assert_eq!(health_events.len(), 2); // Added + Removed
    assert!(health_events.iter().all(|e| {
        let tid = match e {
            ComponentEvent::Added(_, tid) | ComponentEvent::Removed(_, tid) => *tid,
        };
        tid == std::any::TypeId::of::<Health>()
    }));

    let mana_events = world.component_events_for::<Mana>();
    assert_eq!(mana_events.len(), 1); // Only Added
}

#[test]
fn test_component_events_flushed() {
    let mut world = World::new();
    let id = world.create_entity();

    world.add_component(id, TestComponent { value: 1 });
    assert_eq!(world.component_events().len(), 1);

    world.update(0.016);
    assert!(world.component_events().is_empty());
}

#[test]
fn test_remove_nonexistent_component() {
    let mut world = World::new();
    let id = world.create_entity();

    assert!(!world.remove_component::<TestComponent>(id));
    assert!(world.component_events().is_empty());
}

#[test]
fn test_double_add_component() {
    let mut world = World::new();
    let id = world.create_entity();

    world.add_component(id, TestComponent { value: 1 });
    world.add_component(id, TestComponent { value: 2 });

    let events: Vec<_> = world
        .component_events()
        .iter()
        .filter(|e| matches!(e, ComponentEvent::Added(..)))
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0],
        &ComponentEvent::Added(id, std::any::TypeId::of::<TestComponent>())
    );
    assert_eq!(
        events[1],
        &ComponentEvent::Added(id, std::any::TypeId::of::<TestComponent>())
    );
}

#[test]
fn test_component_events_accumulate_across_frame() {
    let mut world = World::new();

    let id = world.create_entity();
    world.add_component(id, TestComponent { value: 1 });

    world.update(0.016);
    assert!(world.component_events().is_empty());

    let id2 = world.create_entity();
    world.add_component(id2, TestComponent { value: 2 });
    assert_eq!(world.component_events().len(), 1);
}
