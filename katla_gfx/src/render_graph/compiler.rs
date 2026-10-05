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
//!
//! The same analysis serves images and buffers, differing only in the range
//! type: image accesses version subresource ranges, buffer accesses version byte
//! ranges, and the two axes are independent so an access to one never orders an
//! access to the other.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

mod access_state;

use access_state::{ResourceAccessState, apply_access};

use super::SyncPlan;
use super::access::{
    BufferAccess, BufferByteRange, ImageAccess, ImageSubresourceRange, ResourceAccessMode,
};
use super::error::RenderGraphError;
use super::handles::ResourceId;
use super::pass::{PassDesc, PassType};
use super::resource::ImportedImageContract;
use super::sync_plan::build_sync_plan;

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

/// Compiled execution plan for a render graph.
///
/// Contains:
/// - topologically sorted pass indices;
/// - pass dependency DAG with predecessor/successor edges;
/// - parallel groups of passes that can execute concurrently;
/// - the synchronization plan compiled from the same typed accesses.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Live pass indices in stable topological order.
    pub(super) sorted_passes: Vec<usize>,
    /// Pass dependency DAG nodes indexed by declared pass index.
    pub(super) dag: Vec<PassDagNode>,
    /// Groups of live pass indices that can execute concurrently, ordered by level.
    pub(super) parallel_groups: Vec<Vec<usize>>,
    /// Whether observable roots selected pass liveness.
    pub(super) culling_enabled: bool,
    /// Actual side-effect and final-export seeds, in stable declaration order.
    pub(super) liveness_roots: Vec<usize>,
    /// Producers of the latest disjoint versions of each exported resource.
    pub(super) final_export_writers: BTreeMap<ResourceId, Vec<usize>>,
    /// One liveness bit per declared pass.
    pub(super) live_passes: Vec<bool>,
    /// Declared pass indices removed by liveness analysis.
    pub(super) culled_passes: Vec<usize>,
    /// Live resource intervals in canonical execution-order coordinates.
    pub(super) resource_lifetimes: BTreeMap<ResourceId, ResourceLifetime>,
    /// Typed image accesses of the live passes, in declaration order. The
    /// allocation planner classifies tile-memory eligibility from these, so
    /// culled passes never influence it.
    pub(super) live_image_accesses: Vec<ImageAccess>,
    /// Synchronization operations derived from the same typed accesses and
    /// edges that produced the dependency DAG.
    pub(super) sync: SyncPlan,
}

fn validate_image_scope(name: &str, access: &ImageAccess) -> Result<(), RenderGraphError> {
    use super::access::{ResourceAccessStage as S, ResourceAccessUsage as U};
    let shader = matches!(
        access.stage,
        S::VertexShader | S::FragmentShader | S::ComputeShader | S::AllGraphics
    );
    let valid = match access.usage {
        U::Sampled => shader && access.mode == ResourceAccessMode::Read,
        U::Storage => shader,
        U::ColorAttachment => access.stage == S::ColorAttachmentOutput,
        U::DepthStencilAttachment => access.stage == S::DepthStencil,
        U::TransferSource => access.stage == S::Transfer && access.mode == ResourceAccessMode::Read,
        U::TransferDestination => {
            access.stage == S::Transfer && access.mode == ResourceAccessMode::Write
        }
        U::Present => access.stage == S::Present && access.mode == ResourceAccessMode::Write,
    };
    if valid && !access.range.is_empty() {
        Ok(())
    } else {
        Err(super::error::GraphValidationError::InvalidImageAccess {
            pass: name.to_owned(),
            resource: access.resource.0,
            reason: format!(
                "{:?} {:?} at {:?} over {:?}",
                access.mode, access.usage, access.stage, access.range
            ),
        }
        .into())
    }
}

/// Dependency graph node.
#[derive(Debug, Clone, Default)]
pub(crate) struct DependencyNode {
    incoming: BTreeSet<usize>,
    outgoing: BTreeSet<usize>,
}

/// Simplified pass info for the compiler (without the execute callback).
#[derive(Debug, Clone)]
pub struct PassInfo {
    pub name: String,
    /// Encoding operation, independent of the diagnostic name.
    pub operation: PassType,
    pub reads: Vec<ResourceId>,
    pub writes: Vec<ResourceId>,
    /// Typed image accesses: the authoritative dependency-analysis input.
    /// The coarse `reads`/`writes` sets are derived from these.
    pub image_accesses: Vec<ImageAccess>,
    /// Typed buffer accesses, analyzed against byte ranges exactly as image
    /// accesses are analyzed against subresource ranges.
    pub buffer_accesses: Vec<BufferAccess>,
    /// Authored attachment content requirements and store effects.
    pub attachment_ops: Vec<(
        ResourceId,
        super::ImageAspects,
        crate::render_pass::AttachmentOps,
    )>,
    pub side_effect: bool,
}

