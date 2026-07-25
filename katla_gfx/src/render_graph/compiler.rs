//! Graph compiler for the render graph API.
//!
//! This module derives one canonical pass dependency DAG from declared resource
//! accesses. The same DAG drives execution order, cycle diagnostics, per-pass
//! dependency metadata, and parallel scheduling groups.
//!
//! Resource accesses are versioned by declaration order. A write starts a new
//! version of a resource, reads consume the latest preceding version, and later
//! writes wait for all readers of the version they replace. This produces the
//! minimal RAW, WAR, and WAW ordering constraints without introducing backwards
//! dependencies from future writers.

use std::collections::{BTreeSet, HashMap};

use super::error::RenderGraphError;
use super::handles::ResourceId;
use super::pass::PassDesc;

/// Node in the pass dependency DAG.
///
/// Captures the resource reads/writes and predecessor/successor edges for a
/// single pass, along with the topological level used for parallel scheduling.
#[derive(Debug, Clone)]
#[expect(dead_code)]
pub(crate) struct PassDagNode {
    /// Index into the pass list.
    pub pass_index: usize,
    /// Resources this pass reads.
    pub reads: Vec<ResourceId>,
    /// Resources this pass writes.
    pub writes: Vec<ResourceId>,
    /// Indices of predecessor passes (must complete before this one).
    pub predecessors: Vec<usize>,
    /// Indices of successor passes (depend on this one).
    pub successors: Vec<usize>,
    /// Topological depth (0 for root passes).
    pub level: usize,
}

/// Compiled execution plan for a render graph.
///
/// Contains:
/// - topologically sorted pass indices;
/// - pass dependency DAG with predecessor/successor edges;
/// - parallel groups of passes that can execute concurrently.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub(super) sorted_passes: Vec<usize>,
    /// Pass dependency DAG nodes indexed by pass index.
    pub(super) dag: Vec<PassDagNode>,
    /// Groups of pass indices that can execute concurrently, ordered by level.
    pub(super) parallel_groups: Vec<Vec<usize>>,
}

/// Dependency graph node.
#[derive(Debug, Clone, Default)]
struct DependencyNode {
    incoming: BTreeSet<usize>,
    outgoing: BTreeSet<usize>,
}

/// Access state for the current declaration-order version of a resource.
#[derive(Debug, Default)]
struct ResourceAccessState {
    last_writer: Option<usize>,
    readers_since_write: BTreeSet<usize>,
}

/// Simplified pass info for the compiler (without the execute callback).
#[derive(Debug, Clone)]
pub struct PassInfo {
    pub name: String,
    pub reads: Vec<ResourceId>,
    pub writes: Vec<ResourceId>,
}

impl From<&PassDesc> for PassInfo {
    fn from(desc: &PassDesc) -> Self {
        Self {
            name: desc.name.clone(),
            reads: desc.reads.clone(),
            writes: desc.writes.clone(),
        }
    }
}

fn add_dependency(graph: &mut [DependencyNode], predecessor: usize, successor: usize) {
    if predecessor == successor {
        return;
    }

    if graph[predecessor].outgoing.insert(successor) {
        graph[successor].incoming.insert(predecessor);
    }
}

/// Graph compiler that analyzes resource hazards and creates execution plans.
#[derive(Debug)]
pub struct GraphCompiler {
    passes: Vec<PassInfo>,
    dependency_graph: Vec<DependencyNode>,
}

impl GraphCompiler {
    pub fn new(passes: Vec<PassInfo>) -> Self {
        Self {
            passes,
            dependency_graph: Vec::new(),
        }
    }

    pub fn from_pass_descs(passes: &[PassDesc]) -> Self {
        Self::new(passes.iter().map(PassInfo::from).collect())
    }

