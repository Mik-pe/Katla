use std::sync::atomic::AtomicUsize;

use crate::system::ComponentAccess;
use crate::system::OrderedSystem;
use crate::unsafe_world_cell::UnsafeWorldCell;

#[derive(Copy, Clone)]
struct SendPtr(*mut OrderedSystem);
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

impl SendPtr {
    fn get(self) -> *mut OrderedSystem {
        self.0
    }
}

/// Node in the system dependency DAG.
pub(crate) struct SystemNode {
    /// Index into the system list.
    index: usize,
    /// Indices of systems that must complete before this one.
    dependencies: Vec<usize>,
    /// Indices of systems that depend on this one.
    dependents: Vec<usize>,
    /// Component access pattern for this system.
    access: Vec<ComponentAccess>,
    /// Number of unresolved dependencies (for topological execution).
    unresolved_deps: AtomicUsize,
}

/// Builds and manages a dependency DAG of systems based on component access conflicts.
///
/// Systems that access disjoint component sets can execute in parallel.
/// Conflicts (read-write or write-write on the same component type) create
/// dependency edges, and the resulting DAG is split into execution groups
/// where systems within a group may run concurrently.
pub(crate) struct SystemScheduler {
    nodes: Vec<SystemNode>,
    /// Execution order: groups of systems that can run in parallel.
    groups: Vec<Vec<usize>>,
}

impl SystemScheduler {
    /// Build a DAG from a list of (system_index, access_pattern) pairs.
    ///
    /// For each pair of systems, a conflict is detected when:
    /// - Both write the same component type, or
    /// - One reads and the other writes the same component type.
    ///
    /// Two systems that only read the same component do NOT conflict and
    /// may execute in parallel.
    pub fn build(systems: &[(usize, Vec<ComponentAccess>)]) -> Self {
        let nodes: Vec<SystemNode> = systems
            .iter()
            .map(|(sys_index, access)| SystemNode {
                index: *sys_index,
                dependencies: Vec::new(),
                dependents: Vec::new(),
                access: access.clone(),
                unresolved_deps: AtomicUsize::new(0),
            })
            .collect();

        let mut scheduler = Self {
            nodes,
            groups: Vec::new(),
        };

        scheduler.build_edges();
        scheduler.compute_groups();
        scheduler
    }

    /// Get the execution groups (systems within each group can run in parallel).
    #[cfg(test)]
    pub fn groups(&self) -> &[Vec<usize>] {
        &self.groups
    }

    /// Execute systems in parallel according to the computed groups.
    ///
    /// Systems within a group run in parallel via rayon. Groups run sequentially
    /// in topological order. Single-system groups run on the current thread.
    pub fn execute_parallel(
        &self,
        systems: &mut [OrderedSystem],
        world_cell: UnsafeWorldCell,
        delta_time: f32,
    ) {
        for group in &self.groups {
            if group.len() <= 1 {
                for &sys_idx in group {
                    let ordered = &mut systems[sys_idx];
                    if !ordered.system.is_enabled() {
                        continue;
                    }
                    let world = unsafe { &mut *world_cell.as_ptr() };
                    ordered.system.update(world, delta_time);
                }
                continue;
            }

            let enabled: Vec<usize> = group
                .iter()
                .filter(|&&idx| systems[idx].system.is_enabled())
                .copied()
                .collect();

            if enabled.len() <= 1 {
                for &sys_idx in &enabled {
                    let world = unsafe { &mut *world_cell.as_ptr() };
                    systems[sys_idx].system.update(world, delta_time);
                }
                continue;
            }

            let systems_ptr = SendPtr(systems.as_mut_ptr());
            rayon::scope(|s| {
                for &sys_idx in &enabled {
                    let ptr = systems_ptr;
                    s.spawn(move |_| {
                        let ordered = unsafe { &mut *ptr.get().add(sys_idx) };
                        let world = unsafe { &mut *world_cell.as_ptr() };
                        ordered.system.update(world, delta_time);
                    });
                }
            });
        }
    }

    fn build_edges(&mut self) {
        // Systems are sorted by SystemExecutionOrder before the scheduler is built
        // (see World::register_system → sort_systems → scheduler_cache = None, then
        // update_parallel rebuilds from the already-sorted list).  Vector index therefore
        // reflects execution order: lower index = earlier execution.  For mutual conflicts
        // the edge direction i→j (later depends on earlier) preserves the intended order.
        let n = self.nodes.len();
        for i in 0..n {
            for j in (i + 1)..n {
                if conflicts(&self.nodes[i].access, &self.nodes[j].access) {
                    self.nodes[j].dependencies.push(i);
                    self.nodes[i].dependents.push(j);
                }
            }
        }
    }

