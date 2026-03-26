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

use super::error::RenderGraphError;
use super::pass::PassDesc;

/// Compiled execution plan for a render graph.
///
/// Contains:
/// - Topologically sorted pass indices
/// - Resource state tracking info
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    sorted_passes: Vec<usize>,
}

impl ExecutionPlan {
    fn new() -> Self {
        Self {
            sorted_passes: Vec::new(),
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
    pub reads: Vec<String>,
    pub writes: Vec<String>,
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

/// Graph compiler - analyzes dependencies and creates execution plans.
#[derive(Debug)]
pub struct GraphCompiler {
    passes: Vec<PassInfo>,
    /// Maps resource name -> pass indices that write to it
    resource_writers: HashMap<String, Vec<usize>>,
    /// Maps resource name -> pass indices that read from it
    resource_readers: HashMap<String, Vec<usize>>,
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
                    .entry(resource.clone())
                    .or_default()
                    .push(pass_idx);
            }
            for resource in &pass.reads {
                self.resource_readers
                    .entry(resource.clone())
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

        let mut plan = ExecutionPlan::new();
        plan.sorted_passes = sorted_passes;

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pass(name: &str, reads: Vec<&str>, writes: Vec<&str>) -> PassInfo {
        PassInfo {
            name: name.to_string(),
            reads: reads.iter().map(|s| s.to_string()).collect(),
            writes: writes.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_topological_sort_simple() {
        let passes = vec![
            make_pass("A", vec![], vec!["r0"]),
            make_pass("B", vec!["r0"], vec!["r1"]),
            make_pass("C", vec!["r1"], vec![]),
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
            make_pass("A", vec![], vec!["r0"]),
            make_pass("B", vec![], vec!["r1"]),
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
            make_pass("A", vec![], vec!["r0"]),
            make_pass("B", vec!["r0"], vec![]),
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
                reads: vec!["r1".to_string()],
                writes: vec!["r0".to_string()],
            },
            PassInfo {
                name: "B".to_string(),
                reads: vec!["r0".to_string()],
                writes: vec!["r1".to_string()],
            },
        ];

        let mut compiler = GraphCompiler::new(passes);
        compiler.analyze_dependencies();

        assert!(compiler.detect_cycle().is_some() || compiler.topological_sort().is_err());
    }

    #[test]
    fn test_compile_full_workflow() {
        let passes = vec![
            make_pass("Geometry", vec![], vec!["r0"]),
            make_pass("Lighting", vec!["r0"], vec!["r1"]),
            make_pass("PostProcess", vec!["r1"], vec![]),
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
        let passes = vec![make_pass("A", vec![], vec!["r0"])];
        let compiler = GraphCompiler::new(passes);
        let plan = compiler.compile().unwrap();

        assert_eq!(plan.sorted_passes, &[0]);
    }

    #[test]
    fn test_complex_dependency_chain() {
        let passes = vec![
            make_pass("Shadow", vec![], vec!["r0"]),
            make_pass("Geometry", vec!["r0"], vec!["r1", "r2"]),
            make_pass("Lighting", vec!["r0", "r1"], vec!["r3"]),
            make_pass("PostProcess", vec!["r3"], vec![]),
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
        // Diamond: A -> B, A -> C, B -> D, C -> D
        // Verify D comes after both B and C in topological order
        let passes = vec![
            make_pass("A", vec![], vec!["r_a"]),
            make_pass("B", vec!["r_a"], vec!["r_b"]),
            make_pass("C", vec!["r_a"], vec!["r_c"]),
            make_pass("D", vec!["r_b", "r_c"], vec![]),
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
        // Two passes writing the same resource: last writer should create WAW dependency
        let passes = vec![
            make_pass("Writer1", vec![], vec!["shared"]),
            make_pass("Writer2", vec![], vec!["shared"]),
            make_pass("Reader", vec!["shared"], vec![]),
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
        // A pass that reads and writes the same resource should NOT create a cycle
        let passes = vec![make_pass("self_loop", vec!["r"], vec!["r"])];
        let compiler = GraphCompiler::new(passes);
        let plan = compiler.compile().unwrap();
        assert_eq!(plan.sorted_passes.len(), 1);
    }

    #[test]
    fn test_three_way_cycle() {
        // A -> B -> C -> A (three-node cycle)
        let passes = vec![
            make_pass("A", vec!["r_c"], vec!["r_a"]),
            make_pass("B", vec!["r_a"], vec!["r_b"]),
            make_pass("C", vec!["r_b"], vec!["r_c"]),
        ];

        let result = GraphCompiler::new(passes).compile();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Cycle"));
    }
}