    /// Build the canonical dependency graph from declaration-order accesses.
    ///
    /// For each resource version, the compiler adds only the hazards required
    /// to preserve observable behavior:
    ///
    /// - RAW: the latest writer must complete before a later reader;
    /// - WAW: the latest writer must complete before a later writer;
    /// - WAR: every reader of the current version must complete before a later
    ///   writer replaces that version.
    ///
    /// Future writers are never treated as producers for earlier reads.
    pub fn analyze_dependencies(&mut self) {
        let mut graph = vec![DependencyNode::default(); self.passes.len()];
        let mut resources: HashMap<ResourceId, ResourceAccessState> = HashMap::new();

        for (pass_index, pass) in self.passes.iter().enumerate() {
            for resource in &pass.reads {
                let last_writer = resources.get(resource).and_then(|state| state.last_writer);
                if let Some(writer) = last_writer {
                    add_dependency(&mut graph, writer, pass_index);
                }

                resources
                    .entry(*resource)
                    .or_default()
                    .readers_since_write
                    .insert(pass_index);
            }

            for resource in &pass.writes {
                let (last_writer, readers) = resources
                    .get(resource)
                    .map(|state| {
                        (
                            state.last_writer,
                            state
                                .readers_since_write
                                .iter()
                                .copied()
                                .collect::<Vec<_>>(),
                        )
                    })
                    .unwrap_or_default();

                if let Some(writer) = last_writer {
                    add_dependency(&mut graph, writer, pass_index);
                }
                for reader in readers {
                    add_dependency(&mut graph, reader, pass_index);
                }

                let state = resources.entry(*resource).or_default();
                state.last_writer = Some(pass_index);
                state.readers_since_write.clear();
            }
        }

        self.dependency_graph = graph;
    }

    /// Perform a stable topological sort on the canonical dependency graph.
    ///
    /// Pass declaration order is used as the tie-breaker whenever several
    /// passes are ready, keeping captures and diagnostics reproducible.
    pub fn topological_sort(&self) -> Result<Vec<usize>, String> {
        let mut in_degree: Vec<usize> = self
            .dependency_graph
            .iter()
            .map(|node| node.incoming.len())
            .collect();
        let mut ready: BTreeSet<usize> = in_degree
            .iter()
            .enumerate()
            .filter_map(|(index, degree)| (*degree == 0).then_some(index))
            .collect();
        let mut sorted = Vec::with_capacity(self.passes.len());

        while let Some(current) = ready.pop_first() {
            sorted.push(current);

            for &successor in &self.dependency_graph[current].outgoing {
                in_degree[successor] -= 1;
                if in_degree[successor] == 0 {
                    ready.insert(successor);
                }
            }
        }

        if sorted.len() == self.passes.len() {
            return Ok(sorted);
        }

        let cycle = self.detect_cycle().unwrap_or_else(|| {
            (0..self.passes.len())
                .filter(|i| !sorted.contains(i))
                .collect()
        });
        let names = cycle
            .iter()
            .map(|&index| self.passes[index].name.as_str())
            .collect::<Vec<_>>()
            .join(" -> ");
        Err(format!("Cycle detected involving passes: {names}"))
    }

    /// Detect a cycle in the dependency graph and return a closed cycle path.
    fn detect_cycle(&self) -> Option<Vec<usize>> {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum VisitState {
            Unvisited,
            Visiting,
            Visited,
        }

        fn dfs(
            node: usize,
            graph: &[DependencyNode],
            state: &mut [VisitState],
            path: &mut Vec<usize>,
        ) -> Option<Vec<usize>> {
            state[node] = VisitState::Visiting;
            path.push(node);

            for &successor in &graph[node].outgoing {
                match state[successor] {
                    VisitState::Visiting => {
                        let cycle_start = path.iter().position(|&entry| entry == successor)?;
                        let mut cycle = path[cycle_start..].to_vec();
                        cycle.push(successor);
                        return Some(cycle);
                    }
                    VisitState::Unvisited => {
                        if let Some(cycle) = dfs(successor, graph, state, path) {
                            return Some(cycle);
                        }
                    }
                    VisitState::Visited => {}
                }
            }

            path.pop();
            state[node] = VisitState::Visited;
            None
        }

        let mut state = vec![VisitState::Unvisited; self.passes.len()];
        let mut path = Vec::new();

        for pass_index in 0..self.passes.len() {
            if state[pass_index] == VisitState::Unvisited
                && let Some(cycle) = dfs(pass_index, &self.dependency_graph, &mut state, &mut path)
            {
                return Some(cycle);
            }
        }

        None
    }

