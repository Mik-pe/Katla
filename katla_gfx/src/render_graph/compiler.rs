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

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use super::error::RenderGraphError;
use super::handles::ResourceId;
use super::pass::PassDesc;

/// Node in the pass dependency DAG.
///
/// Captures the resource reads/writes and predecessor/successor edges for a
/// single pass, along with the topological level used for parallel scheduling.
#[derive(Debug, Clone)]
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

/// First and last live scheduled access to one logical graph resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLifetime {
    pub first_execution_position: usize,
    pub first_pass: usize,
    pub last_execution_position: usize,
    pub last_pass: usize,
}

/// Backend-neutral hazard kind derived from the canonical access DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceHazardKind {
    ReadAfterWrite,
    WriteAfterRead,
    WriteAfterWrite,
}

/// One compiled resource transition between two live passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceTransition {
    pub resource: ResourceId,
    pub from_pass: usize,
    pub to_pass: usize,
    pub hazard: ResourceHazardKind,
}

/// Compiled execution plan for a render graph.
///
/// Contains:
/// - topologically sorted pass indices;
/// - pass dependency DAG with predecessor/successor edges;
/// - parallel groups of passes that can execute concurrently.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Live pass indices in stable topological order.
    pub(super) sorted_passes: Vec<usize>,
    /// Pass dependency DAG nodes indexed by declared pass index.
    pub(super) dag: Vec<PassDagNode>,
    /// Groups of live pass indices that can execute concurrently, ordered by level.
    pub(super) parallel_groups: Vec<Vec<usize>>,
    /// One liveness bit per declared pass.
    pub(super) live_passes: Vec<bool>,
    /// Declared pass indices removed by liveness analysis.
    pub(super) culled_passes: Vec<usize>,
    /// Live resource intervals in canonical execution-order coordinates.
    pub(super) resource_lifetimes: BTreeMap<ResourceId, ResourceLifetime>,
    /// Ordered resource hazards consumed by synchronization planning and diagnostics.
    pub(super) resource_transitions: Vec<ResourceTransition>,
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
    pub side_effect: bool,
}

