use std::cell::RefCell;
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

#[derive(Debug, PartialEq)]
struct TransientAllocationMember {
    resource: ResourceId,
    format: crate::texture::ImageFormat,
    width: u32,
    height: u32,
    resource_class: u8,
}

#[derive(Debug, PartialEq)]
struct TransientAllocationContract {
    members: Vec<TransientAllocationMember>,
    policy: super::backend::TransientSlotPolicy,
}

/// Default state contract for the built-in backbuffer: contents from before
/// the graph (the previously presented frame) are observable, so a pass may
/// load them without an in-graph producer. Applications that present the
/// backbuffer override this with `FrameGraphBuilder::backbuffer_contract` to
/// also require the final `PresentSrc` state.
const DEFAULT_BACKBUFFER_CONTRACT: ImportedImageContract =
    ImportedImageContract::arrives_in(ResourceState::ColorAttachment);

fn validate_buffer_access_descriptor(
    pass: &str,
    resource: &str,
    mode: ResourceAccessMode,
    usage: BufferUsage,
    stage: ResourceAccessStage,
    range: BufferByteRange,
    desc: BufferDesc,
) -> Result<(), RenderGraphError> {
    if range.is_empty()
        || range.offset >= desc.size
        || (range.size != u64::MAX && range.end() > desc.size)
    {
        return Err(GraphValidationError::InvalidBufferAccessRange {
            pass: pass.to_string(),
            resource: resource.to_string(),
            offset: range.offset,
            size: range.size,
            capacity: desc.size,
        }
        .into());
    }

    let (required, usage_name) = match usage {
        BufferUsage::Uniform => (BufferUsages::UNIFORM, "uniform"),
        BufferUsage::Storage => (BufferUsages::STORAGE, "storage"),
        BufferUsage::Vertex => (BufferUsages::VERTEX, "vertex"),
        BufferUsage::Index => (BufferUsages::INDEX, "index"),
        BufferUsage::Indirect => (BufferUsages::INDIRECT, "indirect"),
        BufferUsage::TransferSource => (BufferUsages::TRANSFER_SOURCE, "transfer-source"),
        BufferUsage::TransferDestination => {
            (BufferUsages::TRANSFER_DESTINATION, "transfer-destination")
        }
        BufferUsage::Readback => (BufferUsages::READBACK, "readback"),
    };
    let usage_declared = desc.usages.contains(required)
        && (usage != BufferUsage::Readback || desc.memory == BufferMemoryPolicy::Readback);
    if !usage_declared {
        return Err(GraphValidationError::BufferUsageNotDeclared {
            pass: pass.to_string(),
            resource: resource.to_string(),
            usage: usage_name.to_string(),
        }
        .into());
    }

    let mode_valid = match usage {
        BufferUsage::Uniform
        | BufferUsage::Vertex
        | BufferUsage::Index
        | BufferUsage::Indirect
        | BufferUsage::TransferSource
        | BufferUsage::Readback => mode == ResourceAccessMode::Read,
        BufferUsage::TransferDestination => mode == ResourceAccessMode::Write,
        BufferUsage::Storage => true,
    };
    if !mode_valid {
        return Err(GraphValidationError::InvalidBufferAccessMode {
            pass: pass.to_string(),
            resource: resource.to_string(),
            usage: usage_name.to_string(),
        }
        .into());
    }

    let stage_valid = match usage {
        BufferUsage::Uniform | BufferUsage::Storage => matches!(
            stage,
            ResourceAccessStage::VertexShader
                | ResourceAccessStage::FragmentShader
                | ResourceAccessStage::ComputeShader
                | ResourceAccessStage::AllGraphics
        ),
        BufferUsage::Vertex | BufferUsage::Index => stage == ResourceAccessStage::VertexInput,
        BufferUsage::Indirect => stage == ResourceAccessStage::DrawIndirect,
        BufferUsage::TransferSource | BufferUsage::TransferDestination => {
            stage == ResourceAccessStage::Transfer
        }
        BufferUsage::Readback => stage == ResourceAccessStage::Host,
    };
    if !stage_valid {
        return Err(GraphValidationError::InvalidBufferAccessStage {
            pass: pass.to_string(),
            resource: resource.to_string(),
            usage: usage_name.to_string(),
            stage,
        }
        .into());
    }

    Ok(())
}

/// Per-frame parameters for render graph execution.
///
/// These values change every frame and are set before calling `execute()`.
/// Logically separate from the graph structure which is "built once, executed many times."
pub(crate) struct FrameParams {
    pub delta_time: f32,
    pub frame_count: usize,
    pub particle_emit_workgroup_count: u32,
    pub particle_simulate_workgroup_count: u32,
    pub animation_skeleton_count: u32,
    pub skeleton_copy_commands: Vec<(crate::handle::SkeletonHandle, u32, u32)>,
}