    fn build_execution_metadata(
        &self,
        sorted_passes: &[usize],
    ) -> (Vec<PassDagNode>, Vec<Vec<usize>>) {
        let mut levels = vec![0usize; self.passes.len()];

        for &pass_index in sorted_passes {
            levels[pass_index] = self.dependency_graph[pass_index]
                .incoming
                .iter()
                .map(|&predecessor| levels[predecessor] + 1)
                .max()
                .unwrap_or(0);
        }

        let dag = self
            .passes
            .iter()
            .enumerate()
            .map(|(pass_index, pass)| PassDagNode {
                pass_index,
                reads: pass.reads.clone(),
                writes: pass.writes.clone(),
                predecessors: self.dependency_graph[pass_index]
                    .incoming
                    .iter()
                    .copied()
                    .collect(),
                successors: self.dependency_graph[pass_index]
                    .outgoing
                    .iter()
                    .copied()
                    .collect(),
                level: levels[pass_index],
            })
            .collect();

        let mut parallel_groups = Vec::<Vec<usize>>::new();
        for &pass_index in sorted_passes {
            let level = levels[pass_index];
            if parallel_groups.len() <= level {
                parallel_groups.resize_with(level + 1, Vec::new);
            }
            parallel_groups[level].push(pass_index);
        }

        (dag, parallel_groups)
    }

