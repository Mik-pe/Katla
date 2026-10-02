mod allocation;
mod buffers;
mod builder;
mod execution;

use allocation::TransientAllocationContract;
use buffers::validate_buffer_access_descriptor;
pub use builder::FrameGraphBuilder;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::access::{BufferByteRange, BufferUsage, ResourceAccessMode, ResourceAccessStage};
use super::allocation_plan::TransientAllocationPlan;
use super::backend::RenderGraphBackend;
use super::builder::{InternalPassBuilder, PassBuilder};
use super::compiler::{ExecutionPlan, GraphCompiler};
use super::error::{GraphValidationError, RenderGraphError};
use super::handles::{PassId, ResourceId};
use super::pass::{PassDesc, PassType};
use super::resource::{
    BufferDesc, BufferMemoryPolicy, BufferUsages, GraphBufferDesc, GraphResourceDesc,
    GraphResourceHandle, ImportedImageContract, ResourceState,
};
use crate::handle::{BufferHandle, TextureHandle};
use crate::render_pass::{ClearValue, DepthStencilAttachmentOps, LoadOp};

const BACKBUFFER_NAME: &str = super::BACKBUFFER_NAME;

/// Default state contract for the built-in backbuffer: contents from before
/// the graph (the previously presented frame) are observable, so a pass may
/// load them without an in-graph producer. Applications that present the
/// backbuffer override this with `FrameGraphBuilder::backbuffer_contract` to
/// also require the final `PresentSrc` state.
const DEFAULT_BACKBUFFER_CONTRACT: ImportedImageContract =
    ImportedImageContract::arrives_in(ResourceState::ColorAttachment);

/// What kind of render target a written graph resource resolves to.
///
/// Used by attachment validation and by backend attachment resolution to
/// distinguish the imported swapchain backbuffer from graph-owned transients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WriteTargetRole {
    /// The imported swapchain image; contents exist outside the graph.
    ImportedBackbuffer,
    /// A graph-owned color-attachment transient.
    TransientColor,
    /// A graph-owned depth attachment transient (e.g. a shadow atlas).
    TransientDepth,
    /// Not an attachment target (sampled image, imported texture, unknown).
    Other,
}

/// Executable render graph.
///
/// Built once from a [`FrameGraphBuilder`], executed many times per frame.
/// Generic over the GPU backend (`VulkanRenderer` or `MetalRenderer`).
pub struct FrameGraph<B: RenderGraphBackend> {
    graph_identity: u64,
    pass_ids: Vec<PassId>,
    pass_positions: Vec<usize>,

    /// Pass descriptors in execution order.
    pub(super) passes: Vec<PassDesc>,

    /// Resource descriptors indexed by ResourceId.
    pub(super) resources: Vec<GraphResourceDesc>,

    /// Name -> ResourceId mapping for resource lookup.
    pub(super) resource_by_name: HashMap<String, ResourceId>,

    /// Pass name -> index mapping for execution context.
    pub(super) pass_names: HashMap<String, usize>,

    /// Resources whose final values are externally observable after execution.
    pub(crate) exported_resources: BTreeSet<ResourceId>,

    /// State contracts for imported images (including the backbuffer),
    /// validated at compile time and consumed by synchronization planning.
    pub(crate) imported_contracts: BTreeMap<ResourceId, ImportedImageContract>,

    /// External renderer-owned buffers imported by this graph.
    imported_buffers: HashMap<ResourceId, BufferHandle>,
    /// Native texture identities for imported image contracts.
    pub(crate) imported_images: HashMap<ResourceId, TextureHandle>,
    external_image_accesses: Vec<super::ImageAccess>,
    external_buffer_accesses: Vec<super::BufferAccess>,
    external_uploads_pending: bool,

    /// Descriptors for all graph-visible buffers, keyed by graph resource id.
    buffer_desc_by_id: HashMap<ResourceId, BufferDesc>,

