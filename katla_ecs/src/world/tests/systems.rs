use super::*;

#[test]
fn test_parallel_matches_sequential() {
    #[derive(Component, Default)]
    struct CompA {
        value: i32,
    }
    #[derive(Component, Default)]
    struct CompB {
        value: i32,
    }

    struct WriteA;
    impl System for WriteA {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, a) in world.query::<&mut CompA>() {
                a.value = 42;
            }
        }
    }

    struct WriteB;
    impl System for WriteB {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, b) in world.query::<&mut CompB>() {
                b.value = 99;
            }
        }
    }

    // Build sequential world
    let mut world_seq = World::new();
    let e1 = world_seq.create_entity();
    world_seq.add_component(e1, CompA::default());
    world_seq.add_component(e1, CompB::default());

    world_seq.register_exclusive_system(Box::new(WriteA), SystemExecutionOrder::EARLY);
    world_seq.register_exclusive_system(Box::new(WriteB), SystemExecutionOrder::NORMAL);
    world_seq.update(0.016);

    let seq_a = world_seq.get_component::<CompA>(e1).unwrap().value;
    let seq_b = world_seq.get_component::<CompB>(e1).unwrap().value;

    let mut world_par = World::new();
    let e2 = world_par.create_entity();
    world_par.add_component(e2, CompA::default());
    world_par.add_component(e2, CompB::default());

    world_par.register_exclusive_system(Box::new(WriteA), SystemExecutionOrder::EARLY);
    world_par.register_exclusive_system(Box::new(WriteB), SystemExecutionOrder::NORMAL);
    world_par.update_parallel(0.016);

    assert_eq!(world_par.get_component::<CompA>(e2).unwrap().value, seq_a);
    assert_eq!(world_par.get_component::<CompB>(e2).unwrap().value, seq_b);
}

#[test]
fn test_parallel_conflicting_systems_run_sequentially() {
    #[derive(Component, Default)]
    struct Counter {
        value: i32,
    }

    struct IncrementSystem;
    impl System for IncrementSystem {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, c) in world.query::<&mut Counter>() {
                let prev = c.value;
                c.value = prev + 1;
            }
        }
    }

    let mut world = World::new();
    let e = world.create_entity();
    world.add_component(e, Counter { value: 0 });

    // Two systems that both write Counter — they conflict, so scheduler
    // must run them sequentially.
    world.register_exclusive_system(Box::new(IncrementSystem), SystemExecutionOrder::EARLY);
    world.register_exclusive_system(Box::new(IncrementSystem), SystemExecutionOrder::NORMAL);

    world.update_parallel(0.016);

    assert_eq!(world.get_component::<Counter>(e).unwrap().value, 2);
}

#[test]
fn test_parallel_disabled_systems_skipped() {
    #[derive(Component, Default)]
    struct Flag {
        set: bool,
    }

    struct SetFlagSystem;
    impl System for SetFlagSystem {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, f) in world.query::<&mut Flag>() {
                f.set = true;
            }
        }
    }

    struct DisabledSystem;
    impl System for DisabledSystem {
        fn update(&mut self, _world: &mut World, _dt: f32) {
            panic!("disabled system should not run");
        }
        fn is_enabled(&self) -> bool {
            false
        }
    }

    let mut world = World::new();
    let e = world.create_entity();
    world.add_component(e, Flag { set: false });

    world.register_exclusive_system(Box::new(SetFlagSystem), SystemExecutionOrder::EARLY);
    world.register_exclusive_system(Box::new(DisabledSystem), SystemExecutionOrder::NORMAL);

    world.update_parallel(0.016);

    assert!(world.get_component::<Flag>(e).unwrap().set);
}

#[test]
fn test_parallel_events_flushed_after_update() {
    #[derive(Component, Default)]
    struct DummyComp {
        _x: i32,
    }

    struct NoopSystem;
    impl System for NoopSystem {
        fn update(&mut self, _world: &mut World, _dt: f32) {}
    }

    let mut world = World::new();
    let e = world.create_entity();
    world.add_component(e, DummyComp::default());

    assert!(!world.entity_events().is_empty());
    assert!(!world.component_events().is_empty());

    world.register_exclusive_system(Box::new(NoopSystem), SystemExecutionOrder::NORMAL);

    world.update_parallel(0.016);

    assert!(world.entity_events().is_empty());
    assert!(world.component_events().is_empty());
}

#[test]
fn test_parallel_empty_world_no_panic() {
    let mut world = World::new();
    world.update_parallel(0.016);
}

#[test]
fn test_parallel_no_systems_no_panic() {
    let mut world = World::new();
    world.create_entity();
    world.update_parallel(0.016);
}

