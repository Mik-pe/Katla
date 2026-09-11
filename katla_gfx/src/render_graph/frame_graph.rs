use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::backend::RenderGraphBackend;
use super::builder::{InternalPassBuilder, PassBuilder};
use super::compiler::{ExecutionPlan, GraphCompiler};
use super::error::{GraphValidationError, RenderGraphError};
use super::handles::{PassId, ResourceId};
use super::pass::{PassDesc, PassType};
use super::resource::{
    GraphResourceDesc, GraphResourceHandle, ImportedImageContract, ResourceState,
};
use crate::render_pass::{ClearValue, DepthStencilAttachmentOps, LoadOp};

const BACKBUFFER_NAME: &str = super::BACKBUFFER_NAME;

/// Default state contract for the built-in backbuffer: contents from before
/// the graph (the previously presented frame) are observable, so a pass may
/// load them without an in-graph producer. Applications that present the
/// backbuffer override this with `FrameGraphBuilder::backbuffer_contract` to
/// also require the final `PresentSrc` state.
const DEFAULT_BACKBUFFER_CONTRACT: ImportedImageContract =
    ImportedImageContract::arrives_in(ResourceState::ColorAttachment);

#[derive(Debug, Clone, Default)]
pub(super) struct PassBarrierCache {
    pub(super) pre_write_resources: Vec<ResourceId>,
    pub(super) pre_read_resources: Vec<ResourceId>,
    pub(super) post_write_to_read_resources: Vec<ResourceId>,
    pub(super) needs_depth_sync: bool,
}

/// Per-frame parameters for render graph execution.
///
/// These values change every frame and are set before calling `execute()`.
/// Logically separate from the graph structure which is "built once, executed many times."
pub(super) struct FrameParams {
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

    /// Whether pass liveness analysis is enabled for this graph.
    pub(crate) pass_culling_enabled: bool,

    /// Compiled execution plan (sorted live passes and dependency metadata).
    execution_plan: Option<ExecutionPlan>,

    /// Whether the graph has been compiled.
    compiled: bool,

    /// Whether the barrier cache needs recomputation.
    barriers_dirty: bool,

    /// Cached barrier info per pass (indexed by pass index).
    barrier_cache: Vec<PassBarrierCache>,

    /// Transient resource descriptors (for lazy GPU resource creation).
    pub(super) transient_resources: Vec<GraphResourceDesc>,

    /// Created transient textures (frame_idx -> ResourceId -> texture).
    /// Per-frame transient textures. One set per frame-in-flight to prevent
    /// race conditions where frame N+1 modifies layout tracking while frame N is still executing.
    pub(super) transient_textures: Vec<HashMap<ResourceId, B::TransientTexture>>,

    /// Base bindless index for LDR texture (actual index = base + frame_idx).
    ldr_texture_base_index: Option<u32>,

    /// Per-frame parameters set before each `execute()` call.
    /// These are logically separate from the graph structure itself,
    /// which is "built once, executed many times."
    pub(super) params: FrameParams,

    /// Per-frame compositing descriptor sets (one per frame in flight).
    /// Pre-allocated and reused each frame via update_textures().
    pub(super) compositing_descriptor_sets:
        RefCell<[Option<crate::render_graph::descriptor_sets::CompositingDescriptorSet>; 2]>,
}