    /// Whether pass liveness analysis is enabled for this graph.
    pub(crate) pass_culling_enabled: bool,

    /// Compiled execution plan (sorted live passes and dependency metadata).
    execution_plan: Option<ExecutionPlan>,

    /// Whether the graph has been compiled.
    compiled: bool,

    /// Transient resource descriptors (for lazy GPU resource creation).
    pub(super) transient_resources: Vec<GraphResourceDesc>,

    /// Graph-owned transient buffer descriptors.
    pub(super) transient_buffers: Vec<GraphBufferDesc>,

    /// Created transient textures (frame_idx -> ResourceId -> texture).
    /// Per-frame transient textures. One set per frame-in-flight to prevent
    /// race conditions where frame N+1 modifies layout tracking while frame N is still executing.
    pub(super) transient_textures: Vec<HashMap<ResourceId, B::TransientTexture>>,
    transient_allocation_contract: Option<Vec<TransientAllocationContract>>,
    transient_allocation_contract_validated: bool,

    /// Per-frame graph-owned buffer allocations.
    pub(super) transient_buffers_by_frame: Vec<HashMap<ResourceId, B::TransientBuffer>>,

    /// Whether compatible, non-overlapping transient textures share physical
    /// memory from the compiled allocation plan. Debugging switch.
    pub(super) transient_aliasing: bool,

    /// Whether each execution records the encoders it emitted. Off by default
    /// so the steady-state path pays nothing; see
    /// [`FrameGraph::set_execution_trace`].
    trace_enabled: bool,

    /// Encoders emitted by the most recent execution, when tracing is enabled.
    last_execution_trace: super::trace::ResourceExecutionTrace,
}

// --- Backend-agnostic methods ---
impl<B: RenderGraphBackend> FrameGraph<B> {
    /// Bind the acquired output image's final consumer before compiling.
    pub(crate) fn set_backbuffer_final_state(&mut self, final_state: ResourceState) {
        if let Some(id) = self.resource_id(BACKBUFFER_NAME)
            && let Some(contract) = self.imported_contracts.get_mut(&id)
            && contract.required_final != Some(final_state)
        {
            contract.required_final = Some(final_state);
            self.compiled = false;
        }
    }

    /// Create a new empty frame graph.
    pub fn new() -> Self {
        Self {
            graph_identity: super::handles::next_graph_identity(),
            pass_ids: Vec::new(),
            pass_positions: Vec::new(),
            passes: Vec::new(),
            resources: Vec::new(),
            resource_by_name: HashMap::new(),
            pass_names: HashMap::new(),
            exported_resources: BTreeSet::new(),
            imported_contracts: BTreeMap::new(),
            imported_buffers: HashMap::new(),
            imported_images: HashMap::new(),
            external_image_accesses: Vec::new(),
            external_buffer_accesses: Vec::new(),
            external_uploads_pending: false,
            buffer_desc_by_id: HashMap::new(),
            pass_culling_enabled: false,
            execution_plan: None,
            compiled: false,
            transient_resources: Vec::new(),
            transient_buffers: Vec::new(),
            transient_textures: Vec::new(),
            transient_allocation_contract: None,
            transient_allocation_contract_validated: false,
            transient_buffers_by_frame: Vec::new(),
            transient_aliasing: true,
            trace_enabled: false,
            last_execution_trace: super::trace::ResourceExecutionTrace::new(),
        }
    }

    /// Append a uniquely named pass and return its stable graph-owned handle.
    pub fn add_pass(&mut self, pass: PassDesc) -> Result<PassId, RenderGraphError> {
        self.insert_pass(self.passes.len(), pass)
    }