impl Default for FrameParams {
    fn default() -> Self {
        Self {
            delta_time: 0.0,
            frame_count: 0,
            particle_emit_workgroup_count: 1,
            particle_simulate_workgroup_count: 1,
            animation_skeleton_count: 0,
            skeleton_copy_commands: Vec::new(),
        }
    }
}

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
    builtin_buffers: HashMap<ResourceId, super::compute::BuiltinBuffer>,

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

    /// Base bindless index for LDR texture (actual index = base + frame_idx).
    ldr_texture_base_index: Option<u32>,

    /// Per-frame parameters set before each `execute()` call.
    /// These are logically separate from the graph structure itself,
    /// which is "built once, executed many times."
    pub(super) params: FrameParams,

    /// Per-frame compositing descriptor sets (one per frame in flight).
    /// Pre-allocated and reused each frame via update_textures().
    pub(super) compositing_descriptor_sets:
        RefCell<[Option<crate::vulkan::compositing::CompositingDescriptorSet>; 2]>,
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
            builtin_buffers: HashMap::new(),
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
            ldr_texture_base_index: None,
            params: FrameParams::default(),
            compositing_descriptor_sets: RefCell::new([None, None]),
        }
    }

    /// Add a pass to the graph.
    pub fn add_pass(&mut self, pass: PassDesc) -> PassId {
        let index = self.passes.len();
        self.pass_names.insert(pass.name.clone(), index);
        self.passes.push(pass);
        self.compiled = false;
        self.execution_plan = None;
        PassId(index as u32)
    }

    /// Insert a pass at a specific index, reindexing all subsequent passes.
    pub fn insert_pass(&mut self, index: usize, pass: PassDesc) {
        self.passes.insert(index, pass);
        self.pass_names.clear();
        for (i, p) in self.passes.iter().enumerate() {
            self.pass_names.insert(p.name.clone(), i);
        }
        self.compiled = false;
        self.execution_plan = None;
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
        self.execution_plan
            .as_ref()
            .and_then(|plan| plan.live_passes.get(pass_id.0 as usize).copied())
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

    #[cfg(target_os = "macos")]
    pub(crate) fn frame_parameters(&self) -> &FrameParams {
        &self.params
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
        let mut frame_depth_written = false;

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
                // Loading depth requires a producer: an earlier pass that
                // wrote the frame depth, or the pass's own depth transient.
                let loads_depth =
                    ops.depth.load == LoadOp::Load || ops.stencil.load == LoadOp::Load;
                if loads_depth {
                    let depth_transient = pass.depth_target.or_else(|| {
                        pass.writes.iter().copied().find(|&id| {
                            self.write_target_role(id) == WriteTargetRole::TransientDepth
                        })
                    });
                    let has_producer = match depth_transient {
                        Some(id) => {
                            produced.contains(&id)
                                || self.imported_contracts.get(&id).is_some_and(|contract| {
                                    contract.initial != ResourceState::Undefined
                                })
                        }
                        None => frame_depth_written,
                    };
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
            if pass.uses_depth {
                frame_depth_written = true;
            }
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
        self.pass_names.get(name).map(|&idx| PassId(idx as u32))
    }

    /// Get the base bindless index for the LDR (tonemapped) texture.
    pub fn get_ldr_texture_base_index(&self) -> Option<u32> {
        self.ldr_texture_base_index
    }

    /// Set the base bindless index for the LDR texture.
    pub fn set_ldr_texture_base_index(&mut self, index: u32) {
        self.ldr_texture_base_index = Some(index);
    }

    /// Set the delta time for this frame.
    pub fn set_delta_time(&mut self, delta_time: f32) {
        self.params.delta_time = delta_time;
    }

    /// Set the global frame counter for this frame.
    pub fn set_frame_count(&mut self, frame_count: usize) {
        self.params.frame_count = frame_count;
    }

    /// Set the particle emit workgroup count for this frame.
    pub fn set_particle_emit_workgroup_count(&mut self, count: u32) {
        self.params.particle_emit_workgroup_count = count;
    }

    /// Set the particle simulate workgroup count for this frame.
    pub fn set_particle_simulate_workgroup_count(&mut self, count: u32) {
        self.params.particle_simulate_workgroup_count = count;
    }

    /// Set the animation skeleton count for this frame.
    pub fn set_animation_skeleton_count(&mut self, count: u32) {
        self.params.animation_skeleton_count = count;
    }

    /// Set skeleton copy commands for this frame.
    pub fn set_skeleton_copy_commands(
        &mut self,
        commands: Vec<(crate::handle::SkeletonHandle, u32, u32)>,
    ) {
        use super::access::{
            BufferAccess, BufferByteRange, BufferUsage, ResourceAccessMode, ResourceAccessStage,
        };
        if self.params.skeleton_copy_commands == commands {
            return;
        }
        self.params.skeleton_copy_commands = commands.clone();
        let Some(output) = self.resource_id("scene_AnimationOutput") else {
            return;
        };
        let skeleton_ids = self
            .builtin_buffers
            .iter()
            .filter_map(|(id, role)| {
                matches!(role, super::compute::BuiltinBuffer::Skeleton(_)).then_some(*id)
            })
            .collect::<HashSet<_>>();
        for pass in &mut self.passes {
            pass.buffer_accesses
                .retain(|access| !skeleton_ids.contains(&access.resource));
        }
        let mut copy_commands = Vec::new();
        let mut copy_accesses = Vec::new();
        let mut vertex_accesses = Vec::new();
        for (handle, offset, count) in commands {
            let bytes = u64::from(count) * 64;
            if bytes == 0 {
                continue;
            }
            let target = self.import_builtin_buffer(
                format!("scene_skeleton_{}_{}", handle.index(), handle.generation()),
                super::compute::BuiltinBuffer::Skeleton(handle),
                BufferDesc::new(
                    bytes,
                    super::resource::BufferUsages::STORAGE
                        | super::resource::BufferUsages::TRANSFER_DESTINATION,
                    super::resource::BufferMemoryPolicy::DeviceLocal,
                ),
            );
            copy_commands.push(super::compute::ComputeCommand::CopyBuffer {
                source: output,
                destination: target,
                source_offset: u64::from(offset) * 64,
                destination_offset: 0,
                size: bytes,
            });
            copy_accesses.push(BufferAccess::new(
                output,
                ResourceAccessMode::Read,
                BufferUsage::TransferSource,
                ResourceAccessStage::Transfer,
                BufferByteRange::new(u64::from(offset) * 64, bytes),
            ));
            copy_accesses.push(BufferAccess::new(
                target,
                ResourceAccessMode::Write,
                BufferUsage::TransferDestination,
                ResourceAccessStage::Transfer,
                BufferByteRange::new(0, bytes),
            ));
            vertex_accesses.push(BufferAccess::new(
                target,
                ResourceAccessMode::Read,
                BufferUsage::Storage,
                ResourceAccessStage::VertexShader,
                BufferByteRange::new(0, bytes),
            ));
        }
        for pass in &mut self.passes {
            if pass.name == "animation_skeleton_copy" {
                pass.commands = copy_commands.clone();
                pass.set_buffer_accesses(copy_accesses.clone());
            } else if pass.pass_type == PassType::Graphics
                && matches!(
                    pass.kind,
                    Some(
                        super::pass::PassKind::DepthPrepass
                            | super::pass::PassKind::Shadow
                            | super::pass::PassKind::Geometry
                            | super::pass::PassKind::ObjectId
                            | super::pass::PassKind::Outline
                    )
                )
            {
                let mut accesses = pass.buffer_accesses.clone();
                accesses.extend(vertex_accesses.iter().copied());
                pass.set_buffer_accesses(accesses);
            }
        }
        self.compiled = false;
        self.execution_plan = None;
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
        self.compositing_descriptor_sets
            .borrow_mut()
            .iter_mut()
            .for_each(|slot| *slot = None);
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

    /// Get a graph-owned buffer by resource id for one frame slot.
    pub fn transient_buffer_by_id(
        &self,
        id: ResourceId,
        frame_idx: usize,
    ) -> Option<&B::TransientBuffer> {
        self.transient_buffers_by_frame.get(frame_idx)?.get(&id)
    }

    /// Get a graph-owned buffer by name for one frame slot.
    pub fn transient_buffer(&self, name: &str, frame_idx: usize) -> Option<&B::TransientBuffer> {
        let id = self.resource_by_name.get(name)?;
        self.transient_buffer_by_id(*id, frame_idx)
    }

    /// Resolve either a transient allocation or an imported buffer handle.
    pub fn buffer_by_id<'a>(
        &'a self,
        backend: &'a B,
        id: ResourceId,
        frame_idx: usize,
    ) -> Option<super::backend::ResolvedGraphBuffer<'a, B>> {
        if let Some(role) = self.builtin_buffers.get(&id) {
            return backend
                .builtin_buffer(*role)
                .map(super::backend::ResolvedGraphBuffer::Imported);
        }
        self.transient_buffer_by_id(id, frame_idx)
            .or_else(|| {
                self.imported_buffers
                    .get(&id)
                    .and_then(|handle| B::buffer_by_handle(backend, *handle))
            })
            .map(super::backend::ResolvedGraphBuffer::Borrowed)
    }

    /// Import a renderer subsystem allocation, resolved independently for each frame slot.
    pub fn import_builtin_buffer(
        &mut self,
        name: impl Into<String>,
        role: super::compute::BuiltinBuffer,
        desc: BufferDesc,
    ) -> ResourceId {
        let id = self.create_resource_id(name);
        self.builtin_buffers.insert(id, role);
        self.compiled = false;
        self.buffer_desc_by_id.insert(id, desc);
        self.execution_plan = None;
        id
    }

    /// Whether a buffer is owned by an optional built-in subsystem.
    pub(crate) fn is_builtin_buffer(&self, resource: ResourceId) -> bool {
        self.builtin_buffers.contains_key(&resource)
    }

    /// Add buffer consumers to an existing pass while preserving its image contract.
    pub fn extend_pass_buffer_accesses(
        &mut self,
        name: &str,
        accesses: impl IntoIterator<Item = super::access::BufferAccess>,
    ) -> Result<(), RenderGraphError> {
        let pass = self
            .passes
            .iter_mut()
            .find(|pass| pass.name == name)
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(format!("Missing pass '{name}'"))
            })?;
        let mut combined = pass.buffer_accesses.clone();
        combined.extend(accesses);
        pass.set_buffer_accesses(combined);
        self.execution_plan = None;
        self.compiled = false;
        Ok(())
    }

    /// Buffer descriptor declared for a named graph resource.
    pub fn buffer_desc(&self, name: &str) -> Option<BufferDesc> {
        let id = self.resource_by_name.get(name)?;
        self.buffer_desc_by_id.get(id).copied()
    }

    fn validate_declared_buffer_accesses(&self) -> Result<(), RenderGraphError> {
        for pass in &self.passes {
            let declared_buffers = pass
                .buffer_accesses
                .iter()
                .map(|access| access.resource)
                .collect::<BTreeSet<_>>();

            for &resource in pass.reads.iter().chain(&pass.writes) {
                if self.buffer_desc_by_id.contains_key(&resource)
                    && !declared_buffers.contains(&resource)
                {
                    return Err(GraphValidationError::MissingTypedBufferAccess {
                        pass: pass.name.clone(),
                        resource: self.resource_name(resource).unwrap_or("?").to_string(),
                    }
                    .into());
                }
            }

            for access in &pass.image_accesses {
                if self.buffer_desc_by_id.contains_key(&access.resource) {
                    return Err(GraphValidationError::ImageAccessOnNonImage {
                        pass: pass.name.clone(),
                        resource: self
                            .resource_name(access.resource)
                            .unwrap_or("?")
                            .to_string(),
                    }
                    .into());
                }
            }

            for access in &pass.buffer_accesses {
                let resource_name = self.resource_name(access.resource).unwrap_or("?");
                let Some(desc) = self.buffer_desc_by_id.get(&access.resource).copied() else {
                    return Err(GraphValidationError::BufferAccessOnNonBuffer {
                        pass: pass.name.clone(),
                        resource: resource_name.to_string(),
                    }
                    .into());
                };
                validate_buffer_access_descriptor(
                    &pass.name,
                    resource_name,
                    access.mode,
                    access.usage,
                    access.stage,
                    access.range,
                    desc,
                )?;
            }
        }
        Ok(())
    }

    /// Warm every live compute pipeline before acquiring or encoding a frame.
    pub fn initialize_compute_pipelines(
        &mut self,
        backend: &mut B,
    ) -> Result<(), RenderGraphError> {
        self.compile()?;
        for pass in &self.passes {
            for command in &pass.commands {
                if let super::compute::ComputeCommand::Dispatch(dispatch) = command {
                    backend.prepare_compute_pipeline(&dispatch.kernel.descriptor())?;
                }
            }
        }
        Ok(())
    }

    /// Allocate the graph's declared transient buffers for every frame slot.
    pub fn initialize_transient_buffers(&mut self, backend: &B) -> Result<(), RenderGraphError> {
        if !self.transient_buffers_by_frame.is_empty() {
            return Ok(());
        }

        for (&resource, &handle) in &self.imported_buffers {
            let Some(buffer) = B::buffer_by_handle(backend, handle) else {
                return Err(RenderGraphError::InvalidConfiguration(format!(
                    "Imported buffer '{}' has a stale handle",
                    self.resource_name(resource).unwrap_or("?")
                )));
            };
            let expected = self.buffer_desc_by_id.get(&resource).copied();
            if expected != Some(B::buffer_desc(buffer)) {
                return Err(RenderGraphError::InvalidConfiguration(format!(
                    "Imported buffer '{}' descriptor does not match its handle",
                    self.resource_name(resource).unwrap_or("?")
                )));
            }
        }

        let mut frame_slots = Vec::with_capacity(B::transient_texture_frames());
        for _ in 0..B::transient_texture_frames() {
            let mut frame_buffers = HashMap::with_capacity(self.transient_buffers.len());
            for desc in &self.transient_buffers {
                let id = self
                    .resource_by_name
                    .get(&desc.name)
                    .copied()
                    .ok_or_else(|| {
                        RenderGraphError::Validation(
                            GraphValidationError::MissingResourceNamespaceEntry(desc.name.clone()),
                        )
                    })?;
                let buffer = B::create_transient_buffer(backend, desc.buffer)?;
                frame_buffers.insert(id, buffer);
            }
            frame_slots.push(frame_buffers);
        }
        self.transient_buffers_by_frame = frame_slots;

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

    /// Group transient resources into physical allocation slots.
    ///
    /// Driven by the compiled allocation plan: compatible resources whose
    /// live intervals do not overlap share a slot. Resources without a
    /// compiled lifetime (culled or unused) receive no allocation. A disabled
    /// optimization policy gives each live texture an independent group.
    fn transient_allocation_groups(&self) -> Result<Vec<Vec<GraphResourceDesc>>, RenderGraphError> {
        let plan = self.build_execution_plan()?;
        let allocation = TransientAllocationPlan::build(
            &self.resources,
            &self.transient_resources,
            &self.exported_resources,
            &plan.resource_lifetimes,
            &plan.live_image_accesses,
        );

        let mut standalone = Vec::new();
        let mut by_slot = BTreeMap::<u32, Vec<GraphResourceDesc>>::new();
        for desc in &self.transient_resources {
            let resource_id = self
                .resource_by_name
                .get(&desc.name)
                .copied()
                .ok_or_else(|| {
                    RenderGraphError::Validation(
                        GraphValidationError::MissingResourceNamespaceEntry(desc.name.clone()),
                    )
                })?;
            match allocation.physical_allocation_id(resource_id) {
                Some(_) if !self.transient_aliasing => standalone.push(vec![desc.clone()]),
                Some(slot) => by_slot.entry(slot).or_default().push(desc.clone()),
                None => {}
            }
        }

        Ok(standalone
            .into_iter()
            .chain(by_slot.into_values())
            .collect())
    }

    fn build_transient_allocation_contract(
        &self,
        groups: &[Vec<GraphResourceDesc>],
    ) -> Result<Vec<TransientAllocationContract>, RenderGraphError> {
        let plan = self.build_execution_plan()?;
        let mut allocation = TransientAllocationPlan::build(
            &self.resources,
            &self.transient_resources,
            &self.exported_resources,
            &plan.resource_lifetimes,
            &plan.live_image_accesses,
        );
        allocation.apply_attachment_storage(&self.passes, &plan.resource_lifetimes);
        groups
            .iter()
            .enumerate()
            .map(|(group_index, group)| {
                let members = group
                    .iter()
                    .map(|desc| {
                        let resource = self.resource_id(&desc.name).ok_or_else(|| {
                            RenderGraphError::Validation(
                                GraphValidationError::MissingResourceNamespaceEntry(
                                    desc.name.clone(),
                                ),
                            )
                        })?;
                        let resource_class = match desc.resource_type {
                            super::resource::GraphResourceType::ColorAttachment { .. } => 0,
                            super::resource::GraphResourceType::DepthAttachment {
                                sampled: false,
                                ..
                            } => 1,
                            super::resource::GraphResourceType::DepthAttachment {
                                sampled: true,
                                ..
                            } => 2,
                            super::resource::GraphResourceType::SampledImage => 3,
                        };
                        Ok(TransientAllocationMember {
                            resource,
                            format: desc.format,
                            width: desc.width,
                            height: desc.height,
                            resource_class,
                        })
                    })
                    .collect::<Result<Vec<_>, RenderGraphError>>()?;
                let uses = |usage| {
                    members.iter().any(|member| {
                        plan.live_image_accesses.iter().any(|access| {
                            access.resource == member.resource && access.usage == usage
                        })
                    })
                };
                let policy = super::backend::TransientSlotPolicy {
                    frame_slot: 0,
                    allocation_slot: group_index as u32,
                    optimize: self.transient_aliasing,
                    memoryless: self.transient_aliasing
                        && members.iter().all(|member| {
                            allocation
                                .persistence(member.resource)
                                .is_some_and(|persistence| persistence.tile_memory.is_eligible())
                        }),
                    storage: uses(super::access::ResourceAccessUsage::Storage),
                    transfer_destination: uses(
                        super::access::ResourceAccessUsage::TransferDestination,
                    ),
                };
                Ok(TransientAllocationContract { members, policy })
            })
            .collect()
    }

    /// Initialize transient textures using the backend.
    ///
    /// Creates per-frame sets of textures — one per frame-in-flight —
    /// grouped into physical allocation slots by the compiled plan.
    pub fn initialize_transient_textures(&mut self, backend: &B) -> Result<(), RenderGraphError> {
        if self.compiled && self.transient_allocation_contract_validated {
            return Ok(());
        }
        let groups = self.transient_allocation_groups()?;
        let contract = self.build_transient_allocation_contract(&groups)?;
        if let Some(existing) = &self.transient_allocation_contract {
            // Fresh groups prove every old shared range still has disjoint live intervals.
            if *existing != contract {
                return Err(RenderGraphError::AllocationContractChanged);
            }
            self.transient_allocation_contract_validated = true;
            return Ok(());
        }

        let frames = B::transient_texture_frames();

        log::info!(
            "Initializing {} transient textures in {} allocation groups ({} frames in flight, aliasing {})",
            self.transient_resources.len(),
            groups.len(),
            frames,
            if self.transient_aliasing { "on" } else { "off" },
        );

        let mut frame_slots = Vec::with_capacity(frames);
        for frame_idx in 0..frames {
            let mut frame_textures = HashMap::new();
            for (group, compiled) in groups.iter().zip(&contract) {
                let policy = super::backend::TransientSlotPolicy {
                    frame_slot: frame_idx,
                    ..compiled.policy
                };
                let textures = B::create_transient_slot(backend, group, policy)?;
                for (desc, texture) in group.iter().zip(textures) {
                    let resource_id =
                        self.resource_by_name
                            .get(&desc.name)
                            .copied()
                            .ok_or_else(|| {
                                RenderGraphError::Validation(
                                    GraphValidationError::MissingResourceNamespaceEntry(
                                        desc.name.clone(),
                                    ),
                                )
                            })?;
                    frame_textures.insert(resource_id, texture);
                }
            }

            frame_slots.push(frame_textures);
        }
        self.transient_textures = frame_slots;
        self.transient_allocation_contract = Some(contract);
        self.transient_allocation_contract_validated = true;

        Ok(())
    }

    /// Register a transient texture with the bindless texture system.
    ///
    /// Registers ALL per-frame instances of the texture.
    /// Returns the base slot index; frame N's texture is at `base_slot + N`.
    pub fn register_transient_texture_bindless(
        &mut self,
        backend: &mut B,
        name: &str,
    ) -> Result<u32, RenderGraphError> {
        let num_frames = self.transient_textures.len();
        if num_frames == 0 {
            return Err(RenderGraphError::InvalidConfiguration(
                "Transient textures not initialized".to_string(),
            ));
        }

        log::info!(
            "Registering transient texture '{}' ({} frames) with bindless system",
            name,
            num_frames
        );

        let resource_id = self
            .resource_by_name
            .get(name)
            .copied()
            .ok_or_else(|| RenderGraphError::ResourceNotFound(name.to_string()))?;

        for frame_idx in 0..num_frames {
            if let Some(frame_textures) = self.transient_textures.get_mut(frame_idx)
                && let Some(texture) = frame_textures.get_mut(&resource_id)
            {
                let slot = backend.register_bindless_texture(texture)?;
                B::set_transient_texture_bindless_slot(texture, slot);
                log::trace!("  Frame {}: slot {}", frame_idx, slot);
            }
        }

        let base_slot = self
            .transient_textures
            .first()
            .and_then(|textures| textures.get(&resource_id))
            .and_then(B::transient_texture_bindless_slot)
            .ok_or_else(|| RenderGraphError::ResourceNotFound(name.to_string()))?;

        if name == "ldr_color" {
            self.ldr_texture_base_index = Some(base_slot);
        }

        Ok(base_slot)
    }

    /// Recreate transient textures with new dimensions.
    ///
    /// Old textures are destroyed and new ones are created with the updated dimensions.
    /// Returns (texture_name, bindless_slot) tuples for all recreated textures.
    pub fn recreate_transient_textures(
        &mut self,
        backend: &mut B,
        new_width: u32,
        new_height: u32,
    ) -> Result<Vec<(String, u32)>, RenderGraphError> {
        let mut existing_slots: std::collections::HashMap<String, Vec<u32>> =
            std::collections::HashMap::new();

        for frame_textures in &self.transient_textures {
            for (&resource_id, texture) in frame_textures {
                if let Some(slot) = B::transient_texture_bindless_slot(texture) {
                    let name = self
                        .resource_name(resource_id)
                        .unwrap_or("unknown")
                        .to_string();
                    existing_slots.entry(name).or_default().push(slot);
                }
            }
        }

        self.transient_textures.clear();
        self.transient_allocation_contract = None;
        self.transient_allocation_contract_validated = false;

        for desc in &mut self.transient_resources {
            if desc.tracks_swapchain_size {
                desc.width = new_width;
                desc.height = new_height;
            }
        }

        self.initialize_transient_textures(backend)?;

        let mut result = Vec::new();
        for (name, slots) in &existing_slots {
            let resource_id = match self.resource_by_name.get(name) {
                Some(&id) => id,
                None => continue,
            };
            for (frame_idx, slot) in slots.iter().enumerate() {
                if let Some(frame_textures) = self.transient_textures.get_mut(frame_idx)
                    && let Some(texture) = frame_textures.get_mut(&resource_id)
                {
                    backend.update_bindless_texture(*slot, texture)?;
                    B::set_transient_texture_bindless_slot(texture, *slot);
                }
            }

            if let Some(&base_slot) = slots.first() {
                result.push((name.clone(), base_slot));
            }
        }

        let new_texture_names: Vec<String> = self
            .transient_resources
            .iter()
            .filter(|desc| !existing_slots.contains_key(&desc.name))
            .map(|desc| desc.name.clone())
            .collect();

        for name in new_texture_names {
            let slot = self.register_transient_texture_bindless(backend, &name)?;
            result.push((name, slot));
        }

        Ok(result)
    }

    /// Update tonemap parameters for a pass.
    pub fn set_tonemap_texture_index(
        &mut self,
        pass_id: PassId,
        texture_index: u32,
    ) -> Result<(), RenderGraphError> {
        let pass_idx = pass_id.0 as usize;
        if pass_idx >= self.passes.len() {
            return Err(RenderGraphError::ResourceNotFound(format!(
                "PassId({}) out of bounds (max {})",
                pass_id.0,
                self.passes.len()
            )));
        }

        if let Some(ref mut params) = self.passes[pass_idx].tonemap_params {
            params.hdr_texture_index = Some(texture_index);
            Ok(())
        } else {
            Err(RenderGraphError::BackendError(format!(
                "PassId({}) is not a tonemap pass (no tonemap_params found)",
                pass_id.0
            )))
        }
    }

    /// Set overlay texture indices for the wallhack overlay pass.
    pub fn set_overlay_texture_indices(
        &mut self,
        pass_id: PassId,
        ldr_texture_index: u32,
        stencil_indicator_index: u32,
    ) -> Result<(), RenderGraphError> {
        let pass_idx = pass_id.0 as usize;
        if pass_idx >= self.passes.len() {
            return Err(RenderGraphError::ResourceNotFound(format!(
                "PassId({}) out of bounds (max {})",
                pass_id.0,
                self.passes.len()
            )));
        }

        if let Some(ref mut params) = self.passes[pass_idx].overlay_params {
            params.ldr_texture_index = Some(ldr_texture_index);
            params.stencil_indicator_index = Some(stencil_indicator_index);
            Ok(())
        } else {
            Err(RenderGraphError::BackendError(format!(
                "PassId({}) is not an overlay pass (no overlay_params found)",
                pass_id.0
            )))
        }
    }
}