impl From<&PassDesc> for PassInfo {
    fn from(desc: &PassDesc) -> Self {
        Self {
            name: desc.name.clone(),
            reads: desc.reads.clone(),
            writes: desc.writes.clone(),
            side_effect: desc.side_effect,
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
    data_predecessors: Vec<BTreeSet<usize>>,
    final_writers: HashMap<ResourceId, usize>,
    exported_resources: BTreeSet<ResourceId>,
    culling_enabled: bool,
}

impl GraphCompiler {
    /// Create a compiler that keeps every declared pass live.
    ///
    /// This preserves the focused low-level compiler API. Production frame graphs
    /// use [`Self::with_exports`] so liveness roots are explicit.
    pub fn new(passes: Vec<PassInfo>) -> Self {
        Self {
            passes,
            dependency_graph: Vec::new(),
            data_predecessors: Vec::new(),
            final_writers: HashMap::new(),
            exported_resources: BTreeSet::new(),
            culling_enabled: false,
        }
    }

    /// Create a compiler with explicit externally observable resource roots.
    pub fn with_exports(
        passes: Vec<PassInfo>,
        exported_resources: impl IntoIterator<Item = ResourceId>,
    ) -> Self {
        Self {
            exported_resources: exported_resources.into_iter().collect(),
            culling_enabled: true,
            ..Self::new(passes)
        }
    }

    pub fn from_pass_descs(passes: &[PassDesc]) -> Self {
        Self::new(passes.iter().map(PassInfo::from).collect())
    }

    pub fn from_pass_descs_with_exports(
        passes: &[PassDesc],
        exported_resources: impl IntoIterator<Item = ResourceId>,
    ) -> Self {
        Self::with_exports(
            passes.iter().map(PassInfo::from).collect(),
            exported_resources,
        )
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
        let mut data_predecessors = vec![BTreeSet::new(); self.passes.len()];
        let mut resources: HashMap<ResourceId, ResourceAccessState> = HashMap::new();

        for (pass_index, pass) in self.passes.iter().enumerate() {
            for resource in &pass.reads {
                let last_writer = resources.get(resource).and_then(|state| state.last_writer);
                if let Some(writer) = last_writer {
                    add_dependency(&mut graph, writer, pass_index);
                    data_predecessors[pass_index].insert(writer);
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

        self.final_writers = resources
            .into_iter()
            .filter_map(|(resource, state)| state.last_writer.map(|writer| (resource, writer)))
            .collect();
        self.data_predecessors = data_predecessors;
        self.dependency_graph = graph;
    }

    fn analyze_liveness(&self) -> Vec<bool> {
        if !self.culling_enabled {
            return vec![true; self.passes.len()];
        }

        let mut live = vec![false; self.passes.len()];
        let mut work = VecDeque::new();

        for (pass_index, pass) in self.passes.iter().enumerate() {
            if pass.side_effect {
                work.push_back(pass_index);
            }
        }
        for resource in &self.exported_resources {
            if let Some(&writer) = self.final_writers.get(resource) {
                work.push_back(writer);
            }
        }

        while let Some(pass_index) = work.pop_front() {
            if live[pass_index] {
                continue;
            }
            live[pass_index] = true;
            work.extend(self.data_predecessors[pass_index].iter().copied());
        }

        live
    }

    fn live_dependency_graph(&self, live: &[bool]) -> Vec<DependencyNode> {
        self.dependency_graph
            .iter()
            .enumerate()
            .map(|(pass_index, node)| {
                if !live[pass_index] {
                    return DependencyNode::default();
                }
                DependencyNode {
                    incoming: node
                        .incoming
                        .iter()
                        .copied()
                        .filter(|&index| live[index])
                        .collect(),
                    outgoing: node
                        .outgoing
                        .iter()
                        .copied()
                        .filter(|&index| live[index])
                        .collect(),
                }
            })
            .collect()
    }

    /// Stable Kahn topological sort over live passes only.
    fn topological_sort_live(
        &self,
        graph: &[DependencyNode],
        live: &[bool],
    ) -> Result<Vec<usize>, String> {
        let mut in_degree = graph
            .iter()
            .map(|node| node.incoming.len())
            .collect::<Vec<_>>();
        let mut ready = live
            .iter()
            .enumerate()
            .filter_map(|(index, is_live)| (*is_live && in_degree[index] == 0).then_some(index))
            .collect::<BTreeSet<_>>();
        let expected = live.iter().filter(|&&is_live| is_live).count();
        let mut sorted = Vec::with_capacity(expected);

        while let Some(current) = ready.pop_first() {
            sorted.push(current);
            for &successor in &graph[current].outgoing {
                in_degree[successor] -= 1;
                if in_degree[successor] == 0 {
                    ready.insert(successor);
                }
            }
        }

        if sorted.len() == expected {
            Ok(sorted)
        } else {
            Err("cycle detected in live render-graph passes".to_string())
        }
    }

    /// Perform a stable topological sort over every declared pass for cycle tests.
    #[cfg(test)]
    fn topological_sort(&self) -> Result<Vec<usize>, String> {
        let live = vec![true; self.passes.len()];
        self.topological_sort_live(&self.dependency_graph, &live)
            .map_err(|_| {
                let cycle = self.detect_cycle().unwrap_or_default();
                let names = cycle
                    .iter()
                    .map(|&index| self.passes[index].name.as_str())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                format!("Cycle detected involving passes: {names}")
            })
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

    fn build_resource_lifetimes(
        &self,
        sorted_passes: &[usize],
    ) -> BTreeMap<ResourceId, ResourceLifetime> {
        let mut lifetimes = BTreeMap::<ResourceId, ResourceLifetime>::new();

        for (execution_position, &pass_index) in sorted_passes.iter().enumerate() {
            let pass = &self.passes[pass_index];
            let resources = pass
                .reads
                .iter()
                .chain(&pass.writes)
                .copied()
                .collect::<BTreeSet<_>>();

            for resource in resources {
                lifetimes
                    .entry(resource)
                    .and_modify(|lifetime| {
                        lifetime.last_execution_position = execution_position;
                        lifetime.last_pass = pass_index;
                    })
                    .or_insert(ResourceLifetime {
                        first_execution_position: execution_position,
                        first_pass: pass_index,
                        last_execution_position: execution_position,
                        last_pass: pass_index,
                    });
            }
        }

        lifetimes
    }

    fn append_resource_transitions(
        transitions: &mut Vec<ResourceTransition>,
        left: &[ResourceId],
        right: &[ResourceId],
        from_pass: usize,
        to_pass: usize,
        hazard: ResourceHazardKind,
    ) {
        let right = right.iter().copied().collect::<BTreeSet<_>>();
        transitions.extend(
            left.iter()
                .copied()
                .filter(|resource| right.contains(resource))
                .map(|resource| ResourceTransition {
                    resource,
                    from_pass,
                    to_pass,
                    hazard,
                }),
        );
    }

    fn build_resource_transitions(
        &self,
        dependency_graph: &[DependencyNode],
        sorted_passes: &[usize],
    ) -> Vec<ResourceTransition> {
        let mut transitions = Vec::new();

        for &to_pass in sorted_passes {
            let to = &self.passes[to_pass];
            for &from_pass in &dependency_graph[to_pass].incoming {
                let from = &self.passes[from_pass];
                Self::append_resource_transitions(
                    &mut transitions,
                    &from.writes,
                    &to.reads,
                    from_pass,
                    to_pass,
                    ResourceHazardKind::ReadAfterWrite,
                );
                Self::append_resource_transitions(
                    &mut transitions,
                    &from.reads,
                    &to.writes,
                    from_pass,
                    to_pass,
                    ResourceHazardKind::WriteAfterRead,
                );
                Self::append_resource_transitions(
                    &mut transitions,
                    &from.writes,
                    &to.writes,
                    from_pass,
                    to_pass,
                    ResourceHazardKind::WriteAfterWrite,
                );
            }
        }

        let mut execution_positions = vec![usize::MAX; self.passes.len()];
        for (position, &pass_index) in sorted_passes.iter().enumerate() {
            execution_positions[pass_index] = position;
        }
        transitions.sort_by_key(|transition| {
            (
                execution_positions[transition.to_pass],
                execution_positions[transition.from_pass],
                transition.resource,
                transition.hazard,
            )
        });
        transitions.dedup();
        transitions
    }

    fn build_execution_metadata(
        &self,
        dependency_graph: &[DependencyNode],
        sorted_passes: &[usize],
        live: &[bool],
    ) -> (Vec<PassDagNode>, Vec<Vec<usize>>) {
        let mut levels = vec![0usize; self.passes.len()];

        for &pass_index in sorted_passes {
            levels[pass_index] = dependency_graph[pass_index]
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
                predecessors: dependency_graph[pass_index]
                    .incoming
                    .iter()
                    .copied()
                    .collect(),
                successors: dependency_graph[pass_index]
                    .outgoing
                    .iter()
                    .copied()
                    .collect(),
                level: if live[pass_index] {
                    levels[pass_index]
                } else {
                    0
                },
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

        let live_passes = self.analyze_liveness();
        let live_graph = self.live_dependency_graph(&live_passes);
        let sorted_passes = self
            .topological_sort_live(&live_graph, &live_passes)
            .map_err(RenderGraphError::DependencyCycle)?;
        let (dag, parallel_groups) =
            self.build_execution_metadata(&live_graph, &sorted_passes, &live_passes);
        let culled_passes = live_passes
            .iter()
            .enumerate()
            .filter_map(|(index, live)| (!live).then_some(index))
            .collect();
        let resource_lifetimes = self.build_resource_lifetimes(&sorted_passes);
        let resource_transitions = self.build_resource_transitions(&live_graph, &sorted_passes);

        Ok(ExecutionPlan {
            sorted_passes,
            dag,
            parallel_groups,
            live_passes,
            culled_passes,
            resource_lifetimes,
            resource_transitions,
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
            side_effect: false,
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

    fn compile_with_exports(passes: Vec<PassInfo>, exports: &[ResourceId]) -> ExecutionPlan {
        GraphCompiler::with_exports(passes, exports.iter().copied())
            .compile()
            .unwrap()
    }

    #[test]
    fn culls_dead_branches_from_explicit_exports() {
        let plan = compile_with_exports(
            vec![
                make_pass("live_source", vec![], vec![rid(0)]),
                make_pass("live_present", vec![rid(0)], vec![rid(1)]),
                make_pass("dead_source", vec![], vec![rid(2)]),
                make_pass("dead_consumer", vec![rid(2)], vec![rid(3)]),
            ],
            &[rid(1)],
        );
        assert_eq!(plan.sorted_passes, vec![0, 1]);
        assert_eq!(plan.culled_passes, vec![2, 3]);
        assert_eq!(plan.live_passes, vec![true, true, false, false]);
    }

    #[test]
    fn keeps_shared_producers_for_multiple_live_consumers() {
        let plan = compile_with_exports(
            vec![
                make_pass("shared", vec![], vec![rid(0)]),
                make_pass("left", vec![rid(0)], vec![rid(1)]),
                make_pass("right", vec![rid(0)], vec![rid(2)]),
            ],
            &[rid(1), rid(2)],
        );
        assert_eq!(plan.sorted_passes, vec![0, 1, 2]);
        assert!(plan.culled_passes.is_empty());
    }

    #[test]
    fn side_effect_passes_are_liveness_roots() {
        let mut side_effect = make_pass("timestamp", vec![rid(0)], vec![]);
        side_effect.side_effect = true;
        let plan = compile_with_exports(
            vec![
                make_pass("producer", vec![], vec![rid(0)]),
                side_effect,
                make_pass("dead", vec![], vec![rid(1)]),
            ],
            &[],
        );
        assert_eq!(plan.sorted_passes, vec![0, 1]);
        assert_eq!(plan.culled_passes, vec![2]);
    }

    #[test]
    fn imported_writes_are_not_implicit_side_effects() {
        let plan =
            compile_with_exports(vec![make_pass("write_imported", vec![], vec![rid(7)])], &[]);
        assert!(plan.sorted_passes.is_empty());
        assert_eq!(plan.culled_passes, vec![0]);
    }

    #[test]
    fn fully_culled_graph_has_no_execution_or_parallel_work() {
        let plan = compile_with_exports(
            vec![
                make_pass("a", vec![], vec![rid(0)]),
                make_pass("b", vec![rid(0)], vec![rid(1)]),
            ],
            &[],
        );
        assert!(plan.sorted_passes.is_empty());
        assert!(plan.parallel_groups.is_empty());
        assert_eq!(plan.culled_passes, vec![0, 1]);
    }

    #[test]
    fn compiles_resource_hazards_from_the_live_dependency_dag() {
        let plan = compile(vec![
            make_pass("write", vec![], vec![rid(0)]),
            make_pass("read", vec![rid(0)], vec![]),
            make_pass("replace", vec![], vec![rid(0)]),
            make_pass("read_replacement", vec![rid(0)], vec![]),
        ]);

        assert_eq!(
            plan.resource_transitions,
            vec![
                ResourceTransition {
                    resource: rid(0),
                    from_pass: 0,
                    to_pass: 1,
                    hazard: ResourceHazardKind::ReadAfterWrite,
                },
                ResourceTransition {
                    resource: rid(0),
                    from_pass: 0,
                    to_pass: 2,
                    hazard: ResourceHazardKind::WriteAfterWrite,
                },
                ResourceTransition {
                    resource: rid(0),
                    from_pass: 1,
                    to_pass: 2,
                    hazard: ResourceHazardKind::WriteAfterRead,
                },
                ResourceTransition {
                    resource: rid(0),
                    from_pass: 2,
                    to_pass: 3,
                    hazard: ResourceHazardKind::ReadAfterWrite,
                },
            ]
        );
    }

    #[test]
    fn culled_passes_emit_no_resource_transitions() {
        let plan = compile_with_exports(
            vec![
                make_pass("live", vec![], vec![rid(0)]),
                make_pass("dead_writer", vec![], vec![rid(1)]),
                make_pass("dead_reader", vec![rid(1)], vec![rid(2)]),
            ],
            &[rid(0)],
        );

        assert_eq!(plan.culled_passes, vec![1, 2]);
        assert!(plan.resource_transitions.is_empty());
    }

    #[test]
    fn compiles_live_resource_lifetimes_in_execution_coordinates() {
        let plan = compile(vec![
            make_pass("write", vec![], vec![rid(0)]),
            make_pass("unrelated", vec![], vec![rid(1)]),
            make_pass("read", vec![rid(0)], vec![]),
        ]);

        assert_eq!(
            plan.resource_lifetimes.get(&rid(0)),
            Some(&ResourceLifetime {
                first_execution_position: 0,
                first_pass: 0,
                last_execution_position: 2,
                last_pass: 2,
            })
        );
        assert_eq!(
            plan.resource_lifetimes.get(&rid(1)),
            Some(&ResourceLifetime {
                first_execution_position: 1,
                first_pass: 1,
                last_execution_position: 1,
                last_pass: 1,
            })
        );
    }

    #[test]
    fn culled_resource_accesses_do_not_extend_live_lifetimes() {
        let plan = compile_with_exports(
            vec![
                make_pass("live_writer", vec![], vec![rid(0)]),
                make_pass("live_present", vec![rid(0)], vec![rid(1)]),
                make_pass("dead_reader", vec![rid(0)], vec![rid(2)]),
            ],
            &[rid(1)],
        );

        assert_eq!(plan.culled_passes, vec![2]);
        assert_eq!(
            plan.resource_lifetimes.get(&rid(0)),
            Some(&ResourceLifetime {
                first_execution_position: 0,
                first_pass: 0,
                last_execution_position: 1,
                last_pass: 1,
            })
        );
        assert!(!plan.resource_lifetimes.contains_key(&rid(2)));
    }

    #[test]
    fn replacing_an_export_does_not_keep_dead_previous_version_readers() {
        let plan = compile_with_exports(
            vec![
                make_pass("old_writer", vec![], vec![rid(0)]),
                make_pass("dead_reader", vec![rid(0)], vec![rid(1)]),
                make_pass("final_writer", vec![], vec![rid(0)]),
            ],
            &[rid(0)],
        );
        assert_eq!(plan.sorted_passes, vec![2]);
        assert_eq!(plan.culled_passes, vec![0, 1]);
    }
}
