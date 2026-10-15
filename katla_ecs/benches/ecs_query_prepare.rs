//! CPU-only query preparation and cache-rebuild costs, using stable public APIs.

use katla_ecs::{
    Component, Query, QueryDescriptor, Read, SystemExecutionOrder, SystemParam, TypedSystem, World,
};
use std::{
    hint::black_box,
    marker::PhantomData,
    time::{Duration, Instant},
};

#[derive(Component)]
struct Position([f32; 3]);
#[derive(Component)]
struct Velocity([f32; 3]);
#[derive(Component)]
struct Health(f32);
#[derive(Component)]
struct Tag;
#[derive(Component)]
struct EpochMarker;

type Query2 = (Read<Position>, Read<Velocity>);
type Query4 = (Read<Position>, Read<Velocity>, Read<Health>, Read<Tag>);

struct Count<D: QueryDescriptor>(PhantomData<fn() -> D>);
impl<D: QueryDescriptor> TypedSystem for Count<D> {
    type Params = Query<D>;

    fn run(&mut self, query: <Self::Params as SystemParam>::Item<'_>, _: f32) {
        black_box(query.len());
    }
}

fn measure(name: &str, size: usize, matches: usize, mut operation: impl FnMut() -> f64) {
    let budget = Duration::from_millis(
        std::env::var("KATLA_BENCH_SAMPLE_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(100),
    );
    for _ in 0..3 {
        black_box(operation());
    }
    for sample in 0..7 {
        let start = Instant::now();
        let mut iterations = 0;
        let mut checksum = 0.0;
        while start.elapsed() < budget {
            checksum += black_box(operation());
            iterations += 1;
        }
        let ns = start.elapsed().as_secs_f64() * 1e9 / iterations as f64;
        println!("{name},{size},1,{matches},{sample},{iterations},{ns:.3},{checksum:.6}");
    }
}

fn run<D: QueryDescriptor>(name: &str, size: usize, tag_stride: usize, matches: usize) {
    let mut world = World::new();
    let mut marker = None;
    for index in 0..size {
        let x = index as f32;
        let id = world.spawn((Position([x; 3]), Velocity([1.0; 3]), Health(100.0)));
        if index % tag_stride == 0 {
            world.add_component(id, Tag);
        }
        marker = Some(id);
    }
    // Validate actual component contents as well as the query's match count.
    assert_eq!(
        world
            .query_ref::<(&Position, &Velocity, &Health)>()
            .map(|(_, position, velocity, health)| {
                f64::from(position.0[0] + velocity.0[0] + health.0)
            })
            .sum::<f64>(),
        (0..size).map(|x| (x + 101) as f64).sum::<f64>()
    );
    assert_eq!(world.query_typed::<D, ()>().len(), matches);
    world.update(0.0);
    measure(&format!("typed_fresh_{name}"), size, matches, || {
        black_box(&mut world).query_typed::<D, ()>().len() as f64
    });

    world.register_typed_system(Count::<D>(PhantomData), SystemExecutionOrder::NORMAL);
    world.update(0.0);
    measure(&format!("typed_steady_frame_{name}"), size, matches, || {
        black_box(&mut world).update(0.0);
        matches as f64
    });
    if let Some(marker) = marker {
        measure(
            &format!("typed_rebuild_frame_{name}"),
            size,
            matches,
            || {
                // Replacement still advances the structural epoch and emits lifecycle
                // events, without changing the query's component membership.
                world.add_component(marker, EpochMarker);
                black_box(&mut world).update(0.0);
                matches as f64
            },
        );
    }
}

fn main() {
    println!("scenario,entities,threads,matches,sample,iterations,ns_per_iteration,checksum");
    for size in [1_000, 10_000, 100_000] {
        run::<Query2>("query2_dense", size, 1, size);
        run::<Query4>("query4_dense", size, 1, size);
        run::<Query4>("query4_one_percent", size, 100, size.div_ceil(100));
    }
}