impl<B: RenderGraphBackend> Default for FrameGraph<B> {
    fn default() -> Self {
        Self::new()
    }
}

// --- Metal-specific methods ---
#[cfg(target_os = "macos")]
impl FrameGraph<crate::MetalRenderer> {
    /// Collect draw lists from the user closure without executing passes.
    ///
    /// Creates a Frame context, calls the closure to submit draw lists,
    /// and returns the pending draw data for MetalRenderer to execute.
    pub(crate) fn collect_draw_lists<F>(
        &mut self,
        renderer: &mut crate::MetalRenderer,
        f: F,
    ) -> Result<std::collections::HashMap<usize, super::frame::PassExecutionData>, RenderGraphError>
    where
        F: FnOnce(&mut super::frame::Frame<'_, crate::MetalRenderer>),
    {
        self.prepare_external_image_producers(renderer);
        if !self.compiled {
            self.compile()?;
        }

        self.initialize_transient_textures(renderer)?;
        self.initialize_transient_buffers(renderer)?;
        self.prepare_external_buffer_producers(renderer);
        self.compile()?;

        let frame_idx = renderer.frame_index();
        let mut frame = super::frame::Frame::new(self, renderer, 0, frame_idx);
        f(&mut frame);
        frame.validate_submissions()?;

        let pending = std::mem::take(&mut frame.pending);

        // Compile pipeline variants before encoding (Metal encoding runs on
        // &self). Geometry draw lists ensure every drawn material for the
        // pass's declared output format — the same filter as the Vulkan
        // pre-compilation, so side-lists like the depth prepass (whose
        // output format is a pick target, not a shader output) compile
        // nothing. UI pass materials ensure for the drawable format the UI
        // record renders into. Unknown handles skip with a warning, exactly
        // like the encoder skips their draws.
        let ensure_variant = |renderer: &mut crate::MetalRenderer,
                              material: crate::handle::MaterialHandle,
                              format: crate::texture::ImageFormat| {
            if !renderer.has_material_impl(material) {
                log::warn!("Skipping variant compilation for unknown material {material:?}");
                return Ok(());
            }
            renderer
                .ensure_material_variant_impl(material, format)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Pipeline variant pre-compilation failed: {e}"
                    ))
                })
        };
        for (&pass_index, data) in pending.iter() {
            let Some(pass) = self.passes.get(pass_index) else {
                continue;
            };
            let is_geometry = pass.kind == Some(crate::render_graph::pass::PassKind::Geometry);
            let format = pass
                .output_format
                .unwrap_or(crate::texture::ImageFormat::Auto);
            if is_geometry {
                for draw_list in &data.draw_lists {
                    for draw in &draw_list.draws {
                        ensure_variant(renderer, draw.material, format)?;
                    }
                }
            }
            if let Some(material_handle) = pass.material {
                let format = if pass.kind == Some(crate::render_graph::pass::PassKind::Ui) {
                    crate::texture::ImageFormat::B8G8R8A8Srgb
                } else {
                    format
                };
                ensure_variant(renderer, material_handle, format)?;
            }
        }

        Ok(pending)
    }
}

// --- Vulkan-specific methods ---
impl FrameGraph<crate::renderer::VulkanRenderer> {
    /// Resolve deferred materials - compile materials for their pass formats.
    fn resolve_materials(
        &mut self,
        renderer: &mut crate::renderer::VulkanRenderer,
    ) -> Result<(), RenderGraphError> {
        for pass_index in self.execution_order() {
            let pass = &self.passes[pass_index];
            if let Some(material_handle) = pass.material {
                let format = pass
                    .output_format
                    .unwrap_or(crate::texture::ImageFormat::Auto);

                log::trace!(
                    "resolve_materials: pass '{}' material={:?} format={:?}",
                    pass.name,
                    material_handle,
                    format
                );
                renderer
                    .ensure_material_compiled(material_handle, format)
                    .map_err(|e| {
                        RenderGraphError::InvalidConfiguration(format!(
                            "Material compilation failed: {}",
                            e
                        ))
                    })?;
            }
        }

        Ok(())
    }

    /// Execute the graph with the given frame context.
    ///
    /// Called internally by `VulkanRenderer::render()`.
    pub(crate) fn execute(
        &mut self,
        renderer: &mut crate::renderer::VulkanRenderer,
        image_index: u32,
        f: impl FnOnce(&mut super::frame::Frame<'_, crate::renderer::VulkanRenderer>),
    ) -> Result<(), RenderGraphError> {
        self.prepare_external_image_producers(renderer);
        if !self.compiled {
            self.compile()?;
        }

        self.initialize_transient_textures(renderer)?;
        self.initialize_transient_buffers(renderer)?;
        self.prepare_external_buffer_producers(renderer);
        self.compile()?;

        let frame_idx = renderer.current_frame();

        log::trace!(
            "Frame graph execute: frame_idx={}, image_index={}",
            frame_idx,
            image_index
        );

        for pass_index in self.execution_order() {
            let pass = &self.passes[pass_index];
            if let Some(ref params) = pass.tonemap_params
                && let Some(hdr_base_index) = params.hdr_texture_index
            {
                let actual_hdr_index = hdr_base_index + frame_idx as u32;
                let mode_value = params.mode as u32;

                renderer.storage_manager.update_tonemap_params(
                    frame_idx,
                    [
                        params.exposure,
                        params.gamma,
                        mode_value as f32,
                        actual_hdr_index as f32,
                    ],
                );
                #[cfg(debug_assertions)]
                {
                    let rb = renderer.storage_manager.read_tonemap_params(frame_idx);
                    log::debug!(
                        "[TONEMAP VERIFY] wrote [{},{},{},{}] readback [{},{},{},{}]",
                        params.exposure,
                        params.gamma,
                        mode_value,
                        actual_hdr_index,
                        rb[0],
                        rb[1],
                        rb[2],
                        rb[3]
                    );
                }
            }

            if let Some(ref params) = pass.overlay_params {
                let ldr_idx = params
                    .ldr_texture_index
                    .map(|base| base + frame_idx as u32)
                    .unwrap_or(0);
                let indicator_idx = params
                    .stencil_indicator_index
                    .map(|base| base + frame_idx as u32)
                    .unwrap_or(0);

                renderer.storage_manager.update_overlay_params(
                    frame_idx,
                    [ldr_idx as f32, indicator_idx as f32, 0.0, 0.0],
                );
            }
        }

        self.resolve_materials(renderer)?;

        let mut frame = super::frame::Frame::new(self, renderer, image_index, frame_idx);
        if self.trace_enabled {
            frame.enable_execution_trace();
        }
        f(&mut frame);
        frame.validate_submissions()?;
        frame.pre_compile_materials()?;
        frame.execute_passes()?;

        if self.trace_enabled {
            self.last_execution_trace = frame.execution_trace().clone();
        }
        self.record_buffer_execution(renderer);

        Ok(())
    }

    /// Get the ImageView of a transient texture by name (frame 0).
    pub fn transient_texture_view(&self, name: &str) -> Option<ash::vk::ImageView> {
        self.transient_texture(name, 0).map(|t| t.image_view.vk())
    }

    /// Get the ImageView of a transient texture by name for a specific frame.
    pub fn transient_texture_view_for_frame(
        &self,
        name: &str,
        frame_idx: usize,
    ) -> Option<ash::vk::ImageView> {
        self.transient_texture(name, frame_idx)
            .map(|t| t.image_view.vk())
    }
}

/// One imported external image, with its state contract.
#[derive(Debug, Clone)]
struct ImportedResource {
    name: String,
    handle: TextureHandle,
    contract: ImportedImageContract,
}

/// One external renderer-owned buffer imported into the graph namespace.
#[derive(Debug, Clone)]
struct ImportedBuffer {
    name: String,
    handle: BufferHandle,
    desc: BufferDesc,
}

/// Builder for constructing a frame graph.
///
/// Created by [`VulkanRenderer::create_frame_graph()`].
/// Provides a fluent API for adding passes before building the executable [`FrameGraph`].
pub struct FrameGraphBuilder {
    pass_builders: Vec<InternalPassBuilder>,
    resources: Vec<ImportedResource>,
    buffers: Vec<ImportedBuffer>,
    transient_resources: Vec<GraphResourceDesc>,
    transient_buffers: Vec<GraphBufferDesc>,
    exported_resources: BTreeSet<String>,
    backbuffer_contract: ImportedImageContract,
}

impl FrameGraphBuilder {
    /// Create a new frame graph builder.
    pub fn new() -> Self {
        Self {
            pass_builders: Vec::new(),
            resources: Vec::new(),
            buffers: Vec::new(),
            transient_resources: Vec::new(),
            transient_buffers: Vec::new(),
            exported_resources: BTreeSet::from([BACKBUFFER_NAME.to_string()]),
            backbuffer_contract: DEFAULT_BACKBUFFER_CONTRACT,
        }
    }

