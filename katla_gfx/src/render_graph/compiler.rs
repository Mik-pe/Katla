//! Graph compiler for the render graph API.
//!
//! This module provides dependency analysis and execution plan generation.
//! It's purely analytical - no GPU interaction occurs here.
//!
//! # Overview
//!
//! - [`ExecutionPlan`] - Contains sorted pass indices and pre-computed barriers
//! - [`GraphCompiler`] - Analyzes dependencies and creates execution plans
//!
//! # Algorithm
//!
//! 1. **Dependency Analysis**: Build a directed graph from pass read/write relationships
//! 2. **Topological Sort**: Order passes so dependencies execute first
//! 3. **Cycle Detection**: Detect and report cyclic dependencies

//!
//! The frame graph compiler focuses on pass ordering and dependency analysis.
//! Barrier computation is handled separately in the graph execution layer.

//! when transient resources exist.

use std::collections::{HashMap, HashSet, VecDeque};

use itertools::Itertools;

use super::error::RenderGraphError;
use super::handles::ResourceId;
use super::pass::PassDesc;

/// Node in the pass dependency DAG.
///
/// Captures the resource reads/writes and predecessor/successor edges
/// for a single pass, along with the topological level used for
/// parallel scheduling.
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
/// - Topologically sorted pass indices
/// - Pass dependency DAG with predecessor/successor edges
/// - Parallel groups of passes that can execute concurrently
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub(super) sorted_passes: Vec<usize>,
    /// Pass dependency DAG nodes indexed by pass index.
    pub(super) dag: Vec<PassDagNode>,
    /// Groups of pass indices that can execute concurrently, ordered by level.
    pub(super) parallel_groups: Vec<Vec<usize>>,
}

impl ExecutionPlan {
    fn new() -> Self {
        Self {
            sorted_passes: Vec::new(),
            dag: Vec::new(),
            parallel_groups: Vec::new(),
        }
    }
}