    /// Insert a uniquely named pass without invalidating existing pass handles.
    ///
    /// Invalid positions, empty names and duplicate names leave the graph intact.
    pub fn insert_pass(
        &mut self,
        index: usize,
        pass: PassDesc,
    ) -> Result<PassId, RenderGraphError> {
        if index > self.passes.len() {
            return Err(RenderGraphError::InvalidConfiguration(format!(
                "Pass insertion index {index} exceeds pass count {}",
                self.passes.len()
            )));
        }
        if pass.name.trim().is_empty() {
            return Err(GraphValidationError::EmptyPassName.into());
        }
        if self.pass_names.contains_key(&pass.name) {
            return Err(GraphValidationError::DuplicatePassName(pass.name).into());
        }
        let id = PassId {
            graph: self.graph_identity,
            slot: self.pass_positions.len(),
        };
        if index < self.passes.len() {
            for position in self
                .pass_positions
                .iter_mut()
                .chain(self.pass_names.values_mut())
            {
                if *position >= index {
                    *position += 1;
                }
            }
        }
        self.pass_positions.push(index);
        self.pass_ids.insert(index, id);
        self.pass_names.insert(pass.name.clone(), index);
        self.passes.insert(index, pass);
        self.compiled = false;
        self.execution_plan = None;
        Ok(id)
    }

    #[inline]
    pub(crate) fn pass_position(&self, id: PassId) -> Option<usize> {
        (id.graph == self.graph_identity)
            .then(|| self.pass_positions.get(id.slot).copied())
            .flatten()
    }

    /// Create or get a ResourceId for a named resource.
    pub(crate) fn create_resource_id(&mut self, name: impl Into<String>) -> ResourceId {
        let name = name.into();
        if let Some(&id) = self.resource_by_name.get(&name) {
            return id;
        }
        let id = ResourceId(self.resources.len() as u32);
        self.resources.push(GraphResourceDesc {
            name: name.clone(),
            resource_type: super::resource::GraphResourceType::SampledImage,
            format: crate::texture::ImageFormat::R8G8B8A8Unorm,
            width: 0,
            height: 0,
            tracks_swapchain_size: false,
        });
        self.resource_by_name.insert(name, id);
        self.compiled = false;
        self.execution_plan = None;
        id
    }

    /// Look up a ResourceId by name.
    pub fn resource_id(&self, name: &str) -> Option<ResourceId> {
        self.resource_by_name.get(name).copied()
    }

    /// Mark a resource's final value as externally observable.
    ///
    /// The first export enables pass culling. Passes that cannot contribute to
    /// an export or an explicit side effect are omitted from execution.
    pub fn export_resource(&mut self, name: &str) -> Result<ResourceId, RenderGraphError> {
        let id = self
            .resource_id(name)
            .ok_or_else(|| RenderGraphError::ResourceNotFound(name.to_string()))?;
        self.pass_culling_enabled = true;
        if self.exported_resources.insert(id) {
            self.compiled = false;
            self.execution_plan = None;
        }
        Ok(id)
    }

    /// Return the liveness of a pass in the current compiled plan.
    pub fn is_pass_live(&self, pass_id: PassId) -> Option<bool> {
        let index = self.pass_position(pass_id)?;
        self.execution_plan
            .as_ref()
            .and_then(|plan| plan.live_passes.get(index).copied())
    }

    pub(crate) fn is_pass_index_live(&self, pass_index: usize) -> bool {
        self.execution_plan
            .as_ref()
            .and_then(|plan| plan.live_passes.get(pass_index))
            .copied()
            .unwrap_or(true)
    }

    pub(crate) fn configure_pass_culling(
        &mut self,
        exported_resources: impl IntoIterator<Item = ResourceId>,
    ) {
        self.exported_resources = exported_resources.into_iter().collect();
        self.pass_culling_enabled = true;
        self.compiled = false;
        self.execution_plan = None;
    }

    /// Get the name of a resource by its ResourceId.
    pub fn resource_name(&self, id: ResourceId) -> Option<&str> {
        self.resources.get(id.0 as usize).map(|r| r.name.as_str())
    }

