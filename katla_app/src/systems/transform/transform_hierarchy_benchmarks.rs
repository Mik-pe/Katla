//! Portable CPU-only resolver benchmark, injectable unchanged into a baseline.

use super::resolve_world_transforms;
use crate::components::{Parent, TransformComponent};
use katla_ecs::World;
use katla_math::Vec3;
use std::hint::black_box;

use crate::cpu_bench_support as support;

fn benchmark_world(size: usize, shape: &str) -> (World, f64) {
    let mut world = World::new();
    let mut root = None;
    let mut previous = None;
    let mut expected = 0.0;
    for index in 0..size {
        let entity = world.spawn((TransformComponent::from_position(Vec3::X_AXIS),));
        let parent = match shape {
            "broad" => root,
            "deep" => previous,
            "mixed_depth" if index < size / 2 => previous,
            _ => None,
        };
        if let Some(parent) = parent {
            world.add_component(entity, Parent { parent });
        }
        let x = match shape {
            "broad" if index > 0 => 2,
            "deep" => index + 1,
            "mixed_depth" if index < size / 2 => index + 1,
            _ => 1,
        };
        expected += x as f64;
        root.get_or_insert(entity);
        previous = Some(entity);
    }
    (world, expected)
}

#[test]
#[ignore = "CPU performance measurement; run on an otherwise idle host"]
fn test_benchmark_transform_resolution() {
    println!("scenario,entities,threads,matches,sample,iterations,ns_per_iteration,checksum");
    for size in [1_000, 10_000, 100_000] {
        for shape in ["flat", "broad", "deep", "mixed_depth"] {
            let (world, expected) = benchmark_world(size, shape);
            let resolve = || {
                let poses = resolve_world_transforms(black_box(&world));
                black_box(&poses)
                    .values()
                    .map(|pose| f64::from(pose.transform.position.x()))
                    .sum::<f64>()
            };
            assert_eq!(resolve(), expected);
            support::measure(&format!("transform_resolve_{shape}"), size, resolve);
        }
    }
}