/// Dependency graph node.
#[derive(Debug, Clone, Default)]
struct DependencyNode {
    incoming: HashSet<usize>,
    outgoing: HashSet<usize>,
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

/// Build a pass dependency DAG from the pass list.
///
/// For each pair of passes (i, j) where i < j, an edge i→j is added when:
/// - i.writes ∩ j.reads ≠ ∅  (RAW: i produces what j consumes)
/// - i.writes ∩ j.writes ≠ ∅  (WAW: both write the same resource)
/// - i.reads ∩ j.writes ≠ ∅   (WAR: j overwrites what i reads)
///
/// Returns a DAG with topological levels computed via BFS from roots,
/// plus parallel groups (passes at the same level can run concurrently).
fn build_pass_dag(passes: &[PassInfo]) -> (Vec<PassDagNode>, Vec<Vec<usize>>) {
    let n = passes.len();
    let mut predecessors: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    let mut successors: Vec<HashSet<usize>> = vec![HashSet::new(); n];

    for i in 0..n {
        for j in (i + 1)..n {
            let pi_reads: HashSet<_> = passes[i].reads.iter().copied().collect();
            let pi_writes: HashSet<_> = passes[i].writes.iter().copied().collect();
            let pj_reads: HashSet<_> = passes[j].reads.iter().copied().collect();
            let pj_writes: HashSet<_> = passes[j].writes.iter().copied().collect();

            let depends = !pi_writes.is_disjoint(&pj_reads)
                || !pi_writes.is_disjoint(&pj_writes)
                || !pi_reads.is_disjoint(&pj_writes);

            if depends {
                predecessors[j].insert(i);
                successors[i].insert(j);
            }
        }
    }

    // Compute topological levels via BFS from roots (passes with no predecessors).
    let mut levels = vec![0usize; n];
    let mut in_degree: Vec<usize> = predecessors.iter().map(|p| p.len()).collect();
    let mut queue: VecDeque<usize> = VecDeque::new();

    for (i, &degree) in in_degree.iter().enumerate() {
        if degree == 0 {
            queue.push_back(i);
        }
    }

    let mut topo_order = Vec::with_capacity(n);
    while let Some(node) = queue.pop_front() {
        topo_order.push(node);
        for &succ in &successors[node] {
            levels[succ] = levels[succ].max(levels[node] + 1);
            in_degree[succ] -= 1;
            if in_degree[succ] == 0 {
                queue.push_back(succ);
            }
        }
    }

    // Build parallel groups: group passes by their level.
    let max_level = levels.iter().copied().max().unwrap_or(0);
    let mut parallel_groups: Vec<Vec<usize>> = vec![Vec::new(); max_level + 1];
    for (idx, &level) in levels.iter().enumerate() {
        parallel_groups[level].push(idx);
    }
    // Remove empty trailing groups (shouldn't happen, but be safe).
    while parallel_groups.last().is_some_and(|g| g.is_empty()) {
        parallel_groups.pop();
    }

    // Build DAG nodes.
    let dag: Vec<PassDagNode> = passes
        .iter()
        .enumerate()
        .map(|(i, pass)| PassDagNode {
            pass_index: i,
            reads: pass.reads.clone(),
            writes: pass.writes.clone(),
            predecessors: predecessors[i].iter().copied().sorted().collect(),
            successors: successors[i].iter().copied().sorted().collect(),
            level: levels[i],
        })
        .collect();

    (dag, parallel_groups)
}

/// Graph compiler - analyzes dependencies and creates execution plans.
#[derive(Debug)]
pub struct GraphCompiler {
    passes: Vec<PassInfo>,
    /// Maps resource -> pass indices that write to it
    resource_writers: HashMap<ResourceId, Vec<usize>>,
    /// Maps resource -> pass indices that read from it
    resource_readers: HashMap<ResourceId, Vec<usize>>,
    dependency_graph: Vec<DependencyNode>,
}

impl GraphCompiler {
    pub fn new(passes: Vec<PassInfo>) -> Self {
        Self {
            passes,
            resource_writers: HashMap::new(),
            resource_readers: HashMap::new(),
            dependency_graph: Vec::new(),
        }
    }

    pub fn from_pass_descs(passes: &[PassDesc]) -> Self {
        let pass_infos: Vec<PassInfo> = passes.iter().map(PassInfo::from).collect();
        Self::new(pass_infos)
    }

    /// Build the dependency graph from passes.
    ///
    /// Creates edges based on:
    /// - Write -> Read (WAR - Write After Read)
    /// - Read -> Write (RAW - Read After Write)
    /// - Write -> Write (WAW - Write After Write)
    pub fn analyze_dependencies(&mut self) {
        let num_passes = self.passes.len();
        self.dependency_graph = vec![DependencyNode::default(); num_passes];

        for (pass_idx, pass) in self.passes.iter().enumerate() {
            for resource in &pass.writes {
                self.resource_writers
                    .entry(*resource)
                    .or_default()
                    .push(pass_idx);
            }
            for resource in &pass.reads {
                self.resource_readers
                    .entry(*resource)
                    .or_default()
                    .push(pass_idx);
            }
        }

        for (pass_idx, pass) in self.passes.iter().enumerate() {
            for resource in &pass.reads {
                if let Some(writers) = self.resource_writers.get(resource) {
                    for &writer_idx in writers {
                        if writer_idx != pass_idx {
                            self.dependency_graph[writer_idx].outgoing.insert(pass_idx);
                            self.dependency_graph[pass_idx].incoming.insert(writer_idx);
                        }
                    }
                }
            }

            for resource in &pass.writes {
                if let Some(readers) = self.resource_readers.get(resource) {
                    for &reader_idx in readers {
                        if reader_idx != pass_idx && reader_idx < pass_idx {
                            self.dependency_graph[reader_idx].outgoing.insert(pass_idx);
                            self.dependency_graph[pass_idx].incoming.insert(reader_idx);
                        }
                    }
                }

                if let Some(writers) = self.resource_writers.get(resource) {
                    for &writer_idx in writers {
                        if writer_idx != pass_idx && writer_idx < pass_idx {
                            self.dependency_graph[writer_idx].outgoing.insert(pass_idx);
                            self.dependency_graph[pass_idx].incoming.insert(writer_idx);
                        }
                    }
                }
            }
        }
    }

