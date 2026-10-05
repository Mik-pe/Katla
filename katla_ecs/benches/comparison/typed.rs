use super::{Health, Position, Tag, Velocity, measure, score2, sparse_world};
use katla_ecs::{
    Query, QueryView, Read, SystemExecutionOrder, SystemParam, TypedSystem, World, Write,
};

struct Sum2;
impl TypedSystem for Sum2 {
    type Params = Query<(Read<Position>, Read<Velocity>)>;
    fn run(&mut self, query: <Self::Params as SystemParam>::Item<'_>, _: f32) {
        std::hint::black_box(query.iter().map(|(_, (p, v))| score2(p, v)).sum::<f64>());
    }
}
struct Sum4;
impl TypedSystem for Sum4 {
    type Params = Query<(Read<Position>, Read<Velocity>, Read<Health>, Read<Tag>)>;
    fn run(&mut self, query: <Self::Params as SystemParam>::Item<'_>, _: f32) {
        std::hint::black_box(
            query
                .iter()
                .map(|(_, (p, v, h, _))| score2(p, v) + h.0 as f64)
                .sum::<f64>(),
        );
    }
}
struct ChunkWork {
    parallel: bool,
    rounds: usize,
}
impl TypedSystem for ChunkWork {
    type Params = Query<(Write<Position>, Read<Velocity>, Read<Health>)>;
    fn run(
        &mut self,
        mut query: QueryView<'_, (Write<Position>, Read<Velocity>, Read<Health>)>,
        _: f32,
    ) {
        let rounds = self.rounds;
        let work = |_, (p, v, h): (&mut Position, &Velocity, &Health)| {
            let mut x = p.0[0];
            for _ in 0..rounds {
                x = std::hint::black_box(x).mul_add(1.000001, v.0[0] * h.0 * 0.000001);
            }
            p.0[0] = x;
        };
        if self.parallel {
            query.par_for_each_mut(1024, work);
        } else {
            query.for_each_mut(work);
        }
    }
}
fn frame(world: &mut World) -> f64 {
    world.update_parallel(1.0 / 60.0);
    world.entity_count() as f64
}

pub(super) fn run() {
    for size in [1_000, 10_000, 100_000] {
        for stride in [1, 100] {
            let suffix = if stride == 1 { "dense" } else { "one_percent" };
            let (mut world, _) = sparse_world(size, stride);
            world.register_typed_system(Sum2, SystemExecutionOrder::NORMAL);
            measure(&format!("typed_query2_{suffix}"), size, 1, size, || {
                frame(&mut world)
            });
            let (mut world, _) = sparse_world(size, stride);
            world.register_typed_system(Sum4, SystemExecutionOrder::NORMAL);
            measure(
                &format!("typed_query4_{suffix}"),
                size,
                1,
                size.div_ceil(stride),
                || frame(&mut world),
            );
        }
    }
    for threads in [1, 2, 4, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("build chunk benchmark worker pool");
        for size in [1_000, 10_000, 100_000] {
            for rounds in [1, 64] {
                for parallel in [false, true] {
                    pool.install(|| {
                        let (mut world, _) = sparse_world(size, 1);
                        world.register_typed_system(
                            ChunkWork { parallel, rounds },
                            SystemExecutionOrder::NORMAL,
                        );
                        let kind = if parallel { "parallel" } else { "sequential" };
                        measure(
                            &format!("typed_chunk_{kind}_rounds{rounds}"),
                            size,
                            threads,
                            size,
                            || frame(&mut world),
                        );
                    });
                }
            }
        }
    }
}
