//! CPU-only reproducible storage and scheduling comparisons; see docs/ecs_benchmarks.md.
use katla_ecs::{Component, EntityId, World};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

#[path = "comparison/archetype.rs"]
mod archetype;
#[path = "comparison/scheduling.rs"]
mod scheduling;
#[cfg(not(katla_ecs_baseline))]
#[path = "comparison/typed.rs"]
mod typed;

#[derive(Component, Clone, Copy)]
struct Position([f32; 3]);
#[derive(Component, Clone, Copy)]
struct Velocity([f32; 3]);
#[derive(Component, Clone, Copy)]
struct Health(f32);
#[derive(Component)]
struct Tag;
#[derive(Component)]
struct Cold([u64; 32]);

fn components(id: usize) -> (Position, Velocity, Health) {
    let x = (id % 1024) as f32;
    (
        Position([x, x + 1.0, x + 2.0]),
        Velocity([0.5, 1.0, 1.5]),
        Health(100.0),
    )
}

fn score2(p: &Position, v: &Velocity) -> f64 {
    (p.0[0] + p.0[1] + p.0[2] + v.0[0] + v.0[1] + v.0[2]) as f64
}

fn sparse_world(count: usize, tag_stride: usize) -> (World, Vec<EntityId>) {
    let mut world = World::new();
    let mut ids = Vec::with_capacity(count);
    for id in 0..count {
        let (p, v, h) = components(id);
        let entity = world.spawn((p, v, h));
        if id % tag_stride == 0 {
            world.add_component(entity, Tag);
        }
        ids.push(entity);
    }
    (world, ids)
}

const SEED: u64 = 0x4b41544c41313338;
fn shuffled(count: usize) -> Vec<usize> {
    let mut ids: Vec<_> = (1..count).collect();
    let mut seed = SEED;
    for index in (1..ids.len()).rev() {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ids.swap(index, seed as usize % (index + 1));
    }
    ids
}

fn measure(name: &str, size: usize, threads: usize, matches: usize, mut f: impl FnMut() -> f64) {
    let budget_ms = std::env::var("KATLA_BENCH_SAMPLE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let budget = Duration::from_millis(budget_ms);
    for _ in 0..3 {
        black_box(f());
    }
    let mut results = Vec::new();
    for sample in 0..7 {
        let start = Instant::now();
        let mut iterations = 0;
        let mut checksum = 0.0;
        while start.elapsed() < budget {
            checksum += black_box(f());
            iterations += 1;
        }
        let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
        println!("{name},{size},{threads},{matches},{sample},{iterations},{ns:.3},{checksum:.3}");
        results.push(ns);
    }
    results.sort_by(f64::total_cmp);
    eprintln!(
        "{name}/{size}/{threads}: median {:.3} us ({:.3}..{:.3})",
        results[3] / 1000.0,
        results[0] / 1000.0,
        results[6] / 1000.0
    );
}

fn storage() {
    for size in [1_000, 10_000, 100_000] {
        for stride in [1, 100] {
            let (mut world, _) = sparse_world(size, stride);
            let tables = archetype::Archetypes::new(size, stride);
            let expected = tables.query2();
            assert_eq!(
                expected,
                world
                    .query::<(&Position, &Velocity)>()
                    .map(|(_, p, v)| score2(p, v))
                    .sum::<f64>()
            );
            assert_eq!(
                tables.query4(),
                world
                    .query::<(&Position, &Velocity, &Health, &Tag)>()
                    .map(|(_, p, v, h, _)| score2(p, v) + h.0 as f64)
                    .sum::<f64>()
            );
            let suffix = if stride == 1 { "dense" } else { "one_percent" };
            measure(&format!("sparse_query2_{suffix}"), size, 1, size, || {
                black_box(&mut world)
                    .query::<(&Position, &Velocity)>()
                    .map(|(_, p, v)| score2(p, v))
                    .sum()
            });
            measure(&format!("archetype_query2_{suffix}"), size, 1, size, || {
                black_box(&tables).query2()
            });
            let matches = size.div_ceil(stride);
            measure(&format!("sparse_query4_{suffix}"), size, 1, matches, || {
                black_box(&mut world)
                    .query::<(&Position, &Velocity, &Health, &Tag)>()
                    .map(|(_, p, v, h, _)| score2(p, v) + h.0 as f64)
                    .sum()
            });
            measure(
                &format!("archetype_query4_{suffix}"),
                size,
                1,
                matches,
                || black_box(&tables).query4(),
            );
        }
        let (mut world, entities) = sparse_world(size, usize::MAX);
        let mut tables = archetype::Archetypes::new(size, usize::MAX);
        let ids = shuffled(size);
        measure("sparse_churn_tag_roundtrip", size, 1, ids.len(), || {
            for &id in &ids {
                world.add_component(entities[id], Tag);
            }
            for &id in &ids {
                black_box(world.remove_component::<Tag>(entities[id]));
            }
            world.update(0.0);
            world.entity_count() as f64
        });
        measure("archetype_churn_tag_roundtrip", size, 1, ids.len(), || {
            tables.churn(&ids) as f64
        });
        tables.validate();
        assert_eq!(
            tables.query4(),
            world
                .query::<(&Position, &Velocity, &Health, &Tag)>()
                .map(|(_, p, v, h, _)| score2(p, v) + h.0 as f64)
                .sum::<f64>()
        );
    }
}

fn wide_churn() {
    for size in [1_000, 10_000, 100_000] {
        let ids = shuffled(size);
        let (mut world, entities) = sparse_world(size, usize::MAX);
        for (index, &id) in entities.iter().enumerate() {
            world.add_component(id, Cold([index as u64; 32]));
        }
        let mut tables = archetype::Archetypes::new(size, usize::MAX).with_cold_rows();
        measure("sparse_churn_wide_roundtrip", size, 1, ids.len(), || {
            for &id in &ids {
                world.add_component(entities[id], Tag);
            }
            for &id in &ids {
                black_box(world.remove_component::<Tag>(entities[id]));
            }
            world.update(0.0);
            black_box(
                world
                    .get_component::<Cold>(entities[size / 2])
                    .expect("retained cold component")
                    .0,
            );
            world.entity_count() as f64
        });
        measure("archetype_churn_wide_roundtrip", size, 1, ids.len(), || {
            tables.churn(&ids) as f64
        });
        tables.validate();
        assert_eq!(
            tables.query2(),
            world
                .query::<(&Position, &Velocity)>()
                .map(|(_, p, v)| score2(p, v))
                .sum::<f64>()
        );
    }
}

fn main() {
    println!("scenario,entities,threads,matches,sample,iterations,ns_per_iteration,checksum");
    let mode = std::env::var("KATLA_BENCH_MODE").unwrap_or_else(|_| "all".to_string());
    if mode == "all" || mode == "storage" {
        storage();
        wide_churn();
    }
    if mode == "all" || mode == "scheduler" {
        scheduling::run();
    }
    #[cfg(not(katla_ecs_baseline))]
    if mode == "all" || mode == "typed" {
        typed::run();
    }
}