    /// Perform topological sort on the dependency graph.
    ///
    /// Returns `Ok(sorted_indices)` on success, or `Err(cycle_description)` if a cycle is detected.
    pub fn topological_sort(&self) -> Result<Vec<usize>, String> {
        let num_passes = self.passes.len();
        let mut in_degree = vec![0usize; num_passes];

        for node in &self.dependency_graph {
            for &neighbor in &node.outgoing {
                in_degree[neighbor] += 1;
            }
        }

        let mut queue: VecDeque<usize> = VecDeque::new();
        for (idx, &degree) in in_degree.iter().enumerate() {
            if degree == 0 {
                queue.push_back(idx);
            }
        }

        let mut sorted = Vec::with_capacity(num_passes);

        while let Some(current) = queue.pop_front() {
            sorted.push(current);

            for &neighbor in &self.dependency_graph[current].outgoing {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    queue.push_back(neighbor);
                }
            }
        }

        if sorted.len() != num_passes {
            let remaining: Vec<_> = self
                .passes
                .iter()
                .enumerate()
                .filter(|(idx, _)| !sorted.contains(idx))
                .map(|(_, p)| p.name.clone())
                .collect();
            return Err(format!(
                "Cycle detected involving passes: {}",
                remaining.join(", ")
            ));
        }

        Ok(sorted)
    }