    /// Build the canonical execution plan without mutating frame state.
    pub(crate) fn build_execution_plan(&self) -> Result<ExecutionPlan, RenderGraphError> {
        let mut compiler = if self.pass_culling_enabled {
            GraphCompiler::from_pass_descs_with_exports(
                &self.passes,
                self.exported_resources.iter().copied(),
                self.imported_contracts.clone(),
            )
        } else {
            let mut compiler = GraphCompiler::from_pass_descs(&self.passes);
            compiler.imported_contracts = self.imported_contracts.clone();
            compiler
        };
        compiler.external_image_accesses = self.external_image_accesses.clone();
        compiler.external_uploads_pending = self.external_uploads_pending;
        compiler.external_buffer_accesses = self.external_buffer_accesses.clone();
        let mut plan = compiler.compile()?;
        if self.transient_aliasing {
            let allocation = TransientAllocationPlan::build(
                &self.resources,
                &self.transient_resources,
                &self.exported_resources,
                &plan.resource_lifetimes,
                &plan.live_image_accesses,
            );
            for slot in allocation
                .slots()
                .iter()
                .filter(|slot| slot.members.len() > 1)
            {
                for resource in &slot.members {
                    if let Some(lifetime) = plan.resource_lifetimes.get(resource) {
                        plan.sync.alias_handoffs[lifetime.first_pass].push(*resource);
                    }
                }
            }
        }
        Ok(plan)
    }

    fn prepare_external_image_producers(&mut self, renderer: &B) {
        let mut accesses = Vec::new();
        let uploads = renderer.graph_texture_upload_producers();
        let pending = !uploads.is_empty();
        for (handle, upload) in uploads {
            for (&resource, &imported) in &self.imported_images {
                if imported == handle {
                    accesses.push(super::ImageAccess::transfer_write(resource).with_range(
                        super::ImageSubresourceRange::new(
                            super::ImageAspects::COLOR,
                            upload.mip_level,
                            1,
                            upload.array_layer,
                            1,
                        ),
                    ));
                }
            }
        }
        if self.external_image_accesses != accesses || self.external_uploads_pending != pending {
            self.external_uploads_pending = pending;
            self.external_image_accesses = accesses;
            self.compiled = false;
        }
    }

    fn prepare_external_buffer_producers(&mut self, renderer: &B) {
        let mut accesses = Vec::new();
        for &resource in self.buffer_desc_by_id.keys() {
            if let Some(buffer) = self.buffer_by_id(renderer, resource, renderer.current_frame()) {
                for mut access in renderer.graph_buffer_previous_accesses(&buffer) {
                    access.resource = resource;
                    if !accesses.contains(&access) {
                        accesses.push(access);
                    }
                }
            }
        }
        accesses.sort_by_key(|access| (access.resource, access.range.offset, access.range.size));
        if self.external_buffer_accesses != accesses {
            self.external_buffer_accesses = accesses;
            self.compiled = false;
        }
    }

    /// Retain resolved canonical buffer scopes after successful native encoding.
    pub(crate) fn record_buffer_execution(&self, renderer: &B) {
        for pass in self.execution_order() {
            for access in &self.passes[pass].buffer_accesses {
                if let Some(buffer) =
                    self.buffer_by_id(renderer, access.resource, renderer.current_frame())
                {
                    let range = access.range.intersection(super::BufferByteRange::new(
                        0,
                        B::transient_buffer_size(&buffer),
                    ));
                    if let Some(range) = range {
                        renderer.record_graph_buffer_accesses(
                            &buffer,
                            &[super::BufferAccess { range, ..*access }],
                        );
                    }
                }
            }
        }
    }

    /// Compile the graph for execution.
    pub(crate) fn compile(&mut self) -> Result<(), RenderGraphError> {
        if self.compiled {
            return Ok(());
        }

        self.validate_declared_buffer_accesses()?;
        self.validate_pass_bindings()?;
        for pass in &self.passes {
            super::compute::validate_commands(pass).map_err(|reason| {
                RenderGraphError::Validation(
                    super::error::GraphValidationError::InvalidComputeCommand {
                        pass: pass.name.clone(),
                        reason,
                    },
                )
            })?;
        }
        let plan = self.build_execution_plan()?;
        self.validate_attachment_ops(&plan)?;
        self.transient_allocation_contract_validated = false;
        self.execution_plan = Some(plan);
        self.compiled = true;
        Ok(())
    }

