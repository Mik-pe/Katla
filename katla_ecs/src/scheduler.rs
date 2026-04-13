use std::sync::atomic::AtomicUsize;

use crate::system::ComponentAccess;

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
    pub fn groups(&self) -> &[Vec<usize>] {
        &self.groups
    }

    fn build_edges(&mut self) {
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
        access
            .into_iter()
            .enumerate()
            .map(|(i, a)| (i, a))
            .collect()
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
    fn test_preserves_system_indices() {
        // Use non-sequential indices to verify original indices are preserved
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
}