    /// Compile the render graph into one internally consistent execution plan.
    pub fn compile(mut self) -> Result<ExecutionPlan, RenderGraphError> {
        self.analyze_dependencies();

        if let Some(cycle) = self.detect_cycle() {
            let cycle_names = cycle
                .iter()
                .map(|&index| self.passes[index].name.as_str())
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(RenderGraphError::DependencyCycle(cycle_names));
        }

        let sorted_passes = self
            .topological_sort()
            .map_err(RenderGraphError::DependencyCycle)?;
        let (dag, parallel_groups) = self.build_execution_metadata(&sorted_passes);

        Ok(ExecutionPlan {
            sorted_passes,
            dag,
            parallel_groups,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rid(n: u32) -> ResourceId {
        ResourceId(n)
    }

    fn make_pass(name: &str, reads: Vec<ResourceId>, writes: Vec<ResourceId>) -> PassInfo {
        PassInfo {
            name: name.to_string(),
            reads,
            writes,
        }
    }

    fn compile(passes: Vec<PassInfo>) -> ExecutionPlan {
        GraphCompiler::new(passes).compile().unwrap()
    }

    #[test]
    fn topological_sort_preserves_dependency_chain() {
        let plan = compile(vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![rid(1)]),
            make_pass("C", vec![rid(1)], vec![]),
        ]);

        assert_eq!(plan.sorted_passes, vec![0, 1, 2]);
    }

    #[test]
    fn independent_passes_keep_stable_declaration_order() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![], vec![rid(1)]),
            make_pass("C", vec![], vec![]),
        ];

        for _ in 0..32 {
            assert_eq!(compile(passes.clone()).sorted_passes, vec![0, 1, 2]);
        }
    }

    #[test]
    fn read_before_later_write_is_war_not_a_false_cycle() {
        let plan = compile(vec![
            make_pass("ReadImported", vec![rid(0)], vec![]),
            make_pass("Replace", vec![], vec![rid(0)]),
            make_pass("ReadReplacement", vec![rid(0)], vec![]),
        ]);

        assert_eq!(plan.sorted_passes, vec![0, 1, 2]);
        assert_eq!(plan.dag[0].successors, vec![1]);
        assert_eq!(plan.dag[1].predecessors, vec![0]);
        assert_eq!(plan.dag[1].successors, vec![2]);
        assert_eq!(plan.dag[2].predecessors, vec![1]);
    }

    #[test]
    fn resource_feedback_names_are_versioned_by_declaration_order() {
        let plan = compile(vec![
            make_pass("A", vec![rid(2)], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![rid(1)]),
            make_pass("C", vec![rid(1)], vec![rid(2)]),
        ]);

        assert_eq!(plan.sorted_passes, vec![0, 1, 2]);
        assert_eq!(plan.dag[0].successors, vec![1, 2]);
        assert_eq!(plan.dag[1].successors, vec![2]);
    }

    #[test]
    fn cycle_diagnostics_report_a_closed_stable_path() {
        let mut compiler = GraphCompiler::new(vec![
            make_pass("A", vec![], vec![]),
            make_pass("B", vec![], vec![]),
        ]);
        compiler.dependency_graph = vec![DependencyNode::default(); 2];
        add_dependency(&mut compiler.dependency_graph, 0, 1);
        add_dependency(&mut compiler.dependency_graph, 1, 0);

        assert_eq!(compiler.detect_cycle(), Some(vec![0, 1, 0]));
        assert_eq!(
            compiler.topological_sort().unwrap_err(),
            "Cycle detected involving passes: A -> B -> A"
        );
    }

    #[test]
    fn raw_dependency_links_latest_writer_to_reader() {
        let plan = compile(vec![
            make_pass("Writer", vec![], vec![rid(0)]),
            make_pass("Reader", vec![rid(0)], vec![]),
        ]);

        assert_eq!(plan.dag[0].successors, vec![1]);
        assert_eq!(plan.dag[1].predecessors, vec![0]);
    }

    #[test]
    fn waw_dependency_links_consecutive_writers() {
        let plan = compile(vec![
            make_pass("WriterA", vec![], vec![rid(0)]),
            make_pass("WriterB", vec![], vec![rid(0)]),
        ]);

        assert_eq!(plan.dag[0].successors, vec![1]);
        assert_eq!(plan.dag[1].predecessors, vec![0]);
    }

    #[test]
    fn war_dependency_waits_for_every_reader_of_replaced_version() {
        let plan = compile(vec![
            make_pass("ReaderA", vec![rid(0)], vec![]),
            make_pass("ReaderB", vec![rid(0)], vec![]),
            make_pass("Writer", vec![], vec![rid(0)]),
        ]);

        assert_eq!(plan.dag[2].predecessors, vec![0, 1]);
        assert_eq!(plan.parallel_groups, vec![vec![0, 1], vec![2]]);
    }

    #[test]
    fn writer_chain_uses_minimal_transitive_edges() {
        let plan = compile(vec![
            make_pass("WriterA", vec![], vec![rid(0)]),
            make_pass("WriterB", vec![], vec![rid(0)]),
            make_pass("Reader", vec![rid(0)], vec![]),
        ]);

        assert_eq!(plan.dag[0].successors, vec![1]);
        assert_eq!(plan.dag[1].successors, vec![2]);
        assert_eq!(plan.dag[2].predecessors, vec![1]);
        assert_eq!(plan.sorted_passes, vec![0, 1, 2]);
    }

    #[test]
    fn read_modify_write_pass_does_not_depend_on_itself() {
        let plan = compile(vec![
            make_pass("Writer", vec![], vec![rid(0)]),
            make_pass("ReadModifyWrite", vec![rid(0)], vec![rid(0)]),
            make_pass("Reader", vec![rid(0)], vec![]),
        ]);

        assert_eq!(plan.dag[1].predecessors, vec![0]);
        assert_eq!(plan.dag[1].successors, vec![2]);
    }

    #[test]
    fn diamond_dependencies_create_expected_parallel_groups() {
        let plan = compile(vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![rid(1)]),
            make_pass("C", vec![rid(0)], vec![rid(2)]),
            make_pass("D", vec![rid(1), rid(2)], vec![]),
        ]);

        assert_eq!(plan.sorted_passes, vec![0, 1, 2, 3]);
        assert_eq!(plan.parallel_groups, vec![vec![0], vec![1, 2], vec![3]]);
        assert_eq!(plan.dag[0].level, 0);
        assert_eq!(plan.dag[1].level, 1);
        assert_eq!(plan.dag[2].level, 1);
        assert_eq!(plan.dag[3].level, 2);
    }

    #[test]
    fn execution_plan_views_share_the_same_edges_and_levels() {
        let plan = compile(vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![], vec![rid(1)]),
            make_pass("C", vec![rid(0), rid(1)], vec![rid(2)]),
            make_pass("D", vec![rid(2)], vec![]),
        ]);
        let positions: HashMap<usize, usize> = plan
            .sorted_passes
            .iter()
            .enumerate()
            .map(|(position, &pass)| (pass, position))
            .collect();

        for node in &plan.dag {
            for &predecessor in &node.predecessors {
                assert!(positions[&predecessor] < positions[&node.pass_index]);
                assert!(plan.dag[predecessor].successors.contains(&node.pass_index));
                assert!(plan.dag[predecessor].level < node.level);
            }
        }

        for (level, group) in plan.parallel_groups.iter().enumerate() {
            assert!(group.iter().all(|&pass| plan.dag[pass].level == level));
        }
    }

    #[test]
    fn empty_and_single_pass_graphs_compile() {
        assert!(compile(Vec::new()).sorted_passes.is_empty());

        let plan = compile(vec![make_pass("Solo", vec![], vec![])]);
        assert_eq!(plan.sorted_passes, vec![0]);
        assert_eq!(plan.parallel_groups, vec![vec![0]]);
    }
}