    /// Detect cycles in the dependency graph.
    ///
    /// Returns `Some(cycle_path)` if a cycle exists, `None` otherwise.
    pub fn detect_cycle(&self) -> Option<Vec<usize>> {
        let num_passes = self.passes.len();
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum VisitState {
            Unvisited,
            Visiting,
            Visited,
        }

        let mut state = vec![VisitState::Unvisited; num_passes];
        let mut path = Vec::new();

        fn dfs(
            node: usize,
            graph: &[DependencyNode],
            state: &mut [VisitState],
            path: &mut Vec<usize>,
        ) -> Option<Vec<usize>> {
            state[node] = VisitState::Visiting;
            path.push(node);

            for &neighbor in &graph[node].outgoing {
                match state[neighbor] {
                    VisitState::Visiting => {
                        let cycle_start = path.iter().position(|&n| n == neighbor).unwrap_or(0);
                        let cycle: Vec<usize> = path[cycle_start..].to_vec();
                        return Some(cycle);
                    }
                    VisitState::Unvisited => {
                        if let Some(cycle) = dfs(neighbor, graph, state, path) {
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

        for i in 0..num_passes {
            if state[i] == VisitState::Unvisited
                && let Some(cycle) = dfs(i, &self.dependency_graph, &mut state, &mut path)
            {
                return Some(cycle);
            }
        }

        None
    }

    /// Compile the render graph into an execution plan.
    ///
    /// This is the main entry point that:
    /// 1. Analyzes dependencies
    /// 2. Detects cycles
    /// 3. Topologically sorts passes
    pub fn compile(mut self) -> Result<ExecutionPlan, RenderGraphError> {
        self.analyze_dependencies();

        if let Some(cycle) = self.detect_cycle() {
            let cycle_names: Vec<_> = cycle
                .iter()
                .map(|&idx| self.passes[idx].name.as_str())
                .collect();
            return Err(RenderGraphError::DependencyCycle(cycle_names.join(" -> ")));
        }

        let sorted_passes = self
            .topological_sort()
            .map_err(RenderGraphError::DependencyCycle)?;

        let (dag, parallel_groups) = build_pass_dag(&self.passes);

        let mut plan = ExecutionPlan::new();
        plan.sorted_passes = sorted_passes;
        plan.dag = dag;
        plan.parallel_groups = parallel_groups;

        Ok(plan)
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

    #[test]
    fn test_topological_sort_simple() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![rid(1)]),
            make_pass("C", vec![rid(1)], vec![]),
        ];

        let mut compiler = GraphCompiler::new(passes);
        compiler.analyze_dependencies();
        let sorted = compiler.topological_sort().unwrap();

        assert_eq!(sorted.len(), 3);
        let a_pos = sorted.iter().position(|&i| i == 0).unwrap();
        let b_pos = sorted.iter().position(|&i| i == 1).unwrap();
        let c_pos = sorted.iter().position(|&i| i == 2).unwrap();
        assert!(a_pos < b_pos, "A should come before B");
        assert!(b_pos < c_pos, "B should come before C");
    }

    #[test]
    fn test_topological_sort_independent_passes() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![], vec![rid(1)]),
            make_pass("C", vec![], vec![]),
        ];

        let mut compiler = GraphCompiler::new(passes);
        compiler.analyze_dependencies();
        let sorted = compiler.topological_sort().unwrap();

        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn test_cycle_detection_no_cycle() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![]),
        ];

        let mut compiler = GraphCompiler::new(passes);
        compiler.analyze_dependencies();

        assert!(compiler.detect_cycle().is_none());
    }

    #[test]
    fn test_cycle_detection_with_cycle() {
        let passes = vec![
            PassInfo {
                name: "A".to_string(),
                reads: vec![rid(1)],
                writes: vec![rid(0)],
            },
            PassInfo {
                name: "B".to_string(),
                reads: vec![rid(0)],
                writes: vec![rid(1)],
            },
        ];

        let mut compiler = GraphCompiler::new(passes);
        compiler.analyze_dependencies();

        assert!(compiler.detect_cycle().is_some() || compiler.topological_sort().is_err());
    }

    #[test]
    fn test_compile_full_workflow() {
        let passes = vec![
            make_pass("Geometry", vec![], vec![rid(0)]),
            make_pass("Lighting", vec![rid(0)], vec![rid(1)]),
            make_pass("PostProcess", vec![rid(1)], vec![]),
        ];

        let compiler = GraphCompiler::new(passes);
        let plan = compiler.compile().unwrap();

        assert_eq!(plan.sorted_passes.len(), 3);

        let geo_pos = plan.sorted_passes.iter().position(|&i| i == 0).unwrap();
        let light_pos = plan.sorted_passes.iter().position(|&i| i == 1).unwrap();
        let post_pos = plan.sorted_passes.iter().position(|&i| i == 2).unwrap();

        assert!(geo_pos < light_pos);
        assert!(light_pos < post_pos);
    }

    #[test]
    fn test_execution_plan_accessors() {
        let passes = vec![make_pass("A", vec![], vec![rid(0)])];
        let compiler = GraphCompiler::new(passes);
        let plan = compiler.compile().unwrap();

        assert_eq!(plan.sorted_passes, &[0]);
    }

    #[test]
    fn test_complex_dependency_chain() {
        let passes = vec![
            make_pass("Shadow", vec![], vec![rid(0)]),
            make_pass("Geometry", vec![rid(0)], vec![rid(1), rid(2)]),
            make_pass("Lighting", vec![rid(0), rid(1)], vec![rid(3)]),
            make_pass("PostProcess", vec![rid(3)], vec![]),
        ];

        let compiler = GraphCompiler::new(passes);
        let plan = compiler.compile().unwrap();

        assert_eq!(plan.sorted_passes.len(), 4);

        let shadow_pos = plan.sorted_passes.iter().position(|&i| i == 0).unwrap();
        let geo_pos = plan.sorted_passes.iter().position(|&i| i == 1).unwrap();
        let light_pos = plan.sorted_passes.iter().position(|&i| i == 2).unwrap();
        let post_pos = plan.sorted_passes.iter().position(|&i| i == 3).unwrap();

        assert!(shadow_pos < geo_pos, "Shadow should come before Geometry");
        assert!(geo_pos < light_pos, "Geometry should come before Lighting");
        assert!(
            light_pos < post_pos,
            "Lighting should come before PostProcess"
        );
    }

    #[test]
    fn test_diamond_dependency() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![rid(1)]),
            make_pass("C", vec![rid(0)], vec![rid(2)]),
            make_pass("D", vec![rid(1), rid(2)], vec![]),
        ];

        let compiler = GraphCompiler::new(passes);
        let plan = compiler.compile().unwrap();

        assert_eq!(plan.sorted_passes.len(), 4);

        let a_pos = plan.sorted_passes.iter().position(|&i| i == 0).unwrap();
        let b_pos = plan.sorted_passes.iter().position(|&i| i == 1).unwrap();
        let c_pos = plan.sorted_passes.iter().position(|&i| i == 2).unwrap();
        let d_pos = plan.sorted_passes.iter().position(|&i| i == 3).unwrap();

        assert!(a_pos < b_pos, "A should come before B");
        assert!(a_pos < c_pos, "A should come before C");
        assert!(b_pos < d_pos, "B should come before D");
        assert!(c_pos < d_pos, "C should come before D");
    }

    #[test]
    fn test_duplicate_resource_writers() {
        let passes = vec![
            make_pass("Writer1", vec![], vec![rid(0)]),
            make_pass("Writer2", vec![], vec![rid(0)]),
            make_pass("Reader", vec![rid(0)], vec![]),
        ];

        let compiler = GraphCompiler::new(passes);
        let plan = compiler.compile().unwrap();

        assert_eq!(plan.sorted_passes.len(), 3);

        let w1_pos = plan.sorted_passes.iter().position(|&i| i == 0).unwrap();
        let w2_pos = plan.sorted_passes.iter().position(|&i| i == 1).unwrap();
        let r_pos = plan.sorted_passes.iter().position(|&i| i == 2).unwrap();

        // Both writers should come before the reader
        assert!(w1_pos < r_pos, "Writer1 should come before Reader");
        assert!(w2_pos < r_pos, "Writer2 should come before Reader");
        // Writer2 should come after Writer1 (WAW dependency)
        assert!(w1_pos < w2_pos, "Writer1 should come before Writer2");
    }

    #[test]
    fn test_single_pass_no_deps() {
        let passes = vec![make_pass("solo", vec![], vec![])];
        let compiler = GraphCompiler::new(passes);
        let plan = compiler.compile().unwrap();
        assert_eq!(plan.sorted_passes.len(), 1);
        assert_eq!(plan.sorted_passes[0], 0);
    }

    #[test]
    fn test_self_read_write_no_cycle() {
        let passes = vec![make_pass("self_loop", vec![rid(0)], vec![rid(0)])];
        let compiler = GraphCompiler::new(passes);
        let plan = compiler.compile().unwrap();
        assert_eq!(plan.sorted_passes.len(), 1);
    }

    #[test]
    fn test_three_way_cycle() {
        let passes = vec![
            make_pass("A", vec![rid(2)], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![rid(1)]),
            make_pass("C", vec![rid(1)], vec![rid(2)]),
        ];

        let result = GraphCompiler::new(passes).compile();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Cycle"));
    }

    // --- DAG tests ---

    #[test]
    fn test_dag_raw_write_read_edge() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![]),
        ];

        let (dag, _groups) = build_pass_dag(&passes);

        assert_eq!(dag[0].successors, vec![1usize]);
        assert!(dag[0].predecessors.is_empty());
        assert_eq!(dag[1].predecessors, vec![0usize]);
        assert!(dag[1].successors.is_empty());
    }

    #[test]
    fn test_dag_independent_passes_same_group() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![], vec![rid(1)]),
        ];

        let (dag, groups) = build_pass_dag(&passes);

        assert!(dag[0].successors.is_empty());
        assert!(dag[1].successors.is_empty());
        assert!(dag[0].predecessors.is_empty());
        assert!(dag[1].predecessors.is_empty());
        assert_eq!(groups.len(), 1, "Independent passes should be in one group");
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_dag_fan_in_parallel() {
        // A writes X, B writes Y, C reads X+Y → A+B at level 0, C at level 1
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![], vec![rid(1)]),
            make_pass("C", vec![rid(0), rid(1)], vec![]),
        ];

        let (dag, groups) = build_pass_dag(&passes);

        assert_eq!(dag[0].level, 0);
        assert_eq!(dag[1].level, 0);
        assert_eq!(dag[2].level, 1);
        assert_eq!(dag[2].predecessors, vec![0, 1]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 2, "Level 0: A and B in parallel");
        assert!(groups[0].contains(&0));
        assert!(groups[0].contains(&1));
        assert_eq!(groups[1], vec![2], "Level 1: C after A and B");
    }

    #[test]
    fn test_dag_waw_edge() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![], vec![rid(0)]),
        ];

        let (dag, _groups) = build_pass_dag(&passes);

        assert_eq!(dag[0].successors, vec![1]);
        assert_eq!(dag[1].predecessors, vec![0]);
    }

    #[test]
    fn test_dag_war_edge() {
        let passes = vec![
            make_pass("A", vec![rid(0)], vec![]),
            make_pass("B", vec![], vec![rid(0)]),
        ];

        let (dag, _groups) = build_pass_dag(&passes);

        assert_eq!(dag[0].successors, vec![1]);
        assert_eq!(dag[1].predecessors, vec![0]);
    }

    #[test]
    fn test_dag_levels_chain() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![rid(1)]),
            make_pass("C", vec![rid(1)], vec![]),
        ];

        let (dag, groups) = build_pass_dag(&passes);

        assert_eq!(dag[0].level, 0);
        assert_eq!(dag[1].level, 1);
        assert_eq!(dag[2].level, 2);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1], vec![1]);
        assert_eq!(groups[2], vec![2]);
    }

    #[test]
    fn test_dag_diamond() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![rid(1)]),
            make_pass("C", vec![rid(0)], vec![rid(2)]),
            make_pass("D", vec![rid(1), rid(2)], vec![]),
        ];

        let (dag, groups) = build_pass_dag(&passes);

        assert_eq!(dag[0].level, 0);
        assert_eq!(dag[1].level, 1);
        assert_eq!(dag[2].level, 1);
        assert_eq!(dag[3].level, 2);

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1].len(), 2);
        assert!(groups[1].contains(&1));
        assert!(groups[1].contains(&2));
        assert_eq!(groups[2], vec![3]);
    }

    #[test]
    fn test_dag_single_pass() {
        let passes = vec![make_pass("solo", vec![], vec![])];

        let (dag, groups) = build_pass_dag(&passes);

        assert_eq!(dag.len(), 1);
        assert_eq!(dag[0].level, 0);
        assert!(dag[0].predecessors.is_empty());
        assert!(dag[0].successors.is_empty());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec![0]);
    }

    #[test]
    fn test_dag_populated_in_execution_plan() {
        let passes = vec![
            make_pass("A", vec![], vec![rid(0)]),
            make_pass("B", vec![rid(0)], vec![rid(1)]),
            make_pass("C", vec![rid(1)], vec![]),
        ];

        let plan = GraphCompiler::new(passes).compile().unwrap();

        assert_eq!(plan.dag.len(), 3);
        assert_eq!(plan.parallel_groups.len(), 3);
        assert_eq!(plan.dag[0].reads, Vec::<ResourceId>::new());
        assert_eq!(plan.dag[0].writes, vec![rid(0)]);
        assert_eq!(plan.dag[1].reads, vec![rid(0)]);
        assert_eq!(plan.dag[1].writes, vec![rid(1)]);
    }
}