    /// Classify what kind of attachment target a written resource is.
    fn write_target_role(&self, id: ResourceId) -> WriteTargetRole {
        use super::resource::GraphResourceType;

        let Some(desc) = self.resources.get(id.0 as usize) else {
            return WriteTargetRole::Other;
        };
        if desc.name == BACKBUFFER_NAME {
            return WriteTargetRole::ImportedBackbuffer;
        }
        match self
            .transient_resources
            .iter()
            .find(|d| d.name == desc.name)
            .map(|d| &d.resource_type)
        {
            Some(GraphResourceType::ColorAttachment { .. }) => WriteTargetRole::TransientColor,
            Some(GraphResourceType::DepthAttachment { .. }) => WriteTargetRole::TransientDepth,
            _ => WriteTargetRole::Other,
        }
    }

    /// Validate declared attachment operations against pass writes.
    ///
    /// Runs at compile time — after liveness culling, before any backend sees
    /// the graph — so invalid attachment contracts fail before command
    /// encoding. The declared operations are the only source of attachment
    /// behavior; this check makes missing or contradictory declarations loud.
    fn validate_attachment_ops(&self, plan: &ExecutionPlan) -> Result<(), RenderGraphError> {
        let mut produced: HashSet<ResourceId> = HashSet::new();

        for &pass_idx in &plan.sorted_passes {
            let pass = &self.passes[pass_idx];

            if pass.pass_type == PassType::Compute {
                if !pass.color_attachments.is_empty() || pass.depth_attachment.is_some() {
                    return Err(GraphValidationError::AttachmentOpsOnComputePass(
                        pass.name.clone(),
                    )
                    .into());
                }
                continue;
            }

            if !pass.uses_depth && pass.depth_attachment.is_some() {
                return Err(
                    GraphValidationError::DepthOpsWithoutDepthUse(pass.name.clone()).into(),
                );
            }

            if pass.uses_depth && pass.depth_target.is_none() {
                return Err(GraphValidationError::MissingDepthTarget {
                    pass: pass.name.clone(),
                }
                .into());
            }

            if let Some(target) = pass.depth_target {
                let resource = self.resource_name(target).unwrap_or("?").to_string();
                if self.write_target_role(target) != WriteTargetRole::TransientDepth
                    && !self.imported_images.contains_key(&target)
                {
                    return Err(GraphValidationError::InvalidDepthTarget {
                        pass: pass.name.clone(),
                        resource,
                    }
                    .into());
                }
                if pass.depth_attachment.is_none() {
                    return Err(GraphValidationError::MissingAttachmentOps {
                        pass: pass.name.clone(),
                        resource,
                    }
                    .into());
                }
            }

            // Declared color ops must target color attachments the pass writes.
            for (resource, ops) in &pass.color_attachments {
                let name = self.resource_name(*resource).unwrap_or("?").to_string();
                match self.write_target_role(*resource) {
                    WriteTargetRole::ImportedBackbuffer | WriteTargetRole::TransientColor => {}
                    _ => {
                        return Err(GraphValidationError::StrayAttachmentOps {
                            pass: pass.name.clone(),
                            resource: name,
                        }
                        .into());
                    }
                }
                if !pass.writes.contains(resource) {
                    return Err(GraphValidationError::StrayAttachmentOps {
                        pass: pass.name.clone(),
                        resource: name,
                    }
                    .into());
                }
                if ops.load == LoadOp::Clear && !matches!(ops.clear_value, ClearValue::Color(_)) {
                    return Err(GraphValidationError::AttachmentClearValueAspect {
                        pass: pass.name.clone(),
                        resource: name,
                        expected: "a color clear value",
                    }
                    .into());
                }
                if ops.load == LoadOp::Load
                    && !self.imported_contracts.contains_key(resource)
                    && !produced.contains(resource)
                {
                    return Err(GraphValidationError::LoadingUndefinedAttachment {
                        pass: pass.name.clone(),
                        resource: name,
                    }
                    .into());
                }
            }

            // Every written attachment target must have declared ops.
            for &write_id in &pass.writes {
                let name = self.resource_name(write_id).unwrap_or("?").to_string();
                let has_color_decl = pass.color_attachments.iter().any(|(id, _)| *id == write_id);
                match self.write_target_role(write_id) {
                    WriteTargetRole::ImportedBackbuffer | WriteTargetRole::TransientColor => {
                        if !has_color_decl {
                            return Err(GraphValidationError::MissingAttachmentOps {
                                pass: pass.name.clone(),
                                resource: name,
                            }
                            .into());
                        }
                    }
                    WriteTargetRole::TransientDepth => {
                        if pass.depth_attachment.is_none() {
                            return Err(GraphValidationError::MissingAttachmentOps {
                                pass: pass.name.clone(),
                                resource: name,
                            }
                            .into());
                        }
                    }
                    WriteTargetRole::Other => {}
                }
            }

            // Depth ops must be consistent and their clear values in range.
            if let Some(ops) = &pass.depth_attachment {
                for (aspect, aspect_ops) in [("depth", ops.depth), ("stencil", ops.stencil)] {
                    if aspect_ops.load == LoadOp::Clear
                        && !matches!(aspect_ops.clear_value, ClearValue::DepthStencil { .. })
                    {
                        return Err(GraphValidationError::AttachmentClearValueAspect {
                            pass: pass.name.clone(),
                            resource: aspect.to_string(),
                            expected: "a depth-stencil clear value",
                        }
                        .into());
                    }
                    if let ClearValue::DepthStencil { depth, .. } = aspect_ops.clear_value
                        && !(0.0..=1.0).contains(&depth)
                    {
                        return Err(GraphValidationError::InvalidDepthClearValue {
                            pass: pass.name.clone(),
                            depth,
                        }
                        .into());
                    }
                }
                // A load consumes this exact target, never another pass's depth image.
                let loads_depth =
                    ops.depth.load == LoadOp::Load || ops.stencil.load == LoadOp::Load;
                if loads_depth {
                    let depth_transient = pass.depth_target;
                    let has_producer = depth_transient.is_some_and(|id| {
                        produced.contains(&id)
                            || self.imported_contracts.get(&id).is_some_and(|contract| {
                                contract.initial != ResourceState::Undefined
                            })
                    });
                    if !has_producer {
                        let resource = depth_transient
                            .and_then(|id| self.resource_name(id))
                            .unwrap_or("scene depth")
                            .to_string();
                        return Err(GraphValidationError::LoadingUndefinedAttachment {
                            pass: pass.name.clone(),
                            resource,
                        }
                        .into());
                    }
                }
            }

            produced.extend(pass.writes.iter().copied());
        }

        Ok(())
    }