impl From<&PassDesc> for PassInfo {
    fn from(desc: &PassDesc) -> Self {
        Self {
            name: desc.name.clone(),
            operation: desc.pass_type,
            reads: desc.reads.clone(),
            writes: desc.writes.clone(),
            image_accesses: desc.image_accesses.clone(),
            buffer_accesses: desc.buffer_accesses.clone(),
            attachment_ops: desc
                .color_attachments
                .iter()
                .map(|&(resource, ops)| (resource, super::ImageAspects::COLOR, ops))
                .chain(
                    desc.depth_target
                        .zip(desc.depth_attachment)
                        .into_iter()
                        .flat_map(|(resource, ops)| {
                            [
                                (resource, super::ImageAspects::DEPTH, ops.depth),
                                (resource, super::ImageAspects::STENCIL, ops.stencil),
                            ]
                        }),
                )
                .collect(),
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
    pub(crate) passes: Vec<PassInfo>,
    pub(crate) dependency_graph: Vec<DependencyNode>,
    pub(crate) data_predecessors: Vec<BTreeSet<usize>>,
    pub(crate) final_writers: HashMap<ResourceId, BTreeSet<usize>>,
    pub(crate) exported_resources: BTreeSet<ResourceId>,
    pub(crate) imported_contracts: BTreeMap<ResourceId, ImportedImageContract>,
    pub(crate) external_image_accesses: Vec<ImageAccess>,
    pub(crate) external_buffer_accesses: Vec<BufferAccess>,
    pub(crate) external_uploads_pending: bool,
    pub(crate) culling_enabled: bool,
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
            imported_contracts: BTreeMap::new(),
            external_image_accesses: Vec::new(),
            external_buffer_accesses: Vec::new(),
            external_uploads_pending: false,
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
        imported_contracts: BTreeMap<ResourceId, ImportedImageContract>,
    ) -> Self {
        Self {
            imported_contracts,
            ..Self::with_exports(
                passes.iter().map(PassInfo::from).collect(),
                exported_resources,
            )
        }
    }

    /// Build the canonical dependency graph from declared typed accesses.
    ///
    /// For each resource, subresource ranges are versioned by declaration
    /// order and only the hazards required to preserve observable behavior
    /// are added:
    ///
    /// - RAW: the latest writer of overlapping subresources must complete
    ///   before a later reader of them;
    /// - WAW: the latest writer of overlapping subresources must complete
    ///   before a later writer of them;
    /// - WAR: every reader of overlapping subresources must complete before a
    ///   later writer replaces them.
    ///
    /// Accesses to disjoint subresource ranges of one resource are
    /// independent and produce no edges. Read-modify-write accesses act as
    /// writers (with a read of the version they replace) and never produce
    /// self-dependencies. Future writers are never treated as producers for
    /// earlier reads.
    pub fn analyze_dependencies(&mut self) {
        let mut graph = vec![DependencyNode::default(); self.passes.len()];
        let mut data_predecessors = vec![BTreeSet::new(); self.passes.len()];

        // Images and buffers version their ranges separately: an access to one
        // never orders an access to the other, even when both are declared by
        // the same pass.
        let mut image_states: HashMap<ResourceId, ResourceAccessState<ImageSubresourceRange>> =
            HashMap::new();
        let mut buffer_states: HashMap<ResourceId, ResourceAccessState<BufferByteRange>> =
            HashMap::new();

        for (pass_index, pass) in self.passes.iter().enumerate() {
            for access in &pass.image_accesses {
                let state = image_states.entry(access.resource).or_default();
                apply_access(
                    state,
                    pass_index,
                    access.mode,
                    access.range,
                    &mut graph,
                    &mut data_predecessors,
                );
            }

            for access in &pass.buffer_accesses {
                let state = buffer_states.entry(access.resource).or_default();
                apply_access(
                    state,
                    pass_index,
                    access.mode,
                    access.range,
                    &mut graph,
                    &mut data_predecessors,
                );
            }
        }

        self.final_writers.clear();
        for (resource, state) in image_states {
            self.final_writers
                .entry(resource)
                .or_default()
                .extend(state.final_writers());
        }
        for (resource, state) in buffer_states {
            self.final_writers
                .entry(resource)
                .or_default()
                .extend(state.final_writers());
        }

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
            if let Some(writers) = self.final_writers.get(resource) {
                work.extend(writers.iter().copied());
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

    fn validate_imported_attachment_contents(
        &self,
        sorted: &[usize],
    ) -> Result<(), RenderGraphError> {
        use crate::render_pass::{LoadOp, StoreOp};
        let mut defined: BTreeMap<ResourceId, Vec<ImageSubresourceRange>> = self
            .imported_contracts
            .iter()
            .map(|(&resource, contract)| {
                (
                    resource,
                    if contract.initial == super::ResourceState::Undefined {
                        Vec::new()
                    } else {
                        vec![ImageSubresourceRange::whole(super::ImageAspects::ALL)]
                    },
                )
            })
            .collect();
        for &index in sorted {
            let pass = &self.passes[index];
            for &(resource, aspects, ops) in &pass.attachment_ops {
                let Some(regions) = defined.get(&resource) else {
                    continue;
                };
                if ops.load == LoadOp::Load {
                    let mut missing = vec![ImageSubresourceRange::whole(aspects)];
                    for &region in regions {
                        missing = missing
                            .iter()
                            .flat_map(|range| range.subtract(region))
                            .collect();
                    }
                    if !missing.is_empty() {
                        return Err(super::GraphValidationError::LoadingUninitializedImport {
                            pass: pass.name.clone(),
                            resource: resource.0,
                            aspects,
                        }
                        .into());
                    }
                }
            }
            for access in &pass.image_accesses {
                if access.mode.writes()
                    && !matches!(
                        access.usage,
                        super::ResourceAccessUsage::ColorAttachment
                            | super::ResourceAccessUsage::DepthStencilAttachment
                    )
                    && let Some(regions) = defined.get_mut(&access.resource)
                {
                    regions.push(access.range);
                }
            }
            for &(resource, aspects, ops) in &pass.attachment_ops {
                let Some(regions) = defined.get_mut(&resource) else {
                    continue;
                };
                let range = ImageSubresourceRange::whole(aspects);
                if ops.store == StoreOp::DontCare {
                    *regions = regions
                        .iter()
                        .flat_map(|region| region.subtract(range))
                        .collect();
                } else if ops.load == LoadOp::Clear {
                    regions.push(range);
                }
            }
        }
        Ok(())
    }

    /// Compile the render graph into one internally consistent execution plan.
    pub fn compile(mut self) -> Result<ExecutionPlan, RenderGraphError> {
        for pass in &self.passes {
            for access in &pass.image_accesses {
                validate_image_scope(&pass.name, access)?;
            }
        }
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
        self.validate_imported_attachment_contents(&sorted_passes)?;
        let (dag, parallel_groups) =
            self.build_execution_metadata(&live_graph, &sorted_passes, &live_passes);
        let culled_passes = live_passes
            .iter()
            .enumerate()
            .filter_map(|(index, live)| (!live).then_some(index))
            .collect();
        let resource_lifetimes = self.build_resource_lifetimes(&sorted_passes);
        let live_image_accesses = self
            .passes
            .iter()
            .zip(&live_passes)
            .filter(|(_, live)| **live)
            .flat_map(|(pass, _)| pass.image_accesses.iter().copied())
            .collect();
        let sync = build_sync_plan(
            &self.passes,
            &sorted_passes,
            &dag,
            &self.imported_contracts,
            &self.external_image_accesses,
            &self.external_buffer_accesses,
            self.external_uploads_pending,
        );

        let final_export_writers = self
            .exported_resources
            .iter()
            .filter_map(|resource| {
                self.final_writers
                    .get(resource)
                    .map(|writers| (*resource, writers.iter().copied().collect::<Vec<_>>()))
            })
            .collect::<BTreeMap<_, _>>();
        let liveness_roots = self
            .passes
            .iter()
            .enumerate()
            .filter_map(|(index, pass)| pass.side_effect.then_some(index))
            .chain(final_export_writers.values().flatten().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        Ok(ExecutionPlan {
            sorted_passes,
            dag,
            parallel_groups,
            culling_enabled: self.culling_enabled,
            liveness_roots,
            final_export_writers,
            live_passes,
            culled_passes,
            resource_lifetimes,
            live_image_accesses,
            sync,
        })
    }
}

#[cfg(test)]
mod tests;
