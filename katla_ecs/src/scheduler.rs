use crate::system::{
    ComponentAccess, OrderedSystem, ResourceAccess, SystemExecutionOrder, SystemKind,
};

/// Cached dependency batches. Ordering tiers are absolute barriers.
pub(crate) struct SystemScheduler {
    groups: Vec<Vec<usize>>,
}

#[derive(Debug)]
pub(crate) enum SchedulerError {
    DependencyCycle,
}
impl std::fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cycle detected in system dependency graph")
    }
}
impl std::error::Error for SchedulerError {}

type Claims = (
    usize,
    Vec<ComponentAccess>,
    Vec<ResourceAccess>,
    SystemExecutionOrder,
    bool,
);
impl SystemScheduler {
    pub fn from_systems(systems: &[OrderedSystem]) -> Self {
        let claims: Vec<_> = systems
            .iter()
            .enumerate()
            .map(|(index, system)| {
                (
                    index,
                    system.access.components.clone(),
                    system.access.resources.clone(),
                    system.order,
                    matches!(system.system, SystemKind::Exclusive(_)),
                )
            })
            .collect();
        Self::build_claims(&claims).expect("forward-only system dependencies are acyclic")
    }
    #[cfg(test)]
    pub fn build(
        systems: &[(usize, Vec<ComponentAccess>, Vec<ResourceAccess>)],
    ) -> Result<Self, SchedulerError> {
        let claims: Vec<_> = systems
            .iter()
            .map(|(index, components, resources)| {
                (
                    *index,
                    components.clone(),
                    resources.clone(),
                    SystemExecutionOrder::NORMAL,
                    false,
                )
            })
            .collect();
        Self::build_claims(&claims)
    }
    fn build_claims(systems: &[Claims]) -> Result<Self, SchedulerError> {
        let n = systems.len();
        let mut dependencies = vec![Vec::new(); n];
        for i in 0..n {
            for j in (i + 1)..n {
                let a = &systems[i];
                let b = &systems[j];
                if a.3 != b.3 || a.4 || b.4 || conflicts(&a.1, &b.1, &a.2, &b.2) {
                    dependencies[j].push(i);
                }
            }
        }
        let mut done = vec![false; n];
        let mut remaining = n;
        let mut groups = Vec::new();
        while remaining != 0 {
            let ready: Vec<_> = (0..n)
                .filter(|&i| !done[i] && dependencies[i].iter().all(|&d| done[d]))
                .collect();
            if ready.is_empty() {
                return Err(SchedulerError::DependencyCycle);
            }
            for &i in &ready {
                done[i] = true;
            }
            remaining -= ready.len();
            groups.push(ready.into_iter().map(|i| systems[i].0).collect());
        }
        Ok(Self { groups })
    }
    pub fn groups(&self) -> &[Vec<usize>] {
        &self.groups
    }
}

fn conflicts(
    comp_a: &[ComponentAccess],
    comp_b: &[ComponentAccess],
    res_a: &[ResourceAccess],
    res_b: &[ResourceAccess],
) -> bool {
    component_conflicts(comp_a, comp_b) || resource_conflicts(res_a, res_b)
}

