//! Graph compiler for the render graph API.
//!
//! This module provides dependency analysis and execution plan generation.
//! It's purely analytical - no GPU interaction occurs here.
//!
//! # Overview
//!
//! - [`ExecutionPlan`] - Contains sorted pass indices and computed barriers
//! - [`GraphCompiler`] - Analyzes dependencies and creates execution plans
//! - [`ResourceBarrier`] - Describes a resource state transition
//!
//! # Algorithm
//!
//! 1. **Dependency Analysis**: Build a directed graph from pass read/write relationships
//! 2. **Topological Sort**: Order passes so dependencies execute first
//! 3. **Cycle Detection**: Detect and report cyclic dependencies
//! 4. **Barrier Computation**: Calculate required resource barriers between passes

use std::collections::{HashMap, HashSet, VecDeque};

use super::error::RenderGraphError;
use super::pass::{PassDesc, PassType};
use super::resource::{GraphResourceHandle, ResourceState};

/// Describes a resource barrier between passes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceBarrier {
    pub resource: GraphResourceHandle,
    pub src_state: ResourceState,
    pub dst_state: ResourceState,
    pub src_pass: Option<usize>,
    pub dst_pass: usize,
}

impl ResourceBarrier {
    pub fn new(
        resource: GraphResourceHandle,
        src_state: ResourceState,
        dst_state: ResourceState,
        src_pass: Option<usize>,
        dst_pass: usize,
    ) -> Self {
        Self {
            resource,
            src_state,
            dst_state,
            src_pass,
            dst_pass,
        }
    }
}

/// Compiled execution plan for a render graph.
///
/// Contains:
/// - Topologically sorted pass indices
/// - Pre-computed barriers for each pass
/// - Resource state tracking info
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    sorted_passes: Vec<usize>,
    barriers: HashMap<usize, Vec<ResourceBarrier>>,
    resource_initial_states: HashMap<GraphResourceHandle, ResourceState>,
}

impl ExecutionPlan {
    fn new() -> Self {
        Self {
            sorted_passes: Vec::new(),
            barriers: HashMap::new(),
            resource_initial_states: HashMap::new(),
        }
    }

    pub fn sorted_passes(&self) -> &[usize] {
        &self.sorted_passes
    }

    pub fn barriers_for_pass(&self, pass_index: usize) -> Option<&[ResourceBarrier]> {
        self.barriers.get(&pass_index).map(Vec::as_slice)
    }

    pub fn all_barriers(&self) -> &HashMap<usize, Vec<ResourceBarrier>> {
        &self.barriers
    }

    pub fn resource_initial_states(&self) -> &HashMap<GraphResourceHandle, ResourceState> {
        &self.resource_initial_states
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
    pub reads: Vec<GraphResourceHandle>,
    pub writes: Vec<GraphResourceHandle>,
    pub pass_type: PassType,
}

impl From<&PassDesc> for PassInfo {
    fn from(desc: &PassDesc) -> Self {
        Self {
            name: desc.name.clone(),
            reads: desc.reads.clone(),
            writes: desc.writes.clone(),
            pass_type: desc.pass_type,
        }
    }
}

/// Graph compiler - analyzes dependencies and creates execution plans.
#[derive(Debug)]
pub struct GraphCompiler {
    passes: Vec<PassInfo>,
    resource_writers: HashMap<GraphResourceHandle, Vec<usize>>,
    resource_readers: HashMap<GraphResourceHandle, Vec<usize>>,
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
            if state[i] == VisitState::Unvisited {
                if let Some(cycle) = dfs(i, &self.dependency_graph, &mut state, &mut path) {
                    return Some(cycle);
                }
            }
        }