    /// Get a pass index by name.
    #[cfg(test)]
    pub(crate) fn pass_index(&self, name: &str) -> Option<usize> {
        self.pass_names.get(name).copied()
    }

    /// Get a pass handle by name.
    pub fn pass_id(&self, name: &str) -> Option<PassId> {
        self.pass_names.get(name).map(|&idx| self.pass_ids[idx])
    }

    /// Cleanup and destroy all transient textures.
    ///
    /// The caller must complete all GPU work using these allocations first.
    pub fn cleanup(&mut self) {
        log::info!(
            "Cleaning up frame graph transient textures ({} frames)",
            self.transient_textures.len()
        );
        let total_textures: usize = self.transient_textures.iter().map(|m| m.len()).sum();
        log::info!("  Total textures to clean up: {}", total_textures);
        self.transient_textures.clear();
        self.transient_allocation_contract = None;
        self.transient_allocation_contract_validated = false;
        self.transient_buffers_by_frame.clear();
        log::info!("Frame graph cleanup complete");
    }

    /// Get the number of passes in the graph.
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    /// Get a pass by index.
    pub(crate) fn pass(&self, index: usize) -> Option<&PassDesc> {
        self.passes.get(index)
    }

    /// Image format declared for a transient graph resource.
    ///
    /// `None` for imported resources (the backbuffer and external textures),
    /// whose formats are backend-owned.
    pub(crate) fn resource_format_for_target(
        &self,
        id: ResourceId,
    ) -> Option<crate::texture::ImageFormat> {
        let name = self.resource_name(id)?;
        self.transient_resources
            .iter()
            .find(|desc| desc.name == name)
            .map(|desc| desc.format)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn resource_format(&self, id: ResourceId) -> Option<crate::texture::ImageFormat> {
        self.resource_format_for_target(id)
    }

    /// Get the execution order for passes.
    pub(crate) fn execution_order(&self) -> Vec<usize> {
        self.execution_plan
            .as_ref()
            .map(|plan| plan.sorted_passes.clone())
            .unwrap_or_else(|| (0..self.passes.len()).collect())
    }

    /// Compiled synchronization operations preceding one pass.
    ///
    /// Empty when the plan is not compiled or the pass is culled.
    pub(crate) fn image_sync_ops(&self, pass_index: usize) -> &[super::sync_plan::ImageSyncOp] {
        self.execution_plan
            .as_ref()
            .and_then(|plan| plan.sync.pass_ops.get(pass_index))
            .map(|ops| ops.as_slice())
            .unwrap_or(&[])
    }

    /// Compiled buffer operations to execute before a pass.
    ///
    /// Byte-range dependencies, so a backend realizes each as a memory barrier
    /// over `range` rather than a layout transition. Public so a backend (and
    /// the diagnostics view) consumes the same plan the compiler produced.
    pub fn buffer_sync_ops(&self, pass_index: usize) -> &[super::sync_plan::BufferSyncOp] {
        self.execution_plan
            .as_ref()
            .and_then(|plan| plan.sync.pass_buffer_ops.get(pass_index))
            .map(|ops| ops.as_slice())
            .unwrap_or(&[])
    }

    /// Every compiled buffer synchronization operation, in pass order.
    pub fn all_buffer_sync_ops(&self) -> Vec<&super::sync_plan::BufferSyncOp> {
        self.execution_plan
            .as_ref()
            .map(|plan| {
                plan.sync
                    .pass_buffer_ops
                    .iter()
                    .flat_map(|ops| ops.iter())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Compiled frame-end operations satisfying imported final-state contracts.
    pub(crate) fn final_image_sync_ops(&self) -> &[super::sync_plan::ImageSyncOp] {
        self.execution_plan
            .as_ref()
            .map(|plan| plan.sync.final_ops.as_slice())
            .unwrap_or(&[])
    }

    /// Get a transient texture by ResourceId for a specific frame.
    pub fn transient_texture_by_id(
        &self,
        id: ResourceId,
        frame_idx: usize,
    ) -> Option<&B::TransientTexture> {
        self.transient_textures.get(frame_idx)?.get(&id)
    }

    /// Get a mutable transient texture by ResourceId for a specific frame.
    pub fn transient_texture_by_id_mut(
        &mut self,
        id: ResourceId,
        frame_idx: usize,
    ) -> Option<&mut B::TransientTexture> {
        self.transient_textures.get_mut(frame_idx)?.get_mut(&id)
    }

    /// Warm every live compute pipeline before acquiring or encoding a frame.
    pub fn initialize_compute_pipelines(
        &mut self,
        backend: &mut B,
    ) -> Result<(), RenderGraphError> {
        self.compile()?;
        for index in self.execution_order() {
            let pass = &self.passes[index];
            for command in &pass.commands {
                if let super::compute::ComputeCommand::Dispatch(dispatch) = command {
                    backend.prepare_compute_pipeline(&dispatch.pipeline)?;
                }
            }
        }
        Ok(())
    }

    /// Get a transient texture by name for a specific frame.
    pub fn transient_texture(&self, name: &str, frame_idx: usize) -> Option<&B::TransientTexture> {
        let id = self.resource_by_name.get(name)?;
        self.transient_texture_by_id(*id, frame_idx)
    }

    /// Get the image view for a transient texture by name and frame index.
    pub fn transient_image_view(&self, name: &str, frame_idx: usize) -> Option<B::ImageView> {
        self.transient_texture(name, frame_idx)
            .map(B::transient_texture_view)
    }

    /// Disable transient memory aliasing for debugging.
    ///
    /// Must be called before [`Self::initialize_transient_textures`]; every
    /// transient texture then gets a standalone allocation, and memoryless selection
    /// is disabled, without changing
    /// graph semantics.
    pub fn set_transient_aliasing(&mut self, enabled: bool) -> Result<(), RenderGraphError> {
        if self.transient_aliasing != enabled && !self.transient_textures.is_empty() {
            return Err(RenderGraphError::BackendError("Transient storage policy cannot change while native allocations exist; wait for GPU completion and clean up the graph first".into()));
        }
        if self.transient_aliasing != enabled {
            self.transient_aliasing = enabled;
            self.compiled = false;
            self.execution_plan = None;
        }
        Ok(())
    }

    /// Enable or disable recording of the emitted encoder trace.
    ///
    /// Validation mode for render graph execution: enabled turns on one trace
    /// entry per pass the backend dispatches, which
    /// [`Self::compare_execution_trace`] then checks against the compiled plan.
    pub fn set_execution_trace(&mut self, enabled: bool) {
        self.trace_enabled = enabled;
    }

    /// Whether execution currently records an emitted encoder trace.
    pub fn execution_trace_enabled(&self) -> bool {
        self.trace_enabled
    }

    /// Encoders emitted by the most recent execution.
    pub fn last_execution_trace(&self) -> &super::trace::ResourceExecutionTrace {
        &self.last_execution_trace
    }

    /// Store an emitted encoder trace produced by a backend that drives
    /// execution itself (Metal encodes from its own compiled pass records).
    #[cfg(target_os = "macos")]
    pub(crate) fn store_last_execution_trace(
        &mut self,
        trace: super::trace::ResourceExecutionTrace,
    ) {
        if self.trace_enabled {
            self.last_execution_trace = trace;
        }
    }

    /// Compare the most recent execution's emitted trace against the compiled
    /// plan, returning every divergence found.
    ///
    /// An empty result means the backend emitted exactly the compiled passes, in
    /// order, against the declared attachment contract. Requires tracing to be
    /// enabled; otherwise the trace is empty and every live pass reports as
    /// missing.
    pub fn compare_execution_trace(&self) -> Vec<super::trace::TraceDivergence> {
        super::trace::compare_with_compiled(
            &self.resources,
            &self.passes,
            &self.execution_order(),
            &self.last_execution_trace,
        )
    }

    /// Whether this pass begins the lifetime of a texture in an aliased range.
    pub(crate) fn texture_alias_handoff_before(&self, pass_index: usize) -> bool {
        self.execution_plan
            .as_ref()
            .and_then(|plan| plan.sync.alias_handoffs.get(pass_index))
            .is_some_and(|handoffs| !handoffs.is_empty())
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn pass_boundary(&self, index: usize) -> Option<&super::PassBoundary> {
        self.execution_plan
            .as_ref()?
            .sync
            .pass_boundaries
            .get(index)?
            .as_ref()
    }
}

impl<B: RenderGraphBackend> Default for FrameGraph<B> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