// --- Backend-agnostic methods ---
impl<B: RenderGraphBackend> FrameGraph<B> {
    /// Create a new empty frame graph.
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            resources: Vec::new(),
            resource_by_name: HashMap::new(),
            pass_names: HashMap::new(),
            exported_resources: BTreeSet::new(),
            imported_contracts: BTreeMap::new(),
            pass_culling_enabled: false,
            execution_plan: None,
            compiled: false,
            barriers_dirty: true,
            barrier_cache: Vec::new(),
            transient_resources: Vec::new(),
            transient_textures: Vec::new(),
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
        self.barriers_dirty = true;
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
        self.barriers_dirty = true;
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
        self.barriers_dirty = true;
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
            self.barriers_dirty = true;
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
        self.barriers_dirty = true;
    }

    /// Get the name of a resource by its ResourceId.
    pub fn resource_name(&self, id: ResourceId) -> Option<&str> {
        self.resources.get(id.0 as usize).map(|r| r.name.as_str())
    }

    /// Build the canonical execution plan without mutating frame state.
    pub(crate) fn build_execution_plan(&self) -> Result<ExecutionPlan, RenderGraphError> {
        if self.pass_culling_enabled {
            GraphCompiler::from_pass_descs_with_exports(
                &self.passes,
                self.exported_resources.iter().copied(),
            )
            .compile()
        } else {
            GraphCompiler::from_pass_descs(&self.passes).compile()
        }
    }

    /// Compile the graph for execution.
    pub(crate) fn compile(&mut self) -> Result<(), RenderGraphError> {
        if self.compiled {
            return Ok(());
        }

        let plan = self.build_execution_plan()?;
        self.validate_attachment_ops(&plan)?;
        self.validate_imported_state_contracts(&plan)?;
        self.execution_plan = Some(plan);
        self.compiled = true;
        Ok(())
    }

    /// Validate imported-image state contracts against the compiled plan.
    ///
    /// A required final state is reachable when a live pass accesses the image
    /// (the backend can always transition after the last access). An image no
    /// live pass touches never leaves its declared initial state, so a
    /// required final state differing from it is a structural error.
    fn validate_imported_state_contracts(
        &self,
        plan: &ExecutionPlan,
    ) -> Result<(), RenderGraphError> {
        for (&resource, contract) in &self.imported_contracts {
            let Some(required) = contract.required_final else {
                continue;
            };
            if contract.initial != required
                && !plan.resource_lifetimes.contains_key(&resource)
                && let Some(name) = self.resource_name(resource)
            {
                return Err(GraphValidationError::UnreachableImportedFinalState {
                    resource: name.to_string(),
                    required,
                }
                .into());
            }
        }
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
                // Contents may only be loaded when they are observable: an
                // earlier live pass produced them, or the image is imported
                // with a non-Undefined initial state.
                if ops.load == LoadOp::Load && !produced.contains(resource) {
                    let imported_contents_observable = self
                        .imported_contracts
                        .get(resource)
                        .is_some_and(|contract| contract.initial != ResourceState::Undefined);
                    if !imported_contents_observable {
                        let error = if self.imported_contracts.contains_key(resource) {
                            GraphValidationError::LoadingUndefinedImportedContents {
                                pass: pass.name.clone(),
                                resource: name,
                            }
                        } else {
                            GraphValidationError::LoadingUndefinedAttachment {
                                pass: pass.name.clone(),
                                resource: name,
                            }
                        };
                        return Err(error.into());
                    }
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
                    let depth_transient =
                        pass.writes.iter().copied().find(|&id| {
                            self.write_target_role(id) == WriteTargetRole::TransientDepth
                        });
                    let has_producer = match depth_transient {
                        Some(id) => produced.contains(&id),
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

    /// Ensure the barrier cache is up to date.
    ///
    /// Recomputes cached per-pass barrier info when the graph structure has changed.
    /// This avoids re-scanning the execution order and pass dependencies every frame.
    pub(crate) fn ensure_barrier_cache(&mut self) {
        if !self.barriers_dirty {
            return;
        }
        let Some(plan) = &self.execution_plan else {
            return;
        };

        let mut cache = vec![PassBarrierCache::default(); self.passes.len()];
        let mut depth_written = false;

        for &pass_idx in &plan.sorted_passes {
            let pass = &self.passes[pass_idx];
            let mut pre_writes = Vec::new();
            let mut pre_reads = Vec::new();
            let mut post_writes = Vec::new();

            let needs_depth_sync = pass.uses_depth && depth_written;

            for &write_id in &pass.writes {
                if self.resource_name(write_id) == Some(BACKBUFFER_NAME) {
                    continue;
                }
                pre_writes.push(write_id);
            }

            for &read_id in &pass.reads {
                if self.resource_name(read_id) == Some(BACKBUFFER_NAME) {
                    continue;
                }
                if pass.writes.contains(&read_id) {
                    continue;
                }
                pre_reads.push(read_id);
            }

            let current_pos = plan.sorted_passes.iter().position(|&p| p == pass_idx);
            if let Some(pos) = current_pos {
                for &write_id in &pass.writes {
                    if self.resource_name(write_id) == Some(BACKBUFFER_NAME) {
                        continue;
                    }
                    let next_access = plan.sorted_passes[pos + 1..].iter().find(|&&idx| {
                        let p = &self.passes[idx];
                        p.reads.contains(&write_id) || p.writes.contains(&write_id)
                    });
                    let next_is_read = match next_access {
                        Some(&idx) => {
                            let p = &self.passes[idx];
                            p.reads.contains(&write_id) && !p.writes.contains(&write_id)
                        }
                        None => true,
                    };
                    if next_is_read {
                        post_writes.push(write_id);
                    }
                }
            }

            if pass.uses_depth {
                depth_written = true;
            }

            cache[pass_idx] = PassBarrierCache {
                pre_write_resources: pre_writes,
                pre_read_resources: pre_reads,
                post_write_to_read_resources: post_writes,
                needs_depth_sync,
            };
        }

        self.barrier_cache = cache;
        self.barriers_dirty = false;
    }

    /// Get cached barrier info for a pass.
    pub(super) fn barrier_cache(&self, pass_index: usize) -> Option<&PassBarrierCache> {
        self.barrier_cache.get(pass_index)
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
        self.params.skeleton_copy_commands = commands;
    }

    /// Cleanup and destroy all transient textures.
    pub fn cleanup(&mut self) {
        log::info!(
            "Cleaning up frame graph transient textures ({} frames)",
            self.transient_textures.len()
        );
        let total_textures: usize = self.transient_textures.iter().map(|m| m.len()).sum();
        log::info!("  Total textures to clean up: {}", total_textures);
        self.transient_textures.clear();
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
    #[cfg(target_os = "macos")]
    pub(crate) fn resource_format(&self, id: ResourceId) -> Option<crate::texture::ImageFormat> {
        let name = self.resource_name(id)?;
        self.transient_resources
            .iter()
            .find(|desc| desc.name == name)
            .map(|desc| desc.format)
    }

    /// Get the execution order for passes.
    pub(crate) fn execution_order(&self) -> Vec<usize> {
        self.execution_plan
            .as_ref()
            .map(|plan| plan.sorted_passes.clone())
            .unwrap_or_else(|| (0..self.passes.len()).collect())
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

    /// Initialize transient textures using the backend.
    ///
    /// Creates per-frame sets of textures — one per frame-in-flight.
    pub fn initialize_transient_textures(&mut self, backend: &B) -> Result<(), RenderGraphError> {
        if !self.transient_textures.is_empty() {
            return Ok(());
        }

        let frames = B::transient_texture_frames();

        log::info!(
            "Initializing {} transient textures ({} frames in flight)",
            self.transient_resources.len(),
            frames
        );

        for _frame_idx in 0..frames {
            let mut frame_textures = HashMap::new();

            for desc in &self.transient_resources {
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

                let texture = B::create_transient_texture(backend, desc)?;
                frame_textures.insert(resource_id, texture);
            }

            self.transient_textures.push(frame_textures);
        }

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
        if !self.compiled {
            self.compile()?;
        }

        self.initialize_transient_textures(renderer)?;

        let frame_idx = renderer.frame_index();
        let mut frame = super::frame::Frame::new(self, renderer, 0, frame_idx);
        f(&mut frame);
        frame.validate_submissions()?;

        Ok(std::mem::take(&mut frame.pending))
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
                    .unwrap_or(crate::texture::ImageFormat::B8G8R8A8Srgb);

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
        if !self.compiled {
            self.compile()?;
        }

        self.ensure_barrier_cache();

        self.initialize_transient_textures(renderer)?;

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
        f(&mut frame);
        frame.validate_submissions()?;
        frame.pre_compile_materials()?;
        frame.execute_passes()?;

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
    handle: GraphResourceHandle,
    contract: ImportedImageContract,
}

/// Builder for constructing a frame graph.
///
/// Created by [`VulkanRenderer::create_frame_graph()`].
/// Provides a fluent API for adding passes before building the executable [`FrameGraph`].
pub struct FrameGraphBuilder {
    pass_builders: Vec<InternalPassBuilder>,
    resources: Vec<ImportedResource>,
    transient_resources: Vec<GraphResourceDesc>,
    exported_resources: BTreeSet<String>,
    backbuffer_contract: ImportedImageContract,
}

impl FrameGraphBuilder {
    /// Create a new frame graph builder.
    pub fn new() -> Self {
        Self {
            pass_builders: Vec::new(),
            resources: Vec::new(),
            transient_resources: Vec::new(),
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
        handle: GraphResourceHandle,
        contract: ImportedImageContract,
    ) -> Self {
        self.resources.push(ImportedResource {
            name: name.into(),
            handle,
            contract,
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

        Ok(())
    }

    /// Build the frame graph after validating its complete resource namespace.
    pub fn build<B: RenderGraphBackend>(self) -> Result<FrameGraph<B>, RenderGraphError> {
        self.validate()?;

        let FrameGraphBuilder {
            pass_builders,
            resources,
            transient_resources,
            exported_resources,
            backbuffer_contract,
        } = self;

        let transient_names = transient_resources
            .iter()
            .map(|desc| desc.name.clone())
            .collect::<Vec<_>>();

        let mut graph = FrameGraph::new();
        graph.transient_resources = transient_resources;

        // The swapchain backbuffer is the only built-in resource. Every other
        // name has already been declared or imported by the validated builder.
        let backbuffer_id = graph.create_resource_id(BACKBUFFER_NAME);
        graph
            .imported_contracts
            .insert(backbuffer_id, backbuffer_contract);
        for name in transient_names {
            graph.create_resource_id(name);
        }
        for resource in &resources {
            let id = graph.create_resource_id(resource.name.clone());
            graph.imported_contracts.insert(id, resource.contract);
        }

        let mut global_resource_map = HashMap::new();
        for (name, &resource_id) in &graph.resource_by_name {
            global_resource_map.insert(name.clone(), GraphResourceHandle::new(resource_id.0));
        }
        for resource in &resources {
            global_resource_map.insert(resource.name.clone(), resource.handle);
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

            let mut pass = PassDesc::new(
                pass_builder.name,
                pass_builder.pass_type,
                read_ids,
                write_ids,
            );

            if has_explicit_image_accesses {
                pass.set_image_accesses(explicit_image_accesses);
            }

            pass.pipeline = pass_builder.pipeline;
            pass.tonemap_params = pass_builder.tonemap_params;
            pass.overlay_params = pass_builder.overlay_params;
            pass.material = pass_builder.material;
            pass.output_format = pass_builder.output_format;
            pass.uses_depth = pass_builder.uses_depth;
            pass.depth_attachment = pass_builder.depth_attachment;
            pass.kind = pass_builder.kind;
            pass.side_effect = pass_builder.side_effect;

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
    use crate::render_graph::resource::TransientTextureOps;

    fn rid(n: u32) -> ResourceId {
        ResourceId(n)
    }

    /// A trivial mock backend for testing FrameGraph without a GPU.
    struct MockBackend;

    struct MockTexture {
        state: std::cell::Cell<ResourceState>,
        slot: std::cell::Cell<Option<u32>>,
    }

    impl TransientTextureOps for MockTexture {
        fn state(&self) -> ResourceState {
            self.state.get()
        }
        fn set_state(&self, state: ResourceState) {
            self.state.set(state);
        }
    }

    #[derive(Clone)]
    struct MockImageView;

    unsafe impl Send for MockImageView {}
    unsafe impl Sync for MockImageView {}

    impl RenderGraphBackend for MockBackend {
        type TransientTexture = MockTexture;
        type ImageView = MockImageView;

        fn create_transient_texture(
            &self,
            _desc: &super::super::resource::GraphResourceDesc,
        ) -> Result<Self::TransientTexture, RenderGraphError> {
            Ok(MockTexture {
                state: std::cell::Cell::new(ResourceState::Undefined),
                slot: std::cell::Cell::new(None),
            })
        }

        fn destroy_transient_texture(_texture: Self::TransientTexture) {}

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
            GraphResourceHandle::new(42),
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
                    GraphResourceHandle::new(7),
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
                    GraphResourceHandle::new(1),
                    ImportedImageContract::undefined(),
                )
                .import_resource(
                    "external",
                    GraphResourceHandle::new(2),
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
                GraphResourceHandle::NONE,
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
        let mut backend = MockBackend;
        let mut frame = super::super::frame::Frame::new(&graph, &mut backend, 0, 0);
        frame.submit(dead, &crate::renderer::types::DrawList::new());

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
            .initialize_transient_textures(&MockBackend)
            .unwrap_err();
        assert!(matches!(
            error,
            RenderGraphError::Validation(
                GraphValidationError::MissingResourceNamespaceEntry(resource)
            ) if resource == "orphan"
        ));
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
    fn imported_contracts_reject_unreachable_final_states() {
        let error = validation_error(
            FrameGraphBuilder::new().import_resource(
                "external",
                GraphResourceHandle::new(7),
                ImportedImageContract::arrives_in(ResourceState::ShaderRead)
                    .must_end_in(ResourceState::TransferSrc),
            ),
        );
        assert!(matches!(
            error,
            GraphValidationError::UnreachableImportedFinalState { resource, required }
                if resource == "external" && required == ResourceState::TransferSrc
        ));
    }

    #[test]
    fn imported_final_state_is_reachable_when_a_live_pass_accesses_the_image() {
        FrameGraphBuilder::new()
            .import_resource(
                "external",
                GraphResourceHandle::new(7),
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
            GraphValidationError::LoadingUndefinedImportedContents { pass, resource }
                if pass == "overlay" && resource == BACKBUFFER_NAME
        ));
    }
}
