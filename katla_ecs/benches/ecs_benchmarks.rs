use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use katla_ecs::{Component, EntityId, World};

#[derive(Component, Default)]
#[allow(dead_code)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Component, Default)]
#[allow(dead_code)]
struct Velocity {
    dx: f32,
    dy: f32,
    dz: f32,
}

#[derive(Component, Default)]
struct Health(f32);

#[derive(Component, Default)]
struct Tag;

fn spawn_entities(world: &mut World, count: usize, with_four: bool) -> Vec<EntityId> {
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let id = if with_four {
            world.spawn((Position::default(), Velocity::default(), Health(100.0), Tag))
        } else {
            world.spawn((Position::default(),))
        };
        ids.push(id);
    }
    ids
}

fn spawn_2component_entities(world: &mut World, count: usize) -> Vec<EntityId> {
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let id = world.spawn((Position::default(), Velocity::default()));
        ids.push(id);
    }
    ids
}

fn spawn_4component_entities(world: &mut World, count: usize) -> Vec<EntityId> {
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let id = world.spawn((Position::default(), Velocity::default(), Health(100.0), Tag));
        ids.push(id);
    }
    ids
}

fn bench_spawn(c: &mut Criterion) {
    let mut group = c.benchmark_group("spawn");
    for size in [1_000, 10_000, 100_000] {
        group.bench_with_input(BenchmarkId::new("1_component", size), &size, |b, &size| {
            b.iter(|| {
                let mut world = World::new();
                for _ in 0..size {
                    black_box(world.spawn((Position::default(),)));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("2_component", size), &size, |b, &size| {
            b.iter(|| {
                let mut world = World::new();
                for _ in 0..size {
                    black_box(world.spawn((Position::default(), Velocity::default())));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("4_component", size), &size, |b, &size| {
            b.iter(|| {
                let mut world = World::new();
                for _ in 0..size {
                    black_box(world.spawn((
                        Position::default(),
                        Velocity::default(),
                        Health(100.0),
                        Tag,
                    )));
                }
            });
        });
    }
    group.finish();
}

fn bench_query_1_component(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_1_component");
    for size in [1_000, 10_000, 100_000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let mut world = World::new();
            spawn_entities(&mut world, size, false);
            b.iter(|| {
                for (_id, pos) in world.query::<&Position>() {
                    black_box(pos.x);
                }
            });
        });
    }
    group.finish();
}

fn bench_query_2_component(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_2_component");
    for size in [1_000, 10_000, 100_000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let mut world = World::new();
            spawn_2component_entities(&mut world, size);
            b.iter(|| {
                for (_id, pos, vel) in world.query::<(&Position, &Velocity)>() {
                    black_box((pos.x, vel.dx));
                }
            });
        });
    }
    group.finish();
}

fn bench_query_4_component(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_4_component");
    for size in [1_000, 10_000, 100_000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let mut world = World::new();
            spawn_4component_entities(&mut world, size);
            b.iter(|| {
                for (_id, pos, vel, hp, _tag) in
                    world.query::<(&Position, &Velocity, &Health, &Tag)>()
                {
                    black_box((pos.x, vel.dx, hp.0));
                }
            });
        });
    }
    group.finish();
}

fn bench_query_mut_1_component(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_mut_1_component");
    for size in [1_000, 10_000, 100_000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let mut world = World::new();
            spawn_entities(&mut world, size, false);
            b.iter(|| {
                for (_id, pos) in world.query::<&mut Position>() {
                    pos.x += 1.0;
                }
            });
        });
    }
    group.finish();
}

fn bench_get_component(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_component");
    for size in [1_000, 10_000, 100_000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let mut world = World::new();
            let ids = spawn_entities(&mut world, size, true);
            let target = ids[size / 2];
            b.iter(|| {
                if let Some(pos) = world.get_component::<Position>(target) {
                    black_box(pos.x);
                }
            });
        });
    }
    group.finish();
}

fn bench_add_remove_component(c: &mut Criterion) {
    let mut group = c.benchmark_group("add_remove_component");
    for size in [1_000, 10_000, 100_000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let mut world = World::new();
            let ids = spawn_entities(&mut world, size, false);
            b.iter(|| {
                for &id in &ids {
                    world.add_component(id, Velocity::default());
                }
                for &id in &ids {
                    world.remove_component::<Velocity>(id);
                }
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_spawn,
    bench_query_1_component,
    bench_query_2_component,
    bench_query_4_component,
    bench_query_mut_1_component,
    bench_get_component,
    bench_add_remove_component,
);
criterion_main!(benches);