    /// Add a pass to the graph.
    pub fn add_pass(mut self, pass: impl PassBuilder + 'static) -> Self {
        self.pass_builders.push(pass.as_builder());
        self
    }

    /// Add a pass whose observable effect is not represented by a resource write.
    ///
    /// Side effects are explicit liveness roots. Ordinary render work should expose
    /// an output resource instead, so the compiler can remove unused branches.
    pub fn add_side_effect_pass(mut self, pass: impl PassBuilder + 'static) -> Self {
        let mut pass = pass.as_builder();
        pass.side_effect = true;
        self.pass_builders.push(pass);
        self
    }

    /// Mark a resource's final value as externally observable.
    ///
    /// The swapchain backbuffer is exported by default. Offscreen outputs used for
    /// picking, readback, streaming, or interop must be exported explicitly.
    pub fn export_resource(mut self, name: impl Into<String>) -> Self {
        self.exported_resources.insert(name.into());
        self
    }

    /// Import an external image into the graph with an explicit state contract.
    ///
    /// `contract.initial` declares the state the image arrives in; loading its
    /// contents from a pass requires a non-`Undefined` initial state.
    /// `contract.required_final` declares the state the graph must leave the
    /// image in (e.g. `ResourceState::PresentSrc` for an image presented
    /// after the frame). Use `ImportedImageContract::undefined()` when
    /// neither side of the contract is observable.
    pub fn import_resource(
        mut self,
        name: impl Into<String>,
        handle: TextureHandle,
        contract: ImportedImageContract,
    ) -> Self {
        self.resources.push(ImportedResource {
            name: name.into(),
            handle,
            contract,
        });
        self
    }

    /// Import a renderer-owned typed buffer into the graph under a name.
    pub fn import_buffer(
        mut self,
        name: impl Into<String>,
        handle: BufferHandle,
        desc: BufferDesc,
    ) -> Self {
        self.buffers.push(ImportedBuffer {
            name: name.into(),
            handle,
            desc,
        });
        self
    }

    /// Override the state contract of the built-in backbuffer.
    ///
    /// By default the backbuffer is imported with observable contents (the
    /// previously presented frame), so passes may load it without an in-graph
    /// producer. Applications that present the backbuffer declare the final
    /// state here, e.g.
    /// `backbuffer_contract(ImportedImageContract::arrives_in(ResourceState::ColorAttachment).must_end_in(ResourceState::PresentSrc))`.
    pub fn backbuffer_contract(mut self, contract: ImportedImageContract) -> Self {
        self.backbuffer_contract = contract;
        self
    }

    /// Create a transient resource in the frame graph.
    pub fn create_resource(mut self, desc: GraphResourceDesc) -> Self {
        self.transient_resources.push(desc);
        self
    }

    /// Create a graph-owned transient buffer.
    pub fn create_buffer(mut self, desc: GraphBufferDesc) -> Self {
        self.transient_buffers.push(desc);
        self
    }

