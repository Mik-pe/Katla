//! System registration, batch execution and shutdown.

use super::World;
use crate::params::{ParamAccess, ParamContext, SystemParam};
use crate::scheduler::SystemScheduler;
use crate::system::{
    OrderedSystem, System, SystemExecutionOrder, SystemKind, TypedAdapter, TypedSystem,
};

impl World {
    /// Registers a caller-thread system with exclusive World access.
    pub fn register_exclusive_system(
        &mut self,
        mut system: Box<dyn System>,
        order: SystemExecutionOrder,
    ) {
        system.initialize();
        let registration = self.next_registration;
        self.next_registration += 1;
        self.systems.push(OrderedSystem {
            system: SystemKind::Exclusive(system),
            order,
            registration,
            access: ParamAccess::default(),
        });
        self.sort_systems();
        self.scheduler_cache = None;
    }

    /// Registers a typed system after rejecting every incompatible parameter alias.
    pub fn register_typed_system<S: TypedSystem>(
        &mut self,
        system: S,
        order: SystemExecutionOrder,
    ) {
        let mut access = ParamAccess::default();
        S::Params::access(&mut access);
        let state = S::Params::init(self);
        let mut adapter = TypedAdapter { system, state };
        crate::system::ErasedTypedSystem::initialize(&mut adapter);
        let registration = self.next_registration;
        self.next_registration += 1;
        self.systems.push(OrderedSystem {
            system: SystemKind::Typed(Box::new(adapter)),
            order,
            registration,
            access,
        });
        self.sort_systems();
        self.scheduler_cache = None;
    }

    fn sort_systems(&mut self) {
        self.systems
            .sort_by_key(|system| (system.order, system.registration));
    }

    /// Runs the dependency batches on the caller thread.
    /// Commands become visible at the same batch boundaries as parallel updates.
    pub fn update(&mut self, delta_time: f32) {
        self.execute_systems(delta_time, false);
    }

    /// Runs independent typed jobs concurrently, with exclusive systems on the caller thread.
    pub fn update_parallel(&mut self, delta_time: f32) {
        self.execute_systems(delta_time, true);
    }

    /// Sets the minimum estimated batch work before dispatching Rayon jobs.
    /// The default is 32,768 entity-system pairs, tuned to avoid tiny batch overhead.
    /// A value of zero always dispatches batches containing multiple enabled systems.
    pub fn set_parallel_work_threshold(&mut self, threshold: usize) {
        self.parallel_work_threshold = threshold;
    }

    fn execute_systems(&mut self, delta_time: f32, parallel: bool) {
        let scheduler = self
            .scheduler_cache
            .take()
            .unwrap_or_else(|| SystemScheduler::from_systems(&self.systems));
        let mut systems = std::mem::take(&mut self.systems);
        self.execution_active = true;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for group in scheduler.groups() {
                if group.len() == 1
                    && let SystemKind::Exclusive(system) = &mut systems[group[0]].system
                {
                    if system.is_enabled() {
                        system.update(self, delta_time);
                    }
                    if self.clear_systems_requested {
                        break;
                    }
                    continue;
                }
                {
                    // Registries and structure remain frozen until all jobs and
                    // their typed borrows have been dropped at the end of scope.
                    let context = ParamContext {
                        // SAFETY: Registries remain frozen until every prepared job is dropped.
                        storage: unsafe { &*self.storage.get() },
                        resources: &self.resources,
                        epoch: self.structural_epoch,
                    };
                    let mut jobs = Vec::with_capacity(group.len());
                    for (index, ordered) in systems.iter_mut().enumerate() {
                        if group.contains(&index)
                            && let SystemKind::Typed(system) = &mut ordered.system
                            && system.is_enabled()
                        {
                            // SAFETY: Scheduler batches exclude conflicting claims;
                            // registration validated aliases inside each parameter tuple.
                            jobs.push(unsafe { system.prepare(&context, delta_time) });
                        }
                    }
                    let work = self.entity_count().max(1).saturating_mul(jobs.len());
                    if parallel && jobs.len() > 1 && work >= self.parallel_work_threshold {
                        rayon::scope(|scope| {
                            for job in jobs {
                                scope.spawn(move |_| job());
                            }
                        });
                    } else {
                        for job in jobs {
                            job();
                        }
                    }
                }
                let mut flush: Vec<_> = group
                    .iter()
                    .map(|&index| (systems[index].registration, systems[index].drain_commands()))
                    .collect();
                flush.sort_by_key(|(registration, _)| *registration);
                for (_, commands) in flush {
                    for command in commands {
                        command(self);
                    }
                }
            }
        }));
        if result.is_err() {
            for system in &mut systems {
                drop(system.drain_commands());
            }
        }
        self.execution_active = false;
        let cleared_systems = self.clear_systems_requested;
        self.clear_systems_requested = false;
        if cleared_systems {
            for system in &mut systems {
                system.shutdown();
            }
            systems.clear();
        }
        let added_systems = !self.systems.is_empty();
        systems.append(&mut self.systems);
        self.systems = systems;
        self.sort_systems();
        if !added_systems && !cleared_systems {
            self.scheduler_cache = Some(scheduler);
        }
        match result {
            Ok(()) => {
                self.entity_events.clear();
                self.component_events.clear();
                self.storage.get_mut().clear_changed();
            }
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// Returns the number of systems registered with the world.
    pub fn system_count(&self) -> usize {
        self.systems.len()
    }

    /// Removes all systems from the world.
    pub fn clear_systems(&mut self) {
        if self.execution_active {
            self.clear_systems_requested = true;
        }
        for ordered_system in &mut self.systems {
            ordered_system.shutdown();
        }
        self.systems.clear();
        self.scheduler_cache = None;
    }
}

impl Drop for World {
    fn drop(&mut self) {
        // Clean up systems when the world is destroyed
        for ordered_system in &mut self.systems {
            ordered_system.shutdown();
        }
    }
}