    fn compute_groups(&mut self) {
        let n = self.nodes.len();
        if n == 0 {
            return;
        }

        // Initialize unresolved_deps from the dependency counts.
        for node in &self.nodes {
            node.unresolved_deps.store(
                node.dependencies.len(),
                std::sync::atomic::Ordering::Relaxed,
            );
        }

        let mut remaining = n;
        let mut visited = vec![false; n];

        while remaining > 0 {
            let mut group = Vec::new();

            for (i, node) in self.nodes.iter().enumerate() {
                if visited[i] {
                    continue;
                }
                if node
                    .unresolved_deps
                    .load(std::sync::atomic::Ordering::Relaxed)
                    == 0
                {
                    group.push(i);
                }
            }

            if group.is_empty() {
                panic!("cycle detected in system dependency graph");
            }

            for &i in &group {
                visited[i] = true;
                for &dep in &self.nodes[i].dependents {
                    self.nodes[dep]
                        .unresolved_deps
                        .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                }
            }

            remaining -= group.len();
            self.groups
                .push(group.iter().map(|&i| self.nodes[i].index).collect());
        }
    }
}

fn conflicts(a: &[ComponentAccess], b: &[ComponentAccess]) -> bool {
    for access_a in a {
        for access_b in b {
            match (access_a, access_b) {
                (ComponentAccess::Write(ta), ComponentAccess::Write(tb)) if ta == tb => {
                    return true;
                }
                (ComponentAccess::Write(ta), ComponentAccess::Read(tb)) if ta == tb => return true,
                (ComponentAccess::Read(ta), ComponentAccess::Write(tb)) if ta == tb => return true,
                _ => {}
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use super::*;

    fn make_systems(access: Vec<Vec<ComponentAccess>>) -> Vec<(usize, Vec<ComponentAccess>)> {
        access.into_iter().enumerate().collect()
    }

    #[test]
    fn test_write_write_same_component_creates_edge() {
        let type_id = TypeId::of::<u32>();
        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_id)],
            vec![ComponentAccess::Write(type_id)],
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
    }

    #[test]
    fn test_write_different_components_no_conflict() {
        let systems = make_systems(vec![
            vec![ComponentAccess::Write(TypeId::of::<u32>())],
            vec![ComponentAccess::Write(TypeId::of::<u64>())],
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_read_read_same_component_no_conflict() {
        let type_id = TypeId::of::<u32>();
        let systems = make_systems(vec![
            vec![ComponentAccess::Read(type_id)],
            vec![ComponentAccess::Read(type_id)],
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_read_write_same_component_creates_edge() {
        let type_id = TypeId::of::<u32>();
        let systems = make_systems(vec![
            vec![ComponentAccess::Read(type_id)],
            vec![ComponentAccess::Write(type_id)],
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
    }

    #[test]
    fn test_chain_three_groups() {
        // A writes X, B reads X writes Y, C reads Y -> 3 groups: (A), (B), (C)
        let type_x = TypeId::of::<u32>();
        let type_y = TypeId::of::<u64>();

        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_x)], // A
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_y),
            ], // B
            vec![ComponentAccess::Read(type_y)],  // C
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 3);
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
        assert!(groups[2].contains(&2));
    }

    #[test]
    fn test_diamond_dependency() {
        // A writes X, B reads X writes Y, C reads X writes Z, D reads Y+Z
        let type_x = TypeId::of::<u32>();
        let type_y = TypeId::of::<u64>();
        let type_z = TypeId::of::<i32>();

        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_x)], // A
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_y),
            ], // B
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_z),
            ], // C
            vec![ComponentAccess::Read(type_y), ComponentAccess::Read(type_z)], // D
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        // Group 0: A
        // Group 1: B, C (both read X, write different components)
        // Group 2: D (reads Y and Z)
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1].len(), 2);
        assert!(groups[1].contains(&1));
        assert!(groups[1].contains(&2));
        assert_eq!(groups[2], vec![3]);
    }

    #[test]
    fn test_empty_systems() {
        let scheduler = SystemScheduler::build(&[]);
        assert!(scheduler.groups().is_empty());
    }

    #[test]
    fn test_single_system() {
        let systems = make_systems(vec![vec![ComponentAccess::Write(TypeId::of::<u32>())]]);
        let scheduler = SystemScheduler::build(&systems);

        assert_eq!(scheduler.groups().len(), 1);
        assert_eq!(scheduler.groups()[0], vec![0]);
    }

    #[test]
    fn test_no_access_no_conflict() {
        let systems = make_systems(vec![Vec::new(), Vec::new()]);
        let scheduler = SystemScheduler::build(&systems);

        assert_eq!(scheduler.groups().len(), 1);
        assert_eq!(scheduler.groups()[0].len(), 2);
    }

    #[test]
    fn test_mixed_read_write_multiple_types() {
        // System 0: reads A, writes B
        // System 1: reads B, writes C
        // System 2: reads C
        // All sequential due to write-write conflicts on B and C
        let type_a = TypeId::of::<u8>();
        let type_b = TypeId::of::<u16>();
        let type_c = TypeId::of::<u32>();

        let systems = make_systems(vec![
            vec![
                ComponentAccess::Read(type_a),
                ComponentAccess::Write(type_b),
            ],
            vec![
                ComponentAccess::Read(type_b),
                ComponentAccess::Write(type_c),
            ],
            vec![ComponentAccess::Read(type_c)],
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 3);
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
        assert!(groups[2].contains(&2));
    }

    #[test]
    fn test_parallel_schedule_ordering() {
        // A writes X, B writes Y, C reads X+Y
        // A and B should run in parallel, C must wait for both
        let type_x = TypeId::of::<u32>();
        let type_y = TypeId::of::<u64>();

        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_x)], // A (index 0)
            vec![ComponentAccess::Write(type_y)], // B (index 1)
            vec![ComponentAccess::Read(type_x), ComponentAccess::Read(type_y)], // C (index 2)
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
        assert!(groups[0].contains(&0)); // A
        assert!(groups[0].contains(&1)); // B
        assert!(groups[1].contains(&2)); // C
    }

    #[test]
    fn test_all_independent_single_group() {
        // A writes X, B writes Y, C writes Z — all in same group
        let type_x = TypeId::of::<u32>();
        let type_y = TypeId::of::<u64>();
        let type_z = TypeId::of::<i32>();

        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_x)], // A
            vec![ComponentAccess::Write(type_y)], // B
            vec![ComponentAccess::Write(type_z)], // C
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
        assert!(groups[0].contains(&0));
        assert!(groups[0].contains(&1));
        assert!(groups[0].contains(&2));
    }

    #[test]
    fn test_all_conflicting_separate_groups() {
        // A writes X, B writes X, C writes X — all separate groups
        let type_x = TypeId::of::<u32>();

        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_x)], // A
            vec![ComponentAccess::Write(type_x)], // B
            vec![ComponentAccess::Write(type_x)], // C
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1], vec![1]);
        assert_eq!(groups[2], vec![2]);
    }

    #[test]
    fn test_preserves_system_indices() {
        let systems: Vec<(usize, Vec<ComponentAccess>)> = vec![
            (5, vec![ComponentAccess::Write(TypeId::of::<u32>())]),
            (10, vec![ComponentAccess::Write(TypeId::of::<u64>())]),
        ];

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 1);
        assert!(groups[0].contains(&5));
        assert!(groups[0].contains(&10));
    }

    #[test]
    fn test_mutual_conflict_resolves_ordering() {
        // A reads X writes Y, B reads Y writes X.
        // Both conflict with each other, but build_edges only creates a
        // one-way edge (later system depends on earlier). This verifies
        // the scheduler doesn't panic and produces a valid ordering.
        let type_x = TypeId::of::<u32>();
        let type_y = TypeId::of::<u64>();

        let systems = make_systems(vec![
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_y),
            ],
            vec![
                ComponentAccess::Read(type_y),
                ComponentAccess::Write(type_x),
            ],
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
    }

    #[test]
    fn test_large_dag_wide_fan_out() {
        // A writes X. Then 5 independent systems each read X and write unique types.
        // All 5 should be in a single parallel group after A.
        let type_x = TypeId::of::<u32>();
        let type_a = TypeId::of::<u8>();
        let type_b = TypeId::of::<u16>();
        let type_c = TypeId::of::<i8>();
        let type_d = TypeId::of::<i16>();
        let type_e = TypeId::of::<f32>();

        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_x)], // A (source)
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_a),
            ], // B
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_b),
            ], // C
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_c),
            ], // D
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_d),
            ], // E
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_e),
            ], // F
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![0]); // A
        assert_eq!(groups[1].len(), 5); // B, C, D, E, F all parallel
        for i in 1..=5 {
            assert!(groups[1].contains(&i));
        }
    }

    #[test]
    fn test_large_dag_multi_level() {
        // Level 0: A writes X, B writes Y (parallel, no conflict)
        // Level 1: C reads X+Y writes Z (depends on both A and B)
        // Level 2: D reads Z writes W, E reads Z writes V (parallel, both read Z)
        // Level 3: F reads W+V (depends on D and E)
        let type_x = TypeId::of::<u8>();
        let type_y = TypeId::of::<u16>();
        let type_z = TypeId::of::<u32>();
        let type_w = TypeId::of::<u64>();
        let type_v = TypeId::of::<i8>();

        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_x)], // A
            vec![ComponentAccess::Write(type_y)], // B
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Read(type_y),
                ComponentAccess::Write(type_z),
            ], // C
            vec![
                ComponentAccess::Read(type_z),
                ComponentAccess::Write(type_w),
            ], // D
            vec![
                ComponentAccess::Read(type_z),
                ComponentAccess::Write(type_v),
            ], // E
            vec![ComponentAccess::Read(type_w), ComponentAccess::Read(type_v)], // F
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 4);
        assert_eq!(groups[0].len(), 2); // A, B
        assert!(groups[0].contains(&0));
        assert!(groups[0].contains(&1));
        assert_eq!(groups[1], vec![2]); // C (depends on A+B)
        assert_eq!(groups[2].len(), 2); // D, E (both read Z, write different)
        assert!(groups[2].contains(&3));
        assert!(groups[2].contains(&4));
        assert_eq!(groups[3], vec![5]); // F (reads W+V)
    }

    #[test]
    fn test_partial_read_overlap_parallel() {
        // Sys0 reads A + writes B, Sys1 reads A + writes C.
        // Only read overlap on A — no conflict. Should be parallel.
        let type_a = TypeId::of::<u8>();
        let type_b = TypeId::of::<u16>();
        let type_c = TypeId::of::<u32>();

        let systems = make_systems(vec![
            vec![
                ComponentAccess::Read(type_a),
                ComponentAccess::Write(type_b),
            ],
            vec![
                ComponentAccess::Read(type_a),
                ComponentAccess::Write(type_c),
            ],
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_multiple_readers_one_writer() {
        // Sys0 writes X. Sys1 reads X writes Y. Sys2 reads X writes Z.
        // Sys0 must run first, then Sys1 and Sys2 in parallel.
        let type_x = TypeId::of::<u32>();
        let type_y = TypeId::of::<u64>();
        let type_z = TypeId::of::<i32>();

        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_x)], // writer
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_y),
            ],
            vec![
                ComponentAccess::Read(type_x),
                ComponentAccess::Write(type_z),
            ],
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1].len(), 2);
        assert!(groups[1].contains(&1));
        assert!(groups[1].contains(&2));
    }

    #[test]
    fn test_system_with_many_components_partial_conflict() {
        // Sys0 writes A B C, Sys1 writes C D E — conflict only on C
        let type_a = TypeId::of::<u8>();
        let type_b = TypeId::of::<u16>();
        let type_c = TypeId::of::<u32>();
        let type_d = TypeId::of::<u64>();
        let type_e = TypeId::of::<i8>();

        let systems = make_systems(vec![
            vec![
                ComponentAccess::Write(type_a),
                ComponentAccess::Write(type_b),
                ComponentAccess::Write(type_c),
            ],
            vec![
                ComponentAccess::Write(type_c),
                ComponentAccess::Write(type_d),
                ComponentAccess::Write(type_e),
            ],
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2); // sequential due to C conflict
    }

    #[test]
    fn test_write_read_read_chain() {
        // Sys0 writes X, Sys1 reads X, Sys2 reads X
        // Only Sys0 conflicts with Sys1 and Sys2. Sys1 and Sys2 have no conflict.
        // Group 0: Sys0, Group 1: Sys1 + Sys2
        let type_x = TypeId::of::<u32>();

        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_x)],
            vec![ComponentAccess::Read(type_x)],
            vec![ComponentAccess::Read(type_x)],
        ]);

        let scheduler = SystemScheduler::build(&systems);
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1].len(), 2);
        assert!(groups[1].contains(&1));
        assert!(groups[1].contains(&2));
    }
}