#[test]
fn test_parallel_scheduler_cache_invalidated_on_new_system() {
    #[derive(Component, Default)]
    struct CompA {
        value: i32,
    }
    #[derive(Component, Default)]
    struct CompB {
        value: i32,
    }

    struct WriteA;
    impl System for WriteA {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, a) in world.query::<&mut CompA>() {
                a.value += 10;
            }
        }
    }

    struct WriteB;
    impl System for WriteB {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, b) in world.query::<&mut CompB>() {
                b.value += 20;
            }
        }
    }

    let mut world = World::new();
    let e = world.create_entity();
    world.add_component(e, CompA::default());
    world.add_component(e, CompB::default());

    // First update with only WriteA
    world.register_exclusive_system(Box::new(WriteA), SystemExecutionOrder::EARLY);
    world.update_parallel(0.016);
    assert_eq!(world.get_component::<CompA>(e).unwrap().value, 10);

    // Add WriteB after first update — cache must be invalidated
    world.register_exclusive_system(Box::new(WriteB), SystemExecutionOrder::NORMAL);
    world.update_parallel(0.016);
    assert_eq!(world.get_component::<CompA>(e).unwrap().value, 20);
    assert_eq!(world.get_component::<CompB>(e).unwrap().value, 20);
}

#[test]
fn test_parallel_partial_read_overlap_correctness() {
    #[derive(Component, Default)]
    struct Input {
        value: i32,
    }
    #[derive(Component, Default)]
    struct OutputA {
        value: i32,
    }
    #[derive(Component, Default)]
    struct OutputB {
        value: i32,
    }

    struct ReadInputWriteA;
    impl System for ReadInputWriteA {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, inp, out) in world.query::<(&Input, &mut OutputA)>() {
                out.value = inp.value * 2;
            }
        }
    }

    struct ReadInputWriteB;
    impl System for ReadInputWriteB {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, inp, out) in world.query::<(&Input, &mut OutputB)>() {
                out.value = inp.value * 3;
            }
        }
    }

    let mut world = World::new();
    let e = world.create_entity();
    world.add_component(e, Input { value: 7 });
    world.add_component(e, OutputA::default());
    world.add_component(e, OutputB::default());

    world.register_exclusive_system(Box::new(ReadInputWriteA), SystemExecutionOrder::EARLY);
    world.register_exclusive_system(Box::new(ReadInputWriteB), SystemExecutionOrder::NORMAL);

    world.update_parallel(0.016);

    assert_eq!(world.get_component::<OutputA>(e).unwrap().value, 14);
    assert_eq!(world.get_component::<OutputB>(e).unwrap().value, 21);
}

#[test]
fn test_parallel_wide_fan_out_correctness() {
    #[derive(Component, Default)]
    struct Source {
        value: i32,
    }
    #[derive(Component, Default)]
    struct Sink {
        count: i32,
    }

    struct ReadSourceWriteSink;
    impl System for ReadSourceWriteSink {
        fn update(&mut self, world: &mut World, _dt: f32) {
            let src = world
                .query::<&Source>()
                .next()
                .map(|(_, s)| s.value)
                .unwrap_or(0);
            for (_id, sink) in world.query::<&mut Sink>() {
                sink.count += src;
            }
        }
    }

    let mut world = World::new();
    let e_src = world.create_entity();
    world.add_component(e_src, Source { value: 5 });

    let mut sinks = Vec::new();
    for _ in 0..5 {
        let e = world.create_entity();
        world.add_component(e, Sink::default());
        sinks.push(e);
    }

    // Register 5 systems that all read Source and write Sink
    for _ in 0..5 {
        world
            .register_exclusive_system(Box::new(ReadSourceWriteSink), SystemExecutionOrder::NORMAL);
    }

    world.update_parallel(0.016);

    // Each system ran sequentially because they all write Sink
    for sink_e in &sinks {
        assert_eq!(world.get_component::<Sink>(*sink_e).unwrap().count, 25);
    }
}

#[test]
fn test_parallel_multi_level_dag_correctness() {
    #[derive(Component, Default)]
    struct Level0 {
        value: i32,
    }
    #[derive(Component, Default)]
    struct Level1 {
        value: i32,
    }
    #[derive(Component, Default)]
    struct Level2 {
        value: i32,
    }

    struct InitSystem;
    impl System for InitSystem {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, l0) in world.query::<&mut Level0>() {
                l0.value = 10;
            }
        }
    }

    struct TransformSystem;
    impl System for TransformSystem {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, l0, l1) in world.query::<(&Level0, &mut Level1)>() {
                l1.value = l0.value + 5;
            }
        }
    }

    struct FinalizeSystem;
    impl System for FinalizeSystem {
        fn update(&mut self, world: &mut World, _dt: f32) {
            for (_id, l1, l2) in world.query::<(&Level1, &mut Level2)>() {
                l2.value = l1.value * 2;
            }
        }
    }

    let mut world = World::new();
    let e = world.create_entity();
    world.add_component(e, Level0::default());
    world.add_component(e, Level1::default());
    world.add_component(e, Level2::default());

    world.register_exclusive_system(Box::new(InitSystem), SystemExecutionOrder::EARLY);
    world.register_exclusive_system(Box::new(TransformSystem), SystemExecutionOrder::NORMAL);
    world.register_exclusive_system(Box::new(FinalizeSystem), SystemExecutionOrder::LATE);

    world.update_parallel(0.016);

    assert_eq!(world.get_component::<Level0>(e).unwrap().value, 10);
    assert_eq!(world.get_component::<Level1>(e).unwrap().value, 15);
    assert_eq!(world.get_component::<Level2>(e).unwrap().value, 30);
}