fn component_conflicts(a: &[ComponentAccess], b: &[ComponentAccess]) -> bool {
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

fn resource_conflicts(a: &[ResourceAccess], b: &[ResourceAccess]) -> bool {
    for access_a in a {
        for access_b in b {
            match (access_a, access_b) {
                (ResourceAccess::Write(ta), ResourceAccess::Write(tb)) if ta == tb => {
                    return true;
                }
                (ResourceAccess::Write(ta), ResourceAccess::Read(tb)) if ta == tb => return true,
                (ResourceAccess::Read(ta), ResourceAccess::Write(tb)) if ta == tb => return true,
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

    fn make_systems(
        access: Vec<Vec<ComponentAccess>>,
    ) -> Vec<(usize, Vec<ComponentAccess>, Vec<ResourceAccess>)> {
        access
            .into_iter()
            .enumerate()
            .map(|(i, a)| (i, a, Vec::new()))
            .collect()
    }

    fn make_systems_with_resources(
        access: Vec<(Vec<ComponentAccess>, Vec<ResourceAccess>)>,
    ) -> Vec<(usize, Vec<ComponentAccess>, Vec<ResourceAccess>)> {
        access
            .into_iter()
            .enumerate()
            .map(|(i, (a, r))| (i, a, r))
            .collect()
    }

    #[test]
    fn test_write_write_same_component_creates_edge() {
        let type_id = TypeId::of::<u32>();
        let systems = make_systems(vec![
            vec![ComponentAccess::Write(type_id)],
            vec![ComponentAccess::Write(type_id)],
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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
        let scheduler = SystemScheduler::build(&[]).unwrap();
        assert!(scheduler.groups().is_empty());
    }

    #[test]
    fn test_single_system() {
        let systems = make_systems(vec![vec![ComponentAccess::Write(TypeId::of::<u32>())]]);
        let scheduler = SystemScheduler::build(&systems).unwrap();

        assert_eq!(scheduler.groups().len(), 1);
        assert_eq!(scheduler.groups()[0], vec![0]);
    }

    #[test]
    fn test_no_access_no_conflict() {
        let systems = make_systems(vec![Vec::new(), Vec::new()]);
        let scheduler = SystemScheduler::build(&systems).unwrap();

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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1], vec![1]);
        assert_eq!(groups[2], vec![2]);
    }

    #[test]
    fn test_preserves_system_indices() {
        let systems: Vec<(usize, Vec<ComponentAccess>, Vec<ResourceAccess>)> = vec![
            (
                5,
                vec![ComponentAccess::Write(TypeId::of::<u32>())],
                Vec::new(),
            ),
            (
                10,
                vec![ComponentAccess::Write(TypeId::of::<u64>())],
                Vec::new(),
            ),
        ];

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
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

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1].len(), 2);
        assert!(groups[1].contains(&1));
        assert!(groups[1].contains(&2));
    }

    #[test]
    fn test_resource_write_write_same_creates_edge() {
        let res_x = TypeId::of::<u32>();
        let systems = make_systems_with_resources(vec![
            (Vec::new(), vec![ResourceAccess::Write(res_x)]),
            (Vec::new(), vec![ResourceAccess::Write(res_x)]),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
    }

    #[test]
    fn test_resource_read_write_same_creates_edge() {
        let res_x = TypeId::of::<u32>();
        let systems = make_systems_with_resources(vec![
            (Vec::new(), vec![ResourceAccess::Read(res_x)]),
            (Vec::new(), vec![ResourceAccess::Write(res_x)]),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
    }

    #[test]
    fn test_resource_read_read_no_conflict() {
        let res_x = TypeId::of::<u32>();
        let systems = make_systems_with_resources(vec![
            (Vec::new(), vec![ResourceAccess::Read(res_x)]),
            (Vec::new(), vec![ResourceAccess::Read(res_x)]),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_resource_write_different_no_conflict() {
        let systems = make_systems_with_resources(vec![
            (Vec::new(), vec![ResourceAccess::Write(TypeId::of::<u32>())]),
            (Vec::new(), vec![ResourceAccess::Write(TypeId::of::<u64>())]),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_resource_conflict_independent_of_component_access() {
        // No component conflict, but resource write-write conflict
        let type_a = TypeId::of::<u32>();
        let res_x = TypeId::of::<u64>();

        let systems = make_systems_with_resources(vec![
            (
                vec![ComponentAccess::Write(type_a)],
                vec![ResourceAccess::Write(res_x)],
            ),
            (Vec::new(), vec![ResourceAccess::Write(res_x)]),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_component_and_resource_conflict_combined() {
        // Component conflict between sys0 and sys1, resource conflict between sys1 and sys2
        let type_a = TypeId::of::<u32>();
        let res_x = TypeId::of::<u64>();

        let systems = make_systems_with_resources(vec![
            (vec![ComponentAccess::Write(type_a)], Vec::new()),
            (
                vec![ComponentAccess::Write(type_a)],
                vec![ResourceAccess::Write(res_x)],
            ),
            (Vec::new(), vec![ResourceAccess::Write(res_x)]),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1], vec![1]);
        assert_eq!(groups[2], vec![2]);
    }

    #[test]
    fn test_resource_only_conflict_creates_groups() {
        let res_x = TypeId::of::<u32>();
        let systems = make_systems_with_resources(vec![
            (Vec::new(), vec![ResourceAccess::Write(res_x)]),
            (Vec::new(), vec![ResourceAccess::Write(res_x)]),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(
            groups.len(),
            2,
            "write-write on same resource must split into separate groups"
        );
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
    }

    #[test]
    fn test_resource_read_write_creates_dependency() {
        let res_x = TypeId::of::<u32>();
        let systems = make_systems_with_resources(vec![
            (Vec::new(), vec![ResourceAccess::Read(res_x)]),
            (Vec::new(), vec![ResourceAccess::Write(res_x)]),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(
            groups.len(),
            2,
            "read-write on same resource must create dependency"
        );
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
    }

    #[test]
    fn test_independent_resources_parallel_group() {
        let res_r1 = TypeId::of::<u8>();
        let res_r2 = TypeId::of::<u16>();
        let res_r3 = TypeId::of::<u32>();

        let systems = make_systems_with_resources(vec![
            (Vec::new(), vec![ResourceAccess::Write(res_r1)]),
            (Vec::new(), vec![ResourceAccess::Write(res_r2)]),
            (Vec::new(), vec![ResourceAccess::Write(res_r3)]),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(
            groups.len(),
            1,
            "independent resource writes should be in one parallel group"
        );
        assert_eq!(groups[0].len(), 3);
        assert!(groups[0].contains(&0));
        assert!(groups[0].contains(&1));
        assert!(groups[0].contains(&2));
    }

    #[test]
    fn test_mixed_component_resource_conflict() {
        let comp_a = TypeId::of::<u8>();
        let comp_b = TypeId::of::<u16>();
        let res_r = TypeId::of::<u32>();

        let systems = make_systems_with_resources(vec![
            (
                vec![ComponentAccess::Write(comp_a)],
                vec![ResourceAccess::Read(res_r)],
            ),
            (
                vec![ComponentAccess::Write(comp_b)],
                vec![ResourceAccess::Write(res_r)],
            ),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(
            groups.len(),
            2,
            "no component conflict (A != B), but resource read-write conflict forces separate groups"
        );
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
    }

    #[test]
    fn test_component_and_resource_conflict_adds_single_edge() {
        let comp_a = TypeId::of::<u8>();
        let res_r = TypeId::of::<u32>();

        let systems = make_systems_with_resources(vec![
            (
                vec![ComponentAccess::Write(comp_a)],
                vec![ResourceAccess::Write(res_r)],
            ),
            (
                vec![ComponentAccess::Write(comp_a)],
                vec![ResourceAccess::Write(res_r)],
            ),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        assert_eq!(
            groups.len(),
            2,
            "conflicts on both component and resource should produce exactly one edge, not two"
        );
        assert!(groups[0].contains(&0));
        assert!(groups[1].contains(&1));
    }

    #[test]
    fn test_resource_conflict_ready_systems() {
        let res_r = TypeId::of::<u32>();

        let systems = make_systems_with_resources(vec![
            (Vec::new(), vec![ResourceAccess::Write(res_r)]),
            (Vec::new(), vec![ResourceAccess::Write(res_r)]),
            (Vec::new(), vec![ResourceAccess::Write(res_r)]),
        ]);

        let scheduler = SystemScheduler::build(&systems).unwrap();
        let groups = scheduler.groups();

        // All three conflict with each other on the same resource,
        // so they must be in separate sequential groups: [0], [1], [2].
        // Group ordering maps directly to ready-system behavior:
        //   After group 0 (sys0) completes, group 1 (sys1) becomes ready.
        //   After group 1 (sys1) completes, group 2 (sys2) becomes ready.
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec![0], "sys0 runs first");
        assert_eq!(groups[1], vec![1], "sys1 ready after sys0 completes");
        assert_eq!(groups[2], vec![2], "sys2 ready after sys1 completes");
    }
}