        None
    }

    /// Compute barriers for resource state transitions.
    ///
    /// Analyzes resource usage across passes and generates barriers
    /// for each state transition.
    pub fn compute_barriers(
        &self,
        sorted_passes: &[usize],
        resource_states: &HashMap<GraphResourceHandle, ResourceState>,
    ) -> HashMap<usize, Vec<ResourceBarrier>> {
        let mut barriers: HashMap<usize, Vec<ResourceBarrier>> = HashMap::new();
        let mut current_states: HashMap<GraphResourceHandle, ResourceState> =
            resource_states.iter().map(|(&r, &s)| (r, s)).collect();

        for &pass_idx in sorted_passes {
            let pass = &self.passes[pass_idx];
            let mut pass_barriers = Vec::new();

            for &resource in &pass.reads {
                let dst_state = ResourceState::ShaderRead;
                if let Some(&src_state) = current_states.get(&resource) {
                    if src_state != dst_state && src_state != ResourceState::Undefined {
                        pass_barriers.push(ResourceBarrier::new(
                            resource, src_state, dst_state, None, pass_idx,
                        ));
                    }
                }
                current_states.insert(resource, dst_state);
            }

            for &resource in &pass.writes {
                let dst_state = match pass.pass_type {
                    PassType::Graphics => ResourceState::ColorAttachment,
                    PassType::Compute => ResourceState::ShaderWrite,
                    PassType::Transfer => ResourceState::TransferDst,
                };

                if let Some(&src_state) = current_states.get(&resource) {
                    if src_state != dst_state && src_state != ResourceState::Undefined {
                        pass_barriers.push(ResourceBarrier::new(
                            resource, src_state, dst_state, None, pass_idx,
                        ));
                    }
                }
                current_states.insert(resource, dst_state);
            }

            if !pass_barriers.is_empty() {
                barriers.insert(pass_idx, pass_barriers);
            }
        }

        barriers
    }

    /// Compile the render graph into an execution plan.
    ///
    /// This is the main entry point that:
    /// 1. Analyzes dependencies
    /// 2. Detects cycles
    /// 3. Topologically sorts passes
    /// 4. Computes barriers
    pub fn compile(
        mut self,
        resource_states: &HashMap<GraphResourceHandle, ResourceState>,
    ) -> Result<ExecutionPlan, RenderGraphError> {
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

        let barriers = self.compute_barriers(&sorted_passes, resource_states);

        let mut plan = ExecutionPlan::new();
        plan.sorted_passes = sorted_passes;
        plan.barriers = barriers;
        plan.resource_initial_states = resource_states.iter().map(|(&r, &s)| (r, s)).collect();

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_resource(id: u32) -> GraphResourceHandle {
        GraphResourceHandle::new(id)
    }

    fn make_pass(
        name: &str,
        reads: Vec<GraphResourceHandle>,
        writes: Vec<GraphResourceHandle>,
    ) -> PassInfo {
        PassInfo {
            name: name.to_string(),
            reads,
            writes,
            pass_type: PassType::Graphics,
        }
    }

    #[test]
    fn test_topological_sort_simple() {
        let r0 = make_resource(0);
        let r1 = make_resource(1);

        let passes = vec![
            make_pass("A", vec![], vec![r0]),
            make_pass("B", vec![r0], vec![r1]),
            make_pass("C", vec![r1], vec![]),
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
        let r0 = make_resource(0);
        let r1 = make_resource(1);

        let passes = vec![
            make_pass("A", vec![], vec![r0]),
            make_pass("B", vec![], vec![r1]),
            make_pass("C", vec![], vec![]),
        ];

        let mut compiler = GraphCompiler::new(passes);
        compiler.analyze_dependencies();
        let sorted = compiler.topological_sort().unwrap();

        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn test_cycle_detection_no_cycle() {
        let r0 = make_resource(0);

        let passes = vec![
            make_pass("A", vec![], vec![r0]),
            make_pass("B", vec![r0], vec![]),
        ];

        let mut compiler = GraphCompiler::new(passes);
        compiler.analyze_dependencies();

        assert!(compiler.detect_cycle().is_none());
    }

    #[test]
    fn test_cycle_detection_with_cycle() {
        let r0 = make_resource(0);
        let r1 = make_resource(1);

        let passes = vec![
            PassInfo {
                name: "A".to_string(),
                reads: vec![r1],
                writes: vec![r0],
                pass_type: PassType::Compute,
            },
            PassInfo {
                name: "B".to_string(),
                reads: vec![r0],
                writes: vec![r1],
                pass_type: PassType::Compute,
            },
        ];

        let mut compiler = GraphCompiler::new(passes);
        compiler.analyze_dependencies();

        assert!(compiler.detect_cycle().is_some() || compiler.topological_sort().is_err());
    }

    #[test]
    fn test_barrier_computation() {
        let r0 = make_resource(0);

        let passes = vec![
            make_pass("A", vec![], vec![r0]),
            make_pass("B", vec![r0], vec![]),
        ];

        let mut compiler = GraphCompiler::new(passes);
        compiler.analyze_dependencies();
        let sorted = compiler.topological_sort().unwrap();

        let mut initial_states = HashMap::new();
        initial_states.insert(r0, ResourceState::Undefined);

        let barriers = compiler.compute_barriers(&sorted, &initial_states);

        let pass_b_barriers = barriers.get(&1);
        if let Some(barriers) = pass_b_barriers {
            assert!(barriers.iter().any(|b| b.resource == r0));
        }
    }

    #[test]
    fn test_barrier_state_transition() {
        let r0 = make_resource(0);

        let barrier = ResourceBarrier::new(
            r0,
            ResourceState::ColorAttachment,
            ResourceState::ShaderRead,
            Some(0),
            1,
        );

        assert_eq!(barrier.resource, r0);
        assert_eq!(barrier.src_state, ResourceState::ColorAttachment);
        assert_eq!(barrier.dst_state, ResourceState::ShaderRead);
        assert_eq!(barrier.src_pass, Some(0));
        assert_eq!(barrier.dst_pass, 1);
    }

    #[test]
    fn test_compile_full_workflow() {
        let r0 = make_resource(0);
        let r1 = make_resource(1);

        let passes = vec![
            make_pass("Geometry", vec![], vec![r0]),
            make_pass("Lighting", vec![r0], vec![r1]),
            make_pass("PostProcess", vec![r1], vec![]),
        ];

        let compiler = GraphCompiler::new(passes);

        let mut initial_states = HashMap::new();
        initial_states.insert(r0, ResourceState::Undefined);
        initial_states.insert(r1, ResourceState::Undefined);

        let plan = compiler.compile(&initial_states).unwrap();

        assert_eq!(plan.sorted_passes().len(), 3);

        let geo_pos = plan.sorted_passes().iter().position(|&i| i == 0).unwrap();
        let light_pos = plan.sorted_passes().iter().position(|&i| i == 1).unwrap();
        let post_pos = plan.sorted_passes().iter().position(|&i| i == 2).unwrap();

        assert!(geo_pos < light_pos);
        assert!(light_pos < post_pos);
    }

    #[test]
    fn test_execution_plan_accessors() {
        let r0 = make_resource(0);

        let passes = vec![make_pass("A", vec![], vec![r0])];

        let compiler = GraphCompiler::new(passes);

        let mut initial_states = HashMap::new();
        initial_states.insert(r0, ResourceState::Undefined);

        let plan = compiler.compile(&initial_states).unwrap();

        assert_eq!(plan.sorted_passes(), &[0]);
        assert!(plan.resource_initial_states().contains_key(&r0));
    }

    #[test]
    fn test_complex_dependency_chain() {
        let r0 = make_resource(0);
        let r1 = make_resource(1);
        let r2 = make_resource(2);
        let r3 = make_resource(3);

        let passes = vec![
            make_pass("Shadow", vec![], vec![r0]),
            make_pass("Geometry", vec![r0], vec![r1, r2]),
            make_pass("Lighting", vec![r0, r1], vec![r3]),
            make_pass("PostProcess", vec![r3], vec![]),
        ];

        let compiler = GraphCompiler::new(passes);

        let mut initial_states = HashMap::new();
        initial_states.insert(r0, ResourceState::Undefined);
        initial_states.insert(r1, ResourceState::Undefined);
        initial_states.insert(r2, ResourceState::Undefined);
        initial_states.insert(r3, ResourceState::Undefined);

        let plan = compiler.compile(&initial_states).unwrap();

        assert_eq!(plan.sorted_passes().len(), 4);

        let shadow_pos = plan.sorted_passes().iter().position(|&i| i == 0).unwrap();
        let geo_pos = plan.sorted_passes().iter().position(|&i| i == 1).unwrap();
        let light_pos = plan.sorted_passes().iter().position(|&i| i == 2).unwrap();
        let post_pos = plan.sorted_passes().iter().position(|&i| i == 3).unwrap();

        assert!(shadow_pos < geo_pos, "Shadow should come before Geometry");
        assert!(geo_pos < light_pos, "Geometry should come before Lighting");
        assert!(
            light_pos < post_pos,
            "Lighting should come before PostProcess"
        );
    }
}