    fn validate_buffer_accesses(&self) -> Result<(), RenderGraphError> {
        let buffer_names = self
            .transient_buffers
            .iter()
            .map(|buffer| buffer.name.as_str())
            .chain(self.buffers.iter().map(|buffer| buffer.name.as_str()))
            .collect::<HashSet<_>>();
        for pass in &self.pass_builders {
            let typed_buffer_names = pass
                .buffer_accesses
                .iter()
                .map(|access| access.resource.as_str())
                .collect::<HashSet<_>>();
            for resource in pass.reads.iter().chain(&pass.writes) {
                if buffer_names.contains(resource.as_str())
                    && !typed_buffer_names.contains(resource.as_str())
                {
                    return Err(GraphValidationError::MissingTypedBufferAccess {
                        pass: pass.name.clone(),
                        resource: resource.clone(),
                    }
                    .into());
                }
            }
            for access in &pass.image_accesses {
                if buffer_names.contains(access.resource.as_str()) {
                    return Err(GraphValidationError::ImageAccessOnNonImage {
                        pass: pass.name.clone(),
                        resource: access.resource.clone(),
                    }
                    .into());
                }
            }
            for access in &pass.buffer_accesses {
                let desc = self
                    .transient_buffers
                    .iter()
                    .find(|buffer| buffer.name == access.resource)
                    .map(|buffer| buffer.buffer)
                    .or_else(|| {
                        self.buffers
                            .iter()
                            .find(|buffer| buffer.name == access.resource)
                            .map(|buffer| buffer.desc)
                    })
                    .ok_or_else(|| GraphValidationError::BufferAccessOnNonBuffer {
                        pass: pass.name.clone(),
                        resource: access.resource.clone(),
                    })?;

                validate_buffer_access_descriptor(
                    &pass.name,
                    &access.resource,
                    access.mode,
                    access.usage,
                    access.stage,
                    access.range,
                    desc,
                )?;
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), RenderGraphError> {
        let mut resource_names = HashSet::from([BACKBUFFER_NAME.to_string()]);

        for desc in &self.transient_resources {
            if desc.name.trim().is_empty() {
                return Err(GraphValidationError::EmptyResourceName.into());
            }
            if desc.width == 0 || desc.height == 0 {
                return Err(GraphValidationError::InvalidResourceExtent {
                    resource: desc.name.clone(),
                    width: desc.width,
                    height: desc.height,
                }
                .into());
            }
            if !resource_names.insert(desc.name.clone()) {
                return Err(GraphValidationError::DuplicateResourceName(desc.name.clone()).into());
            }
        }

        for desc in &self.transient_buffers {
            if desc.name.trim().is_empty() {
                return Err(GraphValidationError::EmptyResourceName.into());
            }
            if desc.buffer.size == 0 || desc.buffer.usages.is_empty() {
                return Err(GraphValidationError::InvalidBufferDescriptor {
                    resource: desc.name.clone(),
                    size: desc.buffer.size,
                }
                .into());
            }
            if !resource_names.insert(desc.name.clone()) {
                return Err(GraphValidationError::DuplicateResourceName(desc.name.clone()).into());
            }
        }

        let mut buffer_identities = HashMap::new();
        for buffer in &self.buffers {
            if buffer.name.trim().is_empty() {
                return Err(GraphValidationError::EmptyResourceName.into());
            }
            if buffer.handle.is_none() {
                return Err(
                    GraphValidationError::InvalidImportedBuffer(buffer.name.clone()).into(),
                );
            }
            if buffer.desc.size == 0 || buffer.desc.usages.is_empty() {
                return Err(GraphValidationError::InvalidBufferDescriptor {
                    resource: buffer.name.clone(),
                    size: buffer.desc.size,
                }
                .into());
            }
            if !resource_names.insert(buffer.name.clone()) {
                return Err(
                    GraphValidationError::DuplicateResourceName(buffer.name.clone()).into(),
                );
            }
            if let Some(first) = buffer_identities.insert(
                (buffer.handle.index(), buffer.handle.generation()),
                buffer.name.clone(),
            ) {
                return Err(GraphValidationError::DuplicateImportedIdentity {
                    kind: "buffer",
                    first,
                    duplicate: buffer.name.clone(),
                }
                .into());
            }
        }

        let mut image_identities = HashMap::new();
        for resource in &self.resources {
            if resource.name.trim().is_empty() {
                return Err(GraphValidationError::EmptyResourceName.into());
            }
            if resource.handle.is_none() {
                return Err(
                    GraphValidationError::InvalidImportedResource(resource.name.clone()).into(),
                );
            }
            if !resource_names.insert(resource.name.clone()) {
                return Err(
                    GraphValidationError::DuplicateResourceName(resource.name.clone()).into(),
                );
            }
            if let Some(first) = image_identities.insert(
                (resource.handle.index(), resource.handle.generation()),
                resource.name.clone(),
            ) {
                return Err(GraphValidationError::DuplicateImportedIdentity {
                    kind: "image",
                    first,
                    duplicate: resource.name.clone(),
                }
                .into());
            }
        }

        for resource in &self.exported_resources {
            if !resource_names.contains(resource) {
                return Err(
                    GraphValidationError::UndeclaredExportedResource(resource.clone()).into(),
                );
            }
        }

        let mut pass_names = HashSet::new();
        for pass in &self.pass_builders {
            if pass.name.trim().is_empty() {
                return Err(GraphValidationError::EmptyPassName.into());
            }
            if !pass_names.insert(pass.name.clone()) {
                return Err(GraphValidationError::DuplicatePassName(pass.name.clone()).into());
            }

            for resource in pass
                .reads
                .iter()
                .chain(&pass.writes)
                .chain(pass.image_accesses.iter().map(|access| &access.resource))
                .chain(pass.buffer_accesses.iter().map(|access| &access.resource))
            {
                if resource.trim().is_empty() {
                    return Err(GraphValidationError::EmptyPassResource {
                        pass: pass.name.clone(),
                    }
                    .into());
                }
                if !resource_names.contains(resource) {
                    return Err(GraphValidationError::UndeclaredResource {
                        pass: pass.name.clone(),
                        resource: resource.clone(),
                    }
                    .into());
                }
            }
        }

        self.validate_buffer_accesses()?;

        Ok(())
    }

    /// Build the frame graph after validating its complete resource namespace.
    pub fn build<B: RenderGraphBackend>(self) -> Result<FrameGraph<B>, RenderGraphError> {
        self.validate()?;

        let FrameGraphBuilder {
            pass_builders,
            resources,
            buffers,
            transient_resources,
            transient_buffers,
            exported_resources,
            backbuffer_contract,
        } = self;

        let transient_names = transient_resources
            .iter()
            .map(|desc| desc.name.clone())
            .collect::<Vec<_>>();
        let buffer_names = transient_buffers
            .iter()
            .map(|desc| desc.name.clone())
            .chain(buffers.iter().map(|buffer| buffer.name.clone()))
            .collect::<Vec<_>>();

        let mut graph = FrameGraph::new();
        graph.transient_resources = transient_resources;
        graph.transient_buffers = transient_buffers;

        // The swapchain backbuffer is the only built-in resource. Every other
        // name has already been declared or imported by the validated builder.
        let backbuffer_id = graph.create_resource_id(BACKBUFFER_NAME);
        graph
            .imported_contracts
            .insert(backbuffer_id, backbuffer_contract);
        for name in transient_names {
            graph.create_resource_id(name);
        }
        for name in buffer_names {
            graph.create_resource_id(name);
        }
        for resource in &resources {
            let id = graph.create_resource_id(resource.name.clone());
            graph.imported_contracts.insert(id, resource.contract);
            graph.imported_images.insert(id, resource.handle);
        }
        for buffer in buffers {
            let id = graph.create_resource_id(buffer.name);
            graph.buffer_desc_by_id.insert(id, buffer.desc);
            graph.imported_buffers.insert(id, buffer.handle);
        }
        for buffer in &graph.transient_buffers {
            let id = graph.resource_by_name[&buffer.name];
            graph.buffer_desc_by_id.insert(id, buffer.buffer);
        }

        let mut global_resource_map = HashMap::new();
        for (name, &resource_id) in &graph.resource_by_name {
            global_resource_map.insert(name.clone(), GraphResourceHandle::new(resource_id.0));
        }

        let exported_resource_ids = exported_resources
            .iter()
            .map(|name| {
                graph.resource_by_name.get(name).copied().ok_or_else(|| {
                    RenderGraphError::ResourceNotFound(format!(
                        "Exported resource '{}' was not created",
                        name
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        graph.configure_pass_culling(exported_resource_ids);

        for pass_builder in pass_builders {
            let pass_data = (pass_builder.build_fn)(&global_resource_map)?;
            let pass_name = pass_builder.name.clone();

            let read_ids = pass_builder
                .reads
                .iter()
                .map(|name| {
                    graph.resource_by_name.get(name).copied().ok_or_else(|| {
                        RenderGraphError::Validation(GraphValidationError::UndeclaredResource {
                            pass: pass_name.clone(),
                            resource: name.clone(),
                        })
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let write_ids = pass_builder
                .writes
                .iter()
                .map(|name| {
                    graph.resource_by_name.get(name).copied().ok_or_else(|| {
                        RenderGraphError::Validation(GraphValidationError::UndeclaredResource {
                            pass: pass_name.clone(),
                            resource: name.clone(),
                        })
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let explicit_image_accesses = pass_builder
                .image_accesses
                .iter()
                .map(|access| {
                    graph
                        .resource_by_name
                        .get(&access.resource)
                        .copied()
                        .map(|resource| access.resolve(resource))
                        .ok_or_else(|| {
                            RenderGraphError::Validation(GraphValidationError::UndeclaredResource {
                                pass: pass_name.clone(),
                                resource: access.resource.clone(),
                            })
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let has_explicit_image_accesses = !explicit_image_accesses.is_empty();

            let explicit_buffer_accesses = pass_builder
                .buffer_accesses
                .iter()
                .map(|access| {
                    graph
                        .resource_by_name
                        .get(&access.resource)
                        .copied()
                        .map(|resource| access.resolve(resource))
                        .ok_or_else(|| {
                            RenderGraphError::Validation(GraphValidationError::UndeclaredResource {
                                pass: pass_name.clone(),
                                resource: access.resource.clone(),
                            })
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let mut pass = PassDesc::new(
                pass_builder.name,
                pass_builder.pass_type,
                read_ids,
                write_ids,
            );

            if has_explicit_image_accesses {
                pass.set_image_accesses(explicit_image_accesses);
            }
            pass.set_buffer_accesses(explicit_buffer_accesses);

            pass.pipeline = pass_builder.pipeline;
            pass.tonemap_params = pass_builder.tonemap_params;
            pass.overlay_params = pass_builder.overlay_params;
            pass.material = pass_builder.material;
            pass.output_format = pass_builder.output_format;
            pass.uses_depth = pass_builder.uses_depth;
            pass.depth_target = pass_builder
                .depth_target
                .as_ref()
                .map(|name| {
                    graph
                        .resource_id(name)
                        .ok_or_else(|| RenderGraphError::ResourceNotFound(name.clone()))
                })
                .transpose()?;
            pass.depth_attachment = pass_builder.depth_attachment;
            pass.kind = pass_builder.kind;
            pass.side_effect = pass_builder.side_effect;
            if let Some(commands) = pass_data.downcast_ref::<Vec<super::compute::ComputeCommand>>()
            {
                pass.commands = commands.clone();
            }

            pass.color_attachments = pass_builder
                .color_attachments
                .iter()
                .map(|(name, ops)| {
                    graph
                        .resource_by_name
                        .get(name)
                        .copied()
                        .map(|resource| (resource, *ops))
                        .ok_or_else(|| {
                            RenderGraphError::Validation(GraphValidationError::UndeclaredResource {
                                pass: pass_name.clone(),
                                resource: name.clone(),
                            })
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;

            // Every graphics pass that uses depth gets an explicit depth
            // contract: the canonical reverse-Z default when the template
            // declares nothing. Execution never guesses.
            if pass_builder.pass_type == PassType::Graphics
                && pass.uses_depth
                && pass.depth_attachment.is_none()
            {
                pass.depth_attachment = Some(DepthStencilAttachmentOps::reverse_z_default());
            }

            if !has_explicit_image_accesses {
                pass.refine_inferred_image_accesses();
            }
            if let Some(resource) = pass.depth_target {
                let ops = pass.depth_attachment.ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Pass '{}' declares a depth target without attachment operations",
                        pass.name
                    ))
                })?;
                let mut access = super::access::ImageAccess::depth_attachment_write(resource);
                if ops.depth.load == LoadOp::Load || ops.stencil.load == LoadOp::Load {
                    access.mode = ResourceAccessMode::ReadWrite;
                }
                pass.image_accesses.retain(|existing| {
                    existing.resource != resource
                        || existing.usage
                            != super::access::ResourceAccessUsage::DepthStencilAttachment
                });
                if graph
                    .resource_format_for_target(resource)
                    .is_some_and(|format| matches!(format, crate::texture::ImageFormat::D32Sfloat))
                {
                    access.range = super::access::ImageSubresourceRange::WHOLE_DEPTH;
                }
                pass.image_accesses.push(access);
                pass.set_image_accesses(pass.image_accesses.clone());
            }

            if let Some(comp_data) =
                pass_data.downcast_ref::<crate::render_graph::passes::CompositePassData>()
            {
                pass.compositing_viewports = Some(comp_data.viewports.clone());
            }

            graph.add_pass(pass);
        }

        graph.compile()?;
        Ok(graph)
    }
}

impl Default for FrameGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::pass::PassType;
    use super::*;
    use crate::render_graph::backend::RenderGraphBackend;
    use crate::render_graph::resource::ResourceState;

    fn rid(n: u32) -> ResourceId {
        ResourceId(n)
    }

    /// A trivial mock backend for testing FrameGraph without a GPU.
    #[derive(Clone)]
    struct MockBackend {
        /// Member count of each `create_transient_slot` call, in call order.
        slot_member_counts: std::rc::Rc<std::cell::RefCell<Vec<usize>>>,
        policies: std::rc::Rc<std::cell::RefCell<Vec<super::super::backend::TransientSlotPolicy>>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                slot_member_counts: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
                policies: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            }
        }
    }

    struct MockTexture {
        slot: std::cell::Cell<Option<u32>>,
    }

    struct MockBuffer {
        desc: BufferDesc,
    }

    #[derive(Clone)]
    struct MockImageView;

    unsafe impl Send for MockImageView {}
    unsafe impl Sync for MockImageView {}

    impl RenderGraphBackend for MockBackend {
        type TransientTexture = MockTexture;
        type ImageView = MockImageView;
        type TransientBuffer = MockBuffer;

        fn create_transient_slot(
            &self,
            members: &[super::super::resource::GraphResourceDesc],
            policy: super::super::backend::TransientSlotPolicy,
        ) -> Result<Vec<Self::TransientTexture>, RenderGraphError> {
            self.slot_member_counts.borrow_mut().push(members.len());
            self.policies.borrow_mut().push(policy);
            Ok(members
                .iter()
                .map(|_| MockTexture {
                    slot: std::cell::Cell::new(None),
                })
                .collect())
        }

        fn create_transient_buffer(
            &self,
            desc: BufferDesc,
        ) -> Result<Self::TransientBuffer, RenderGraphError> {
            Ok(MockBuffer { desc })
        }

        fn destroy_transient_texture(_texture: Self::TransientTexture) {}

        fn destroy_transient_buffer(_buffer: Self::TransientBuffer) {}

        fn transient_buffer_size(buffer: &Self::TransientBuffer) -> u64 {
            buffer.desc.size
        }

        fn buffer_desc(buffer: &Self::TransientBuffer) -> BufferDesc {
            buffer.desc
        }

        fn buffer_by_handle(
            &self,
            _handle: crate::handle::BufferHandle,
        ) -> Option<&Self::TransientBuffer> {
            None
        }

        fn current_frame(&self) -> usize {
            0
        }

        fn transient_texture_frames() -> usize {
            2
        }

        fn register_bindless_texture(
            &mut self,
            _texture: &Self::TransientTexture,
        ) -> Result<u32, RenderGraphError> {
            Ok(0)
        }

        fn update_bindless_texture(
            &mut self,
            _slot: u32,
            _texture: &Self::TransientTexture,
        ) -> Result<(), RenderGraphError> {
            Ok(())
        }

        fn transient_texture_format(
            _texture: &Self::TransientTexture,
        ) -> crate::texture::ImageFormat {
            crate::texture::ImageFormat::R8G8B8A8Unorm
        }

        fn transient_texture_extent(_texture: &Self::TransientTexture) -> (u32, u32) {
            (1, 1)
        }

        fn transient_texture_is_depth(_texture: &Self::TransientTexture) -> bool {
            false
        }

        fn transient_texture_bindless_slot(texture: &Self::TransientTexture) -> Option<u32> {
            texture.slot.get()
        }

        fn set_transient_texture_bindless_slot(texture: &mut Self::TransientTexture, slot: u32) {
            texture.slot.set(Some(slot));
        }

        fn transient_texture_view(_texture: &Self::TransientTexture) -> Self::ImageView {
            MockImageView
        }

        fn swapchain_image_view(&self, _image_index: u32) -> Self::ImageView {
            MockImageView
        }

        fn depth_image_view(&self, _frame_index: usize) -> Option<Self::ImageView> {
            None
        }
    }

    type TestGraph = FrameGraph<MockBackend>;

    #[test]
    fn test_buffer_declarations_reject_incompatible_pipeline_stages() {
        use crate::render_graph::{ResourceAccessStage, SimplePass};

        for (usage, capability, stage) in [
            (
                BufferUsage::Vertex,
                BufferUsages::VERTEX,
                ResourceAccessStage::VertexShader,
            ),
            (
                BufferUsage::Index,
                BufferUsages::INDEX,
                ResourceAccessStage::FragmentShader,
            ),
            (
                BufferUsage::Indirect,
                BufferUsages::INDIRECT,
                ResourceAccessStage::ComputeShader,
            ),
            (
                BufferUsage::Uniform,
                BufferUsages::UNIFORM,
                ResourceAccessStage::Transfer,
            ),
            (
                BufferUsage::Storage,
                BufferUsages::STORAGE,
                ResourceAccessStage::DepthStencil,
            ),
            (
                BufferUsage::TransferSource,
                BufferUsages::TRANSFER_SOURCE,
                ResourceAccessStage::AllGraphics,
            ),
            (
                BufferUsage::Readback,
                BufferUsages::READBACK,
                ResourceAccessStage::Transfer,
            ),
        ] {
            let desc = BufferDesc::new(64, capability, BufferMemoryPolicy::Readback);
            let pass = SimplePass::new("consume", PassType::Graphics).buffer_access(
                "data",
                ResourceAccessMode::Read,
                usage,
                stage,
                BufferByteRange::WHOLE,
            );
            let result = FrameGraphBuilder::new()
                .create_buffer(GraphBufferDesc::new("data", desc))
                .add_pass(pass)
                .build::<MockBackend>();
            assert!(result.is_err(), "accepted {usage:?} at {stage:?}");
        }
    }

    #[test]
    fn test_buffer_stage_validation_survives_graph_mutation() {
        use crate::render_graph::{BufferAccess, ResourceAccessStage, SimplePass};

        let mut graph = FrameGraphBuilder::new()
            .import_buffer(
                "data",
                crate::BufferHandle::from_raw(1, 0),
                BufferDesc::new(64, BufferUsages::UNIFORM, BufferMemoryPolicy::CpuVisible),
            )
            .add_pass(
                SimplePass::new("consume", PassType::Graphics).buffer_access(
                    "data",
                    ResourceAccessMode::Read,
                    BufferUsage::Uniform,
                    ResourceAccessStage::VertexShader,
                    BufferByteRange::WHOLE,
                ),
            )
            .build::<MockBackend>()
            .unwrap();
        let data = graph.resource_id("data").unwrap();
        graph.add_pass(
            PassDesc::new("invalid", PassType::Graphics, vec![], vec![]).with_buffer_accesses([
                BufferAccess::uniform_read(data).with_stage(ResourceAccessStage::Transfer),
            ]),
        );
        assert!(matches!(
            graph.compile(),
            Err(RenderGraphError::Validation(
                GraphValidationError::InvalidBufferAccessStage {
                    stage: ResourceAccessStage::Transfer,
                    ..
                }
            ))
        ));
    }

    #[test]
    fn test_buffer_helpers_build_valid_graphs_for_all_consumers() {
        use crate::render_graph::{BufferAccess, SimplePass};

        let data = ResourceId(0);
        for (access, usages, memory) in [
            (
                BufferAccess::uniform_read(data),
                BufferUsages::UNIFORM,
                BufferMemoryPolicy::CpuVisible,
            ),
            (
                BufferAccess::storage_read_write(data),
                BufferUsages::STORAGE,
                BufferMemoryPolicy::DeviceLocal,
            ),
            (
                BufferAccess::vertex_read(data),
                BufferUsages::VERTEX,
                BufferMemoryPolicy::DeviceLocal,
            ),
            (
                BufferAccess::index_read(data),
                BufferUsages::INDEX,
                BufferMemoryPolicy::DeviceLocal,
            ),
            (
                BufferAccess::indirect_read(data),
                BufferUsages::INDIRECT,
                BufferMemoryPolicy::DeviceLocal,
            ),
            (
                BufferAccess::transfer_read(data),
                BufferUsages::TRANSFER_SOURCE,
                BufferMemoryPolicy::DeviceLocal,
            ),
            (
                BufferAccess::transfer_write(data),
                BufferUsages::TRANSFER_DESTINATION,
                BufferMemoryPolicy::DeviceLocal,
            ),
            (
                BufferAccess::readback_read(data),
                BufferUsages::READBACK,
                BufferMemoryPolicy::Readback,
            ),
        ] {
            FrameGraphBuilder::new()
                .create_buffer(GraphBufferDesc::new(
                    "data",
                    BufferDesc::new(64, usages, memory),
                ))
                .add_pass(
                    SimplePass::new("consume", PassType::Graphics).buffer_access(
                        "data",
                        access.mode,
                        access.usage,
                        access.stage,
                        access.range,
                    ),
                )
                .build::<MockBackend>()
                .unwrap();
        }
    }

    #[test]
    fn test_buffer_diagnostics_describe_transient_and_imported_allocations() {
        use crate::render_graph::{ResourceAccessStage, SimplePass};

        let graph = FrameGraphBuilder::new()
            .create_buffer(GraphBufferDesc::new(
                "scratch",
                BufferDesc::new(
                    1024,
                    BufferUsages::STORAGE | BufferUsages::TRANSFER_SOURCE,
                    BufferMemoryPolicy::DeviceLocal,
                ),
            ))
            .import_buffer(
                "readback",
                crate::BufferHandle::from_raw(9, 2),
                BufferDesc::new(
                    512,
                    BufferUsages::READBACK | BufferUsages::TRANSFER_DESTINATION,
                    BufferMemoryPolicy::Readback,
                ),
            )
            .add_side_effect_pass(
                SimplePass::new("compute", PassType::Graphics).buffer_access(
                    "scratch",
                    ResourceAccessMode::Write,
                    BufferUsage::Storage,
                    ResourceAccessStage::ComputeShader,
                    BufferByteRange::new(128, 256),
                ),
            )
            .add_side_effect_pass(
                SimplePass::new("copy", PassType::Graphics)
                    .buffer_access(
                        "scratch",
                        ResourceAccessMode::Read,
                        BufferUsage::TransferSource,
                        ResourceAccessStage::Transfer,
                        BufferByteRange::new(128, 256),
                    )
                    .buffer_access(
                        "readback",
                        ResourceAccessMode::Write,
                        BufferUsage::TransferDestination,
                        ResourceAccessStage::Transfer,
                        BufferByteRange::new(0, 256),
                    ),
            )
            .build::<MockBackend>()
            .unwrap();
        let diagnostics = graph.diagnostics().unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&diagnostics.to_json_pretty().unwrap()).unwrap();
        assert_eq!(json["resources"][1]["kind"], "buffer");
        assert_eq!(json["resources"][1]["origin"], "transient");
        assert_eq!(
            json["resources"][1]["buffer"],
            serde_json::json!({
                "size": 1024, "usages": ["storage", "transfer_source"], "memory": "device_local",
            })
        );
        assert_eq!(json["resources"][2]["origin"], "imported");
        assert_eq!(
            json["resources"][2]["buffer"],
            serde_json::json!({
                "size": 512, "usages": ["transfer_destination", "readback"], "memory": "readback",
            })
        );
        assert_eq!(json["resources"][1]["width"], serde_json::Value::Null);
        assert_eq!(
            json["resources"][1]["physical_allocation_id"],
            serde_json::Value::Null
        );
        assert_eq!(json["resources"][1]["lifetime"]["last_pass"], 1);
        let text = diagnostics.to_string();
        assert!(text.contains("r1 (scratch) transient buffer, 1024 bytes, DeviceLocal, usages [Storage, TransferSource]"));
        let dot = diagnostics.to_dot();
        assert!(dot.contains("buffer 1024 bytes, DeviceLocal, usages [Storage, TransferSource]"));
        for _ in 0..8 {
            assert_eq!(
                diagnostics.to_json_pretty().unwrap(),
                graph.diagnostics().unwrap().to_json_pretty().unwrap()
            );
        }
    }

    #[test]
    fn test_frame_graph_add_and_index_passes() {
        let mut graph = TestGraph::new();
        let p1 = PassDesc::new("a", PassType::Graphics, vec![], vec![rid(1)]);
        let p2 = PassDesc::new("b", PassType::Graphics, vec![rid(1)], vec![rid(2)]);

        graph.add_pass(p1);
        graph.add_pass(p2);

        assert_eq!(graph.pass_count(), 2);
        assert_eq!(graph.pass_index("a"), Some(0));
        assert_eq!(graph.pass_index("b"), Some(1));
        assert_eq!(graph.pass_index("nonexistent"), None);
    }

    #[test]
    fn test_frame_graph_insert_pass_reindexes() {
        let mut graph = TestGraph::new();
        graph.add_pass(PassDesc::new("a", PassType::Graphics, vec![], vec![]));
        graph.add_pass(PassDesc::new("b", PassType::Graphics, vec![], vec![]));

        graph.insert_pass(
            1,
            PassDesc::new("inserted", PassType::Graphics, vec![], vec![]),
        );

        assert_eq!(graph.pass_count(), 3);
        assert_eq!(graph.pass_index("a"), Some(0));
        assert_eq!(graph.pass_index("inserted"), Some(1));
        assert_eq!(graph.pass_index("b"), Some(2));
    }

    #[test]
    fn test_frame_graph_add_pass_resets_compiled() {
        let mut graph = TestGraph::new();
        graph.add_pass(PassDesc::new("a", PassType::Graphics, vec![], vec![]));
        graph.compile().unwrap();
        assert!(graph.compiled);

        graph.add_pass(PassDesc::new("b", PassType::Graphics, vec![], vec![]));
        assert!(!graph.compiled);
        assert!(graph.execution_plan.is_none());
    }

    #[test]
    fn test_frame_graph_builder_with_resources() {
        let builder = FrameGraphBuilder::new().import_resource(
            "ext",
            TextureHandle::from_raw(42, 0),
            ImportedImageContract::undefined(),
        );

        assert_eq!(builder.resources.len(), 1);
    }

    #[test]
    fn test_resource_id_lookup() {
        let mut graph = TestGraph::new();
        let id = graph.create_resource_id("hdr_color");
        assert_eq!(graph.resource_id("hdr_color"), Some(id));
        assert_eq!(graph.resource_name(id), Some("hdr_color"));
        assert_eq!(graph.resource_id("nonexistent"), None);
    }

    fn validation_resource(name: &str, width: u32, height: u32) -> GraphResourceDesc {
        GraphResourceDesc {
            name: name.to_string(),
            resource_type: super::super::resource::GraphResourceType::ColorAttachment {
                clear_value: None,
            },
            format: crate::texture::ImageFormat::R8G8B8A8Unorm,
            width,
            height,
            tracks_swapchain_size: true,
        }
    }

    fn validation_error(builder: FrameGraphBuilder) -> GraphValidationError {
        match builder.build::<MockBackend>() {
            Err(RenderGraphError::Validation(error)) => error,
            Err(error) => panic!("expected graph validation error, got {error}"),
            Ok(_) => panic!("expected graph validation to fail"),
        }
    }

    #[test]
    fn builder_rejects_duplicate_resource_names() {
        let error = validation_error(
            FrameGraphBuilder::new()
                .create_resource(validation_resource("color", 1, 1))
                .import_resource(
                    "color",
                    TextureHandle::from_raw(7, 0),
                    ImportedImageContract::undefined(),
                ),
        );
        assert_eq!(
            error,
            GraphValidationError::DuplicateResourceName("color".to_string())
        );
    }

    #[test]
    fn builder_rejects_repeated_imports() {
        let error = validation_error(
            FrameGraphBuilder::new()
                .import_resource(
                    "external",
                    TextureHandle::from_raw(1, 0),
                    ImportedImageContract::undefined(),
                )
                .import_resource(
                    "external",
                    TextureHandle::from_raw(2, 0),
                    ImportedImageContract::undefined(),
                ),
        );
        assert_eq!(
            error,
            GraphValidationError::DuplicateResourceName("external".to_string())
        );
    }

    #[test]
    fn builder_rejects_duplicate_pass_names() {
        let error = validation_error(
            FrameGraphBuilder::new()
                .add_pass(super::super::builder::SimplePass::new(
                    "same",
                    PassType::Graphics,
                ))
                .add_pass(super::super::builder::SimplePass::new(
                    "same",
                    PassType::Graphics,
                )),
        );
        assert_eq!(
            error,
            GraphValidationError::DuplicatePassName("same".to_string())
        );
    }

    #[test]
    fn builder_rejects_undeclared_pass_resources() {
        let error = validation_error(
            FrameGraphBuilder::new().add_pass(
                super::super::builder::SimplePass::new("geometry", PassType::Graphics)
                    .write("typo_color"),
            ),
        );
        assert_eq!(
            error,
            GraphValidationError::UndeclaredResource {
                pass: "geometry".to_string(),
                resource: "typo_color".to_string(),
            }
        );
    }

    #[test]
    fn builder_rejects_invalid_resource_descriptors_and_imports() {
        assert_eq!(
            validation_error(
                FrameGraphBuilder::new().create_resource(validation_resource("color", 0, 64))
            ),
            GraphValidationError::InvalidResourceExtent {
                resource: "color".to_string(),
                width: 0,
                height: 64,
            }
        );
        assert_eq!(
            validation_error(FrameGraphBuilder::new().import_resource(
                "external",
                TextureHandle::NONE,
                ImportedImageContract::undefined(),
            )),
            GraphValidationError::InvalidImportedResource("external".to_string())
        );
    }

    // --- Attachment operation validation (#95) ---

    use crate::render_pass::{AttachmentOps, ClearValue};

    fn missing_ops_error(builder: FrameGraphBuilder) -> GraphValidationError {
        match builder.build::<MockBackend>() {
            Err(RenderGraphError::Validation(error)) => error,
            Err(error) => panic!("expected graph validation error, got {error}"),
            Ok(_) => panic!("expected graph validation error, graph compiled"),
        }
    }

    #[test]
    fn writing_an_attachment_without_declared_ops_fails_validation() {
        let error = missing_ops_error(
            FrameGraphBuilder::new()
                .create_resource(validation_resource("color", 64, 64))
                .export_resource("color")
                .add_pass(
                    super::super::builder::SimplePass::new("paint", PassType::Graphics)
                        .write("color"),
                ),
        );
        assert!(matches!(
            error,
            GraphValidationError::MissingAttachmentOps { pass, resource }
                if pass == "paint" && resource == "color"
        ));
    }

    #[test]
    fn writing_the_backbuffer_without_declared_ops_fails_validation() {
        let error = missing_ops_error(
            FrameGraphBuilder::new().add_pass(
                super::super::builder::SimplePass::new("present", PassType::Graphics)
                    .write(BACKBUFFER_NAME),
            ),
        );
        assert!(matches!(
            error,
            GraphValidationError::MissingAttachmentOps { resource, .. } if resource == BACKBUFFER_NAME
        ));
    }

    #[test]
    fn ops_targeting_unwritten_resources_fail_validation() {
        let error = missing_ops_error(
            FrameGraphBuilder::new()
                .create_resource(validation_resource("color", 64, 64))
                .create_resource(validation_resource("other", 64, 64))
                .export_resource("color")
                .add_pass(
                    super::super::builder::SimplePass::new("paint", PassType::Graphics)
                        .write("color")
                        .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                        .attachment("other", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
                ),
        );
        assert!(matches!(
            error,
            GraphValidationError::StrayAttachmentOps { pass, resource }
                if pass == "paint" && resource == "other"
        ));
    }

    #[test]
    fn clear_op_rejects_depth_clear_value_on_a_color_target() {
        let error = missing_ops_error(
            FrameGraphBuilder::new()
                .create_resource(validation_resource("color", 64, 64))
                .export_resource("color")
                .add_pass(
                    super::super::builder::SimplePass::new("paint", PassType::Graphics)
                        .write("color")
                        .attachment("color", AttachmentOps::clear(ClearValue::DEFAULT_DEPTH)),
                ),
        );
        assert!(matches!(
            error,
            GraphValidationError::AttachmentClearValueAspect {
                expected: "a color clear value",
                ..
            }
        ));
    }

    #[test]
    fn loading_an_unproduced_transient_fails_validation() {
        let error = missing_ops_error(
            FrameGraphBuilder::new()
                .create_resource(validation_resource("history", 64, 64))
                .export_resource("history")
                .add_pass(
                    super::super::builder::SimplePass::new("first", PassType::Graphics)
                        .write("history")
                        .attachment("history", AttachmentOps::load()),
                ),
        );
        assert!(matches!(
            error,
            GraphValidationError::LoadingUndefinedAttachment { pass, resource }
                if pass == "first" && resource == "history"
        ));
    }

    #[test]
    fn two_pass_accumulation_compiles() {
        let graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .export_resource("color")
            .add_pass(
                super::super::builder::SimplePass::new("paint", PassType::Graphics)
                    .write("color")
                    .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
            )
            .add_pass(
                super::super::builder::SimplePass::new("extend", PassType::Graphics)
                    .read("color")
                    .write("color")
                    .attachment("color", AttachmentOps::load()),
            )
            .build::<MockBackend>()
            .unwrap();
        assert_eq!(graph.execution_order().len(), 2);
    }

    #[test]
    fn imported_backbuffer_may_load_without_an_in_graph_producer() {
        let graph = FrameGraphBuilder::new()
            .add_pass(
                super::super::builder::SimplePass::new("overlay", PassType::Graphics)
                    .read(BACKBUFFER_NAME)
                    .write(BACKBUFFER_NAME)
                    .attachment(BACKBUFFER_NAME, AttachmentOps::load()),
            )
            .build::<MockBackend>()
            .unwrap();
        assert_eq!(graph.execution_order().len(), 1);
    }

    #[test]
    fn compute_passes_reject_attachment_ops() {
        let error = missing_ops_error(
            FrameGraphBuilder::new()
                .create_resource(validation_resource("color", 64, 64))
                .export_resource("color")
                .add_pass(
                    super::super::builder::SimplePass::new("dispatch", PassType::Compute)
                        .write("color")
                        .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
                ),
        );
        assert!(matches!(
            error,
            GraphValidationError::AttachmentOpsOnComputePass(ref pass) if pass == "dispatch"
        ));
    }

    #[test]
    fn depth_ops_without_depth_use_fail_validation() {
        let mut builder = super::super::builder::SimplePass::new("flat", PassType::Graphics)
            .write(BACKBUFFER_NAME)
            .attachment(BACKBUFFER_NAME, AttachmentOps::load())
            .as_builder();
        builder.uses_depth = false;
        builder.depth_attachment =
            Some(crate::render_pass::DepthStencilAttachmentOps::reverse_z_default());
        let error = match FrameGraphBuilder::new()
            .add_pass(AnonPass(builder))
            .build::<MockBackend>()
        {
            Err(RenderGraphError::Validation(error)) => error,
            Err(other) => panic!("expected validation error, got {other}"),
            Ok(_) => panic!("expected validation error, graph compiled"),
        };
        assert!(matches!(
            error,
            GraphValidationError::DepthOpsWithoutDepthUse(ref pass) if pass == "flat"
        ));
    }

    struct AnonPass(super::super::builder::InternalPassBuilder);
    impl super::super::builder::PassBuilder for AnonPass {
        fn as_builder(self) -> super::super::builder::InternalPassBuilder {
            self.0
        }
    }

    #[test]
    fn depth_load_without_a_depth_producer_fails_validation() {
        let error = missing_ops_error(
            FrameGraphBuilder::new()
                .create_resource(validation_resource("color", 64, 64))
                .export_resource("color")
                .add_pass(
                    super::super::builder::SimplePass::new("lone", PassType::Graphics)
                        .write("color")
                        .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                        .depth_ops(
                            AttachmentOps::clear(ClearValue::DEFAULT_DEPTH)
                                .with_load(crate::render_pass::LoadOp::Load),
                            AttachmentOps::dont_care(),
                        ),
                ),
        );
        assert!(matches!(
            error,
            GraphValidationError::LoadingUndefinedAttachment { pass, .. } if pass == "lone"
        ));
    }

    #[test]
    fn graphics_depth_defaults_are_normalized_to_the_reverse_z_contract() {
        let graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .export_resource("color")
            .add_pass(
                super::super::builder::SimplePass::new("paint", PassType::Graphics)
                    .write("color")
                    .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
            )
            .build::<MockBackend>()
            .unwrap();
        let ops = graph
            .pass(graph.pass_index("paint").unwrap())
            .unwrap()
            .depth_attachment
            .expect("depth contract normalized");
        assert_eq!(ops.depth.load, crate::render_pass::LoadOp::Clear);
        assert_eq!(ops.depth.store, crate::render_pass::StoreOp::Store);
        assert_eq!(
            ops.depth.clear_value,
            ClearValue::DepthStencil {
                depth: 0.0,
                stencil: 0
            }
        );
        assert_eq!(ops.stencil.load, crate::render_pass::LoadOp::Clear);
        assert_eq!(ops.stencil.store, crate::render_pass::StoreOp::DontCare);
    }

    #[test]
    fn distinct_same_format_targets_keep_separate_declarations() {
        let graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("albedo", 64, 64))
            .create_resource(validation_resource("normals", 64, 64))
            .export_resource("albedo")
            .export_resource("normals")
            .add_pass(
                super::super::builder::SimplePass::new("mrt", PassType::Graphics)
                    .write("albedo")
                    .write("normals")
                    .attachment("albedo", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                    .attachment(
                        "normals",
                        AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK),
                    ),
            )
            .build::<MockBackend>()
            .unwrap();
        let pass = graph.pass(graph.pass_index("mrt").unwrap()).unwrap();
        assert_eq!(pass.color_attachments.len(), 2);
        assert_ne!(pass.color_attachments[0].0, pass.color_attachments[1].0);
        assert_ne!(pass.color_attachments[0].1, pass.color_attachments[1].1);
    }

    #[test]
    fn out_of_range_depth_clear_values_fail_validation() {
        let error = missing_ops_error(
            FrameGraphBuilder::new()
                .create_resource(validation_resource("color", 64, 64))
                .export_resource("color")
                .add_pass(AnonPass({
                    let mut builder =
                        super::super::builder::SimplePass::new("bad_depth", PassType::Graphics)
                            .write("color")
                            .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                            .as_builder();
                    builder.depth_attachment =
                        Some(crate::render_pass::DepthStencilAttachmentOps::clear(
                            ClearValue::DepthStencil {
                                depth: 1.5,
                                stencil: 0,
                            },
                        ));
                    builder
                })),
        );
        assert!(matches!(
            error,
            GraphValidationError::InvalidDepthClearValue { pass, depth }
                if pass == "bad_depth" && depth == 1.5
        ));
    }

    #[test]
    fn builder_rejects_empty_names() {
        assert_eq!(
            validation_error(
                FrameGraphBuilder::new().create_resource(validation_resource("", 1, 1))
            ),
            GraphValidationError::EmptyResourceName
        );
        assert_eq!(
            validation_error(FrameGraphBuilder::new().add_pass(
                super::super::builder::SimplePass::new("", PassType::Graphics)
            )),
            GraphValidationError::EmptyPassName
        );
    }

    #[test]
    fn builder_accepts_declared_resources_and_builtin_backbuffer() {
        let result = FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .add_pass(
                super::super::builder::SimplePass::new("geometry", PassType::Graphics)
                    .write("color")
                    .attachment(
                        "color",
                        crate::render_pass::AttachmentOps::clear(
                            crate::render_pass::ClearValue::OPAQUE_BLACK,
                        ),
                    ),
            )
            .add_pass(
                super::super::builder::SimplePass::new("present", PassType::Graphics)
                    .read("color")
                    .write(BACKBUFFER_NAME)
                    .attachment(BACKBUFFER_NAME, crate::render_pass::AttachmentOps::load()),
            )
            .build::<MockBackend>();
        assert!(result.is_ok());
    }

    #[test]
    fn builder_culls_unobserved_branches_but_keeps_the_backbuffer_chain() {
        let graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("dead", 64, 64))
            .add_pass(
                super::super::builder::SimplePass::new("dead_branch", PassType::Graphics)
                    .write("dead"),
            )
            .add_pass(
                super::super::builder::SimplePass::new("present", PassType::Graphics)
                    .write(BACKBUFFER_NAME)
                    .attachment(BACKBUFFER_NAME, crate::render_pass::AttachmentOps::load()),
            )
            .build::<MockBackend>()
            .unwrap();

        let dead = graph.pass_id("dead_branch").unwrap();
        let present = graph.pass_id("present").unwrap();
        assert_eq!(graph.is_pass_live(dead), Some(false));
        assert_eq!(graph.is_pass_live(present), Some(true));
        assert_eq!(graph.execution_order(), vec![present.0 as usize]);
    }

    #[test]
    fn submissions_to_culled_passes_fail_with_a_structured_error() {
        let graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("dead", 64, 64))
            .add_pass(
                super::super::builder::SimplePass::new("dead_branch", PassType::Graphics)
                    .write("dead"),
            )
            .build::<MockBackend>()
            .unwrap();
        let dead = graph.pass_id("dead_branch").unwrap();
        let mut backend = MockBackend::new();
        let mut frame = super::super::frame::Frame::new(&graph, &mut backend, 0, 0);
        frame.submit(
            dead,
            std::rc::Rc::new(crate::renderer::types::DrawList::new()),
        );

        assert!(matches!(
            frame.validate_submissions(),
            Err(RenderGraphError::SubmissionToCulledPass(name)) if name == "dead_branch"
        ));
    }

    #[test]
    fn explicit_offscreen_export_keeps_its_producer_chain() {
        let graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("intermediate", 64, 64))
            .create_resource(validation_resource("readback", 64, 64))
            .export_resource("readback")
            .add_pass(
                super::super::builder::SimplePass::new("produce", PassType::Graphics)
                    .write("intermediate")
                    .attachment(
                        "intermediate",
                        crate::render_pass::AttachmentOps::clear(
                            crate::render_pass::ClearValue::OPAQUE_BLACK,
                        ),
                    ),
            )
            .add_pass(
                super::super::builder::SimplePass::new("copy_for_readback", PassType::Graphics)
                    .read("intermediate")
                    .write("readback")
                    .attachment(
                        "readback",
                        crate::render_pass::AttachmentOps::clear(
                            crate::render_pass::ClearValue::OPAQUE_BLACK,
                        ),
                    ),
            )
            .build::<MockBackend>()
            .unwrap();

        assert_eq!(graph.execution_order(), vec![0, 1]);
        assert_eq!(
            graph.is_pass_live(graph.pass_id("produce").unwrap()),
            Some(true)
        );
        assert_eq!(
            graph.is_pass_live(graph.pass_id("copy_for_readback").unwrap()),
            Some(true)
        );
    }

    #[test]
    fn explicit_side_effect_keeps_data_producers_without_fake_outputs() {
        let graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("query_input", 64, 64))
            .add_pass(
                super::super::builder::SimplePass::new("produce_query_data", PassType::Graphics)
                    .write("query_input")
                    .attachment(
                        "query_input",
                        crate::render_pass::AttachmentOps::clear(
                            crate::render_pass::ClearValue::OPAQUE_BLACK,
                        ),
                    ),
            )
            .add_side_effect_pass(
                super::super::builder::SimplePass::new("timestamp_readback", PassType::Graphics)
                    .read("query_input"),
            )
            .build::<MockBackend>()
            .unwrap();

        assert_eq!(graph.execution_order(), vec![0, 1]);
        assert!(
            graph
                .pass(graph.pass_index("timestamp_readback").unwrap())
                .unwrap()
                .side_effect
        );
    }

    #[test]
    fn builder_rejects_undeclared_exports() {
        assert_eq!(
            validation_error(FrameGraphBuilder::new().export_resource("typo_output")),
            GraphValidationError::UndeclaredExportedResource("typo_output".to_string())
        );
    }

    #[test]
    fn transient_initialization_rejects_a_missing_namespace_entry() {
        let mut graph = TestGraph::new();
        graph
            .transient_resources
            .push(validation_resource("orphan", 1, 1));

        let error = graph
            .initialize_transient_textures(&MockBackend::new())
            .unwrap_err();
        assert!(matches!(
            error,
            RenderGraphError::Validation(
                GraphValidationError::MissingResourceNamespaceEntry(resource)
            ) if resource == "orphan"
        ));
    }

    #[test]
    fn test_culled_textures_have_no_native_allocation_in_either_debug_mode() {
        use super::super::builder::SimplePass;
        let mut graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("live", 16, 16))
            .create_resource(validation_resource("dead", 16, 16))
            .add_side_effect_pass(
                SimplePass::new("live pass", PassType::Graphics)
                    .without_depth()
                    .write("live")
                    .attachment("live", AttachmentOps::clear(ClearValue::Color([0.0; 4]))),
            )
            .add_pass(
                SimplePass::new("dead pass", PassType::Graphics)
                    .without_depth()
                    .write("dead")
                    .attachment("dead", AttachmentOps::clear(ClearValue::Color([0.0; 4]))),
            )
            .build::<MockBackend>()
            .unwrap();
        for optimize in [true, false] {
            graph.cleanup();
            graph.set_transient_aliasing(optimize).unwrap();
            let backend = MockBackend::new();
            graph.initialize_transient_textures(&backend).unwrap();
            assert_eq!(*backend.slot_member_counts.borrow(), vec![1, 1]);
            assert!(graph.transient_texture("dead", 0).is_none());
        }
    }

    #[test]
    fn test_alias_handoffs_are_cached_for_first_use_and_debug_mode_rejects_live_change() {
        use super::super::builder::SimplePass;
        let mut graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("early", 16, 16))
            .create_resource(validation_resource("late", 16, 16))
            .add_side_effect_pass(
                SimplePass::new("early pass", PassType::Graphics)
                    .without_depth()
                    .write("early")
                    .attachment("early", AttachmentOps::clear(ClearValue::Color([0.0; 4]))),
            )
            .add_side_effect_pass(
                SimplePass::new("late pass", PassType::Graphics)
                    .without_depth()
                    .write("late")
                    .attachment("late", AttachmentOps::clear(ClearValue::Color([0.0; 4]))),
            )
            .build::<MockBackend>()
            .unwrap();
        assert!(graph.texture_alias_handoff_before(0));
        assert!(graph.texture_alias_handoff_before(1));
        graph
            .initialize_transient_textures(&MockBackend::new())
            .unwrap();
        assert!(graph.set_transient_aliasing(false).is_err());
        assert!(graph.texture_alias_handoff_before(0));
        graph.cleanup();
        graph.set_transient_aliasing(false).unwrap();
        graph.compile().unwrap();
        assert!(!graph.texture_alias_handoff_before(0));
        let diagnostics = graph.diagnostics().unwrap();
        assert_eq!(diagnostics.summary.physical_transient_allocations, 2);
        assert_eq!(diagnostics.summary.transient_alias_savings_bytes, 0);
    }

    #[test]
    fn test_memoryless_policy_requires_tile_local_discard_and_owns_frame_slot() {
        use super::super::builder::SimplePass;
        use crate::render_pass::StoreOp;
        let mut graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("tile", 16, 16))
            .add_side_effect_pass(
                SimplePass::new("tile pass", PassType::Graphics)
                    .without_depth()
                    .write("tile")
                    .attachment(
                        "tile",
                        AttachmentOps::clear(ClearValue::Color([0.0; 4]))
                            .with_store(StoreOp::DontCare),
                    ),
            )
            .build::<MockBackend>()
            .unwrap();
        let backend = MockBackend::new();
        graph.initialize_transient_textures(&backend).unwrap();
        let policies = backend.policies.borrow();
        assert!(policies.iter().all(|policy| policy.memoryless));
        assert_eq!(
            policies
                .iter()
                .map(|policy| policy.frame_slot)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        drop(policies);
        graph.cleanup();
        graph.set_transient_aliasing(false).unwrap();
        let backend = MockBackend::new();
        graph.initialize_transient_textures(&backend).unwrap();
        assert!(
            backend
                .policies
                .borrow()
                .iter()
                .all(|policy| !policy.memoryless && !policy.optimize)
        );
    }

    #[test]
    fn test_store_action_prevents_memoryless_even_for_one_pass() {
        use super::super::builder::SimplePass;
        let mut graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("stored", 16, 16))
            .add_side_effect_pass(
                SimplePass::new("store pass", PassType::Graphics)
                    .without_depth()
                    .write("stored")
                    .attachment("stored", AttachmentOps::clear(ClearValue::Color([0.0; 4]))),
            )
            .build::<MockBackend>()
            .unwrap();
        let backend = MockBackend::new();
        graph.initialize_transient_textures(&backend).unwrap();
        assert!(
            backend
                .policies
                .borrow()
                .iter()
                .all(|policy| !policy.memoryless)
        );
    }

    #[test]
    fn transient_aliasing_groups_non_overlapping_compatible_transients() {
        let mut graph = TestGraph::new();
        graph.create_resource_id("early");
        graph.create_resource_id("late");
        graph.transient_resources = vec![
            validation_resource("early", 64, 64),
            validation_resource("late", 64, 64),
        ];
        graph.add_pass(PassDesc::new(
            "first",
            PassType::Graphics,
            vec![],
            vec![ResourceId(0)],
        ));
        graph.add_pass(PassDesc::new(
            "second",
            PassType::Graphics,
            vec![],
            vec![ResourceId(1)],
        ));

        let backend = MockBackend::new();
        graph.initialize_transient_textures(&backend).unwrap();

        // One two-member slot per frame in flight.
        assert_eq!(*backend.slot_member_counts.borrow(), vec![2, 2]);
    }

    #[test]
    fn test_allocation_contract_rejects_new_overlap_until_cleanup() {
        let mut graph = TestGraph::new();
        let early = graph.create_resource_id("early");
        let late = graph.create_resource_id("late");
        graph.transient_resources = vec![
            validation_resource("early", 64, 64),
            validation_resource("late", 64, 64),
        ];
        graph.add_pass(PassDesc::new(
            "early write",
            PassType::Graphics,
            vec![],
            vec![early],
        ));
        graph.add_pass(PassDesc::new(
            "late write",
            PassType::Graphics,
            vec![],
            vec![late],
        ));
        let backend = MockBackend::new();
        graph.initialize_transient_textures(&backend).unwrap();
        assert_eq!(*backend.slot_member_counts.borrow(), vec![2, 2]);
        graph.add_pass(PassDesc::new(
            "later early read",
            PassType::Graphics,
            vec![early],
            vec![],
        ));
        assert!(matches!(
            graph.initialize_transient_textures(&backend),
            Err(RenderGraphError::AllocationContractChanged)
        ));
        assert_eq!(*backend.slot_member_counts.borrow(), vec![2, 2]);
        assert!(graph.transient_texture("early", 0).is_some());
        graph.cleanup();
        graph.initialize_transient_textures(&backend).unwrap();
        assert_eq!(*backend.slot_member_counts.borrow(), vec![2, 2, 1, 1, 1, 1]);
    }

    #[test]
    fn test_allocation_contract_rejects_sampling_old_memoryless_storage() {
        use super::super::builder::SimplePass;
        use crate::render_pass::StoreOp;
        let mut graph = FrameGraphBuilder::new()
            .create_resource(validation_resource("tile", 16, 16))
            .add_side_effect_pass(
                SimplePass::new("tile pass", PassType::Graphics)
                    .without_depth()
                    .write("tile")
                    .attachment(
                        "tile",
                        AttachmentOps {
                            load: LoadOp::Clear,
                            store: StoreOp::DontCare,
                            clear_value: ClearValue::Color([0.0; 4]),
                        },
                    ),
            )
            .build::<MockBackend>()
            .unwrap();
        let backend = MockBackend::new();
        graph.initialize_transient_textures(&backend).unwrap();
        assert!(
            backend
                .policies
                .borrow()
                .iter()
                .all(|policy| policy.memoryless)
        );
        let tile = graph.resource_id("tile").unwrap();
        let mut sample = PassDesc::new("sample tile", PassType::Compute, vec![tile], vec![]);
        sample.side_effect = true;
        graph.add_pass(sample);
        assert!(matches!(
            graph.initialize_transient_textures(&backend),
            Err(RenderGraphError::AllocationContractChanged)
        ));
        graph.cleanup();
        graph.initialize_transient_textures(&backend).unwrap();
        assert!(
            backend
                .policies
                .borrow()
                .iter()
                .skip(2)
                .all(|policy| !policy.memoryless)
        );
    }

    #[test]
    fn test_allocation_contract_accepts_changed_disjoint_intervals() {
        let mut graph = TestGraph::new();
        let early = graph.create_resource_id("early");
        let late = graph.create_resource_id("late");
        graph.transient_resources = vec![
            validation_resource("early", 64, 64),
            validation_resource("late", 64, 64),
        ];
        graph.add_pass(PassDesc::new(
            "early write",
            PassType::Graphics,
            vec![],
            vec![early],
        ));
        graph.add_pass(PassDesc::new(
            "late write",
            PassType::Graphics,
            vec![],
            vec![late],
        ));
        let backend = MockBackend::new();
        graph.initialize_transient_textures(&backend).unwrap();
        graph.insert_pass(
            1,
            PassDesc::new("early read", PassType::Graphics, vec![early], vec![]),
        );
        graph.initialize_transient_textures(&backend).unwrap();
        assert_eq!(*backend.slot_member_counts.borrow(), vec![2, 2]);
    }

    #[test]
    fn transient_aliasing_keeps_overlapping_transients_separate() {
        let mut graph = TestGraph::new();
        graph.create_resource_id("a");
        graph.create_resource_id("b");
        graph.transient_resources = vec![
            validation_resource("a", 64, 64),
            validation_resource("b", 64, 64),
        ];
        graph.add_pass(PassDesc::new(
            "write_a",
            PassType::Graphics,
            vec![],
            vec![ResourceId(0)],
        ));
        graph.add_pass(PassDesc::new(
            "write_b",
            PassType::Graphics,
            vec![],
            vec![ResourceId(1)],
        ));
        graph.add_pass(PassDesc::new(
            "read_a",
            PassType::Graphics,
            vec![ResourceId(0)],
            vec![],
        ));

        let backend = MockBackend::new();
        graph.initialize_transient_textures(&backend).unwrap();

        // `a` is live across `b`'s write, so the two never share a slot.
        assert_eq!(*backend.slot_member_counts.borrow(), vec![1, 1, 1, 1]);
    }

    #[test]
    fn transient_aliasing_disabled_creates_standalone_textures() {
        let mut graph = TestGraph::new();
        graph.create_resource_id("early");
        graph.create_resource_id("late");
        graph.transient_resources = vec![
            validation_resource("early", 64, 64),
            validation_resource("late", 64, 64),
        ];
        graph.add_pass(PassDesc::new(
            "first",
            PassType::Graphics,
            vec![],
            vec![ResourceId(0)],
        ));
        graph.add_pass(PassDesc::new(
            "second",
            PassType::Graphics,
            vec![],
            vec![ResourceId(1)],
        ));
        graph.set_transient_aliasing(false).unwrap();

        let backend = MockBackend::new();
        graph.initialize_transient_textures(&backend).unwrap();

        assert_eq!(*backend.slot_member_counts.borrow(), vec![1, 1, 1, 1]);
    }

    // --- Typed template declarations keep cross-aspect hazards ordered (#30) ---

    #[test]
    fn sampling_a_depth_atlas_orders_after_the_shadow_pass_and_keeps_it_live() {
        use super::super::passes::{FullscreenPass, GeometryPass, ShadowPass};
        use crate::GraphResourceType;
        use crate::ImageFormat;
        use crate::handle::PipelineHandle;

        // Editor-graph shape: sky produces hdr_color, shadow clears the depth
        // atlas, geometry loads hdr_color and samples the atlas, tonemap
        // presents through the exported backbuffer. The sampled read covers
        // every aspect of the atlas (the graph cannot narrow sampling to the
        // depth aspect without the image format), so the shadow write and the
        // geometry read must stay RAW-ordered and liveness must keep the
        // shadow pass.
        let graph = FrameGraphBuilder::new()
            .create_resource(GraphResourceDesc {
                name: "hdr_color".to_string(),
                resource_type: GraphResourceType::ColorAttachment {
                    clear_value: Some([0.0; 4]),
                },
                format: ImageFormat::R16G16B16A16Sfloat,
                width: 64,
                height: 64,
                tracks_swapchain_size: false,
            })
            .create_resource(GraphResourceDesc {
                name: "shadow_atlas".to_string(),
                resource_type: GraphResourceType::DepthAttachment {
                    clear_value: 1.0,
                    sampled: true,
                },
                format: ImageFormat::D32Sfloat,
                width: 64,
                height: 64,
                tracks_swapchain_size: false,
            })
            .add_pass(
                FullscreenPass::new("sky")
                    .write("hdr_color", ImageFormat::R16G16B16A16Sfloat)
                    .pipeline(PipelineHandle::from_raw(0, 0)),
            )
            .add_pass(ShadowPass::new("shadow").write_depth("shadow_atlas", ImageFormat::D32Sfloat))
            .add_pass(
                GeometryPass::new("geometry")
                    .write_color_ops(
                        "hdr_color",
                        ImageFormat::R16G16B16A16Sfloat,
                        AttachmentOps::load(),
                    )
                    .read("shadow_atlas"),
            )
            .add_pass(
                FullscreenPass::new("tonemap")
                    .read("hdr_color")
                    .write_backbuffer()
                    .pipeline(PipelineHandle::from_raw(0, 0)),
            )
            .build::<MockBackend>()
            .unwrap();

        let plan = graph.build_execution_plan().unwrap();
        assert!(
            plan.sorted_passes.contains(&1),
            "shadow pass must stay live: geometry samples its atlas"
        );
        assert!(
            plan.dag[2].predecessors.contains(&1),
            "geometry must depend on the shadow pass it samples from"
        );
    }

    // --- Imported-image state contracts (#30) ---

    #[test]
    fn test_unused_imported_image_compiles_its_final_transition() {
        let graph = FrameGraphBuilder::new()
            .import_resource(
                "external",
                TextureHandle::from_raw(7, 0),
                ImportedImageContract::arrives_in(ResourceState::ShaderRead)
                    .must_end_in(ResourceState::TransferSrc),
            )
            .build::<MockBackend>()
            .unwrap();
        let ops = graph.final_image_sync_ops();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].before_pass, None);
        assert_eq!(ops[0].reason, super::super::SyncReason::ImportedFinal);
    }

    #[test]
    fn imported_final_state_is_reachable_when_a_live_pass_accesses_the_image() {
        FrameGraphBuilder::new()
            .import_resource(
                "external",
                TextureHandle::from_raw(7, 0),
                ImportedImageContract::arrives_in(ResourceState::ShaderRead)
                    .must_end_in(ResourceState::TransferSrc),
            )
            .add_side_effect_pass(
                super::super::builder::SimplePass::new("readback", PassType::Compute)
                    .read("external"),
            )
            .build::<MockBackend>()
            .unwrap();
    }

    #[test]
    fn backbuffer_loads_rely_on_the_default_contract_and_undefining_it_fails() {
        // The default backbuffer contract declares observable contents, so a
        // UI-only graph may load the backbuffer without an in-graph producer.
        FrameGraphBuilder::new()
            .add_pass(
                super::super::builder::SimplePass::new("overlay", PassType::Graphics)
                    .write(BACKBUFFER_NAME)
                    .attachment(BACKBUFFER_NAME, AttachmentOps::load()),
            )
            .build::<MockBackend>()
            .unwrap();

        // Overriding the contract to Undefined removes that guarantee.
        let error = validation_error(
            FrameGraphBuilder::new()
                .backbuffer_contract(ImportedImageContract::undefined())
                .add_pass(
                    super::super::builder::SimplePass::new("overlay", PassType::Graphics)
                        .write(BACKBUFFER_NAME)
                        .attachment(BACKBUFFER_NAME, AttachmentOps::load()),
                ),
        );
        assert!(matches!(
            error,
            GraphValidationError::LoadingUninitializedImport { pass, resource: 0, aspects }
                if pass == "overlay" && aspects == super::super::ImageAspects::COLOR
        ));
    }
    #[test]
    fn test_duplicate_native_import_identities_are_rejected_before_scheduling() {
        let texture = TextureHandle::from_raw(8, 2);
        let error = validation_error(
            FrameGraphBuilder::new()
                .import_resource("first", texture, ImportedImageContract::undefined())
                .import_resource("second", texture, ImportedImageContract::undefined()),
        );
        assert!(matches!(
            error,
            GraphValidationError::DuplicateImportedIdentity { kind: "image", .. }
        ));
        let buffer = BufferHandle::from_raw(8, 2);
        let desc = BufferDesc::new(64, BufferUsages::STORAGE, BufferMemoryPolicy::DeviceLocal);
        let error = validation_error(
            FrameGraphBuilder::new()
                .import_buffer("first", buffer, desc)
                .import_buffer("second", buffer, desc),
        );
        assert!(matches!(
            error,
            GraphValidationError::DuplicateImportedIdentity { kind: "buffer", .. }
        ));
    }
}
