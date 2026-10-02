use super::*;

#[test]
fn test_par_query_single_component() {
    use rayon::iter::ParallelIterator;

    let mut world = World::new();
    let mut ids = Vec::new();
    for i in 0..100 {
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: i });
        ids.push(id);
    }

    let results: std::collections::HashSet<EntityId> = world
        .par_query::<&TestComponent>()
        .map(|(eid, _)| eid)
        .collect();

    assert_eq!(results.len(), 100);
    for id in &ids {
        assert!(results.contains(id));
    }
}

#[test]
fn test_par_query_two_components() {
    use rayon::iter::ParallelIterator;

    #[derive(Component, Default)]
    struct Velocity {
        dx: f32,
        dy: f32,
    }

    let mut world = World::new();
    let mut ids = Vec::new();
    for i in 0..100 {
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: i });
        world.add_component(
            id,
            Velocity {
                dx: i as f32,
                dy: 0.0,
            },
        );
        ids.push(id);
    }

    let results: std::collections::HashSet<EntityId> = world
        .par_query::<(&TestComponent, &Velocity)>()
        .map(|(eid, _, vel)| {
            let _ = (vel.dx, vel.dy);
            eid
        })
        .collect();

    assert_eq!(results.len(), 100);
    for id in &ids {
        assert!(results.contains(id));
    }
}

#[test]
fn test_par_query_matches_sequential() {
    use rayon::iter::ParallelIterator;

    #[derive(Component, Default)]
    struct Velocity {
        dx: f32,
    }

    let mut world = World::new();

    // Spawn 10K entities with (TestComponent, Velocity)
    for i in 0..10_000 {
        world.spawn((TestComponent { value: i }, Velocity { dx: i as f32 * 0.1 }));
    }

    // Sequential results
    let mut seq: Vec<(EntityId, i32, f32)> = world
        .query::<(&TestComponent, &Velocity)>()
        .map(|(id, t, v)| (id, t.value, v.dx))
        .collect();
    seq.sort_by_key(|(id, _, _)| id.id());

    // Parallel results
    let mut par: Vec<(EntityId, i32, f32)> = world
        .par_query::<(&TestComponent, &Velocity)>()
        .map(|(id, t, v)| (id, t.value, v.dx))
        .collect();
    par.sort_by_key(|(id, _, _)| id.id());

    assert_eq!(seq.len(), par.len());
    assert_eq!(seq, par);
}

#[test]
fn test_par_query_empty_world() {
    use rayon::iter::ParallelIterator;

    let world = World::new();
    assert_eq!(world.par_query::<&TestComponent>().count(), 0);
}

#[test]
fn test_par_query_filters_missing() {
    use rayon::iter::ParallelIterator;

    #[derive(Component, Default)]
    struct OnlySome;

    let mut world = World::new();

    let id_both = world.create_entity();
    world.add_component(id_both, TestComponent { value: 1 });
    world.add_component(id_both, OnlySome);

    let id_only_tc = world.create_entity();
    world.add_component(id_only_tc, TestComponent { value: 2 });

    let id_only_os = world.create_entity();
    world.add_component(id_only_os, OnlySome);

    let results: std::collections::HashSet<EntityId> = world
        .par_query::<(&TestComponent, &OnlySome)>()
        .map(|(id, _, _)| id)
        .collect();

    assert_eq!(results.len(), 1);
    assert!(results.contains(&id_both));
}
