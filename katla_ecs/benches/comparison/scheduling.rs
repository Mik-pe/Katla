use super::measure;
use katla_ecs::{Component, SystemExecutionOrder, World};

#[derive(Component)]
struct Lane<const N: usize>(f64);
struct Work<const N: usize> {
    rounds: usize,
}

fn compute(value: &mut f64, rounds: usize) {
    let mut result = *value;
    for _ in 0..rounds {
        result = std::hint::black_box(result).mul_add(1.000000001, 0.0000001);
    }
    *value = result;
}

#[cfg(katla_ecs_baseline)]
impl<const N: usize> katla_ecs::System for Work<N> {
    fn update(&mut self, world: &mut World, _: f32) {
        for (_, lane) in world.query::<&mut Lane<N>>() {
            compute(&mut lane.0, self.rounds);
        }
    }
    fn component_access() -> Vec<katla_ecs::ComponentAccess> {
        vec![katla_ecs::ComponentAccess::write::<Lane<N>>()]
    }
    fn component_access_dyn(&self) -> Vec<katla_ecs::ComponentAccess> {
        Self::component_access()
    }
    fn resource_access() -> Vec<katla_ecs::ResourceAccess> {
        Vec::new()
    }
    fn resource_access_dyn(&self) -> Vec<katla_ecs::ResourceAccess> {
        Self::resource_access()
    }
}

#[cfg(not(katla_ecs_baseline))]
impl<const N: usize> katla_ecs::TypedSystem for Work<N> {
    type Params = katla_ecs::Query<katla_ecs::Write<Lane<N>>>;
    fn run(&mut self, mut query: <Self::Params as katla_ecs::SystemParam>::Item<'_>, _: f32) {
        query.for_each_mut(|_, lane| compute(&mut lane.0, self.rounds));
    }
}

fn register<const N: usize>(world: &mut World, rounds: usize) {
    #[cfg(katla_ecs_baseline)]
    world.register_system(Box::new(Work::<N> { rounds }), SystemExecutionOrder::NORMAL);
    #[cfg(not(katla_ecs_baseline))]
    world.register_typed_system(Work::<N> { rounds }, SystemExecutionOrder::NORMAL);
}

fn world<const CONFLICT: bool>(size: usize, rounds: usize) -> World {
    let mut world = World::new();
    for _ in 0..size {
        world.spawn((
            Lane::<0>(1.0),
            Lane::<1>(1.0),
            Lane::<2>(1.0),
            Lane::<3>(1.0),
            Lane::<4>(1.0),
            Lane::<5>(1.0),
            Lane::<6>(1.0),
            Lane::<7>(1.0),
        ));
    }
    register::<0>(&mut world, rounds);
    if CONFLICT {
        for _ in 0..7 {
            register::<0>(&mut world, rounds);
        }
    } else {
        register::<1>(&mut world, rounds);
        register::<2>(&mut world, rounds);
        register::<3>(&mut world, rounds);
        register::<4>(&mut world, rounds);
        register::<5>(&mut world, rounds);
        register::<6>(&mut world, rounds);
        register::<7>(&mut world, rounds);
    }
    world
}

pub(super) fn run() {
    for threads in [1, 2, 4, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("build benchmark worker pool");
        for size in [0, 1_000, 10_000, 100_000] {
            for rounds in [1, 16] {
                for conflict in [false, true] {
                    pool.install(|| {
                        let mut world = if conflict {
                            world::<true>(size, rounds)
                        } else {
                            world::<false>(size, rounds)
                        };
                        let kind = if conflict {
                            "conflicting"
                        } else {
                            "independent"
                        };
                        world.update_parallel(1.0 / 60.0);
                        measure(
                            &format!("scheduler_sequential_{kind}_rounds{rounds}"),
                            size,
                            threads,
                            size * 8,
                            || {
                                world.update(1.0 / 60.0);
                                size as f64
                            },
                        );
                        measure(
                            &format!("scheduler_parallel_{kind}_rounds{rounds}"),
                            size,
                            threads,
                            size * 8,
                            || {
                                world.update_parallel(1.0 / 60.0);
                                size as f64
                            },
                        );
                    });
                }
            }
        }
    }
}
