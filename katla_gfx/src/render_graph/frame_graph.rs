use std::cell::RefCell;
use std::collections::HashMap;

use super::backend::RenderGraphBackend;
use super::builder::{InternalPassBuilder, PassBuilder};
use super::compiler::{ExecutionPlan, GraphCompiler};
use super::error::RenderGraphError;
use super::handles::{PassId, ResourceId};
use super::pass::PassDesc;
use super::passes::geometry::GeometryPassData;
use super::resource::{GraphResourceDesc, GraphResourceHandle};

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
    pub skeleton_copy_commands: Vec<(u32, u32, u32)>,
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

    /// Compiled execution plan (sorted passes, barriers).
    execution_plan: Option<ExecutionPlan>,

    /// Whether the graph has been compiled.
    compiled: bool,

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
    pub(crate) fn new() -> Self {
        Self {
            passes: Vec::new(),
            resources: Vec::new(),
            resource_by_name: HashMap::new(),
            pass_names: HashMap::new(),
            execution_plan: None,
            compiled: false,
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

    /// Get the name of a resource by its ResourceId.
    pub fn resource_name(&self, id: ResourceId) -> Option<&str> {
        self.resources.get(id.0 as usize).map(|r| r.name.as_str())
    }

    /// Compile the graph for execution.
    pub(crate) fn compile(&mut self) -> Result<(), RenderGraphError> {
        if self.compiled {
            return Ok(());
        }

        let compiler = GraphCompiler::from_pass_descs(&self.passes);
        let execution_plan = compiler.compile()?;

        self.execution_plan = Some(execution_plan);
        self.compiled = true;
        Ok(())
    }

    /// Get a pass index by name.
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
    pub fn set_skeleton_copy_commands(&mut self, commands: Vec<(u32, u32, u32)>) {
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
                let resource_id = self
                    .resource_by_name
                    .get(&desc.name)
                    .copied()
                    .unwrap_or(ResourceId(frame_textures.len() as u32));

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
        for pass in &self.passes {
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

        self.initialize_transient_textures(renderer)?;

        let frame_idx = renderer.current_frame();

        log::trace!(
            "Frame graph execute: frame_idx={}, image_index={}",
            frame_idx,
            image_index
        );

        for pass in &self.passes {
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

/// Builder for constructing a frame graph.
///
/// Created by [`VulkanRenderer::create_frame_graph()`].
/// Provides a fluent API for adding passes before building the executable [`FrameGraph`].
pub struct FrameGraphBuilder {
    pass_builders: Vec<InternalPassBuilder>,
    resources: HashMap<String, GraphResourceHandle>,
    transient_resources: Vec<GraphResourceDesc>,
}

impl FrameGraphBuilder {
    /// Create a new frame graph builder.
    pub fn new() -> Self {
        Self {
            pass_builders: Vec::new(),
            resources: HashMap::new(),
            transient_resources: Vec::new(),
        }
    }

    /// Add a pass to the graph.
    pub fn add_pass(mut self, pass: impl PassBuilder + 'static) -> Self {
        self.pass_builders.push(pass.as_builder());
        self
    }

    /// Import an external resource into the graph.
    pub fn import_resource(mut self, name: impl Into<String>, handle: GraphResourceHandle) -> Self {
        self.resources.insert(name.into(), handle);
        self
    }

    /// Create a transient resource in the frame graph.
    pub fn create_resource(mut self, desc: GraphResourceDesc) -> Self {
        self.transient_resources.push(desc);
        self
    }

    /// Build the frame graph.
    pub fn build<B: RenderGraphBackend>(self) -> Result<FrameGraph<B>, RenderGraphError> {
        let mut graph = FrameGraph::new();

        graph.transient_resources = self.transient_resources;

        let transient_names: Vec<String> = graph
            .transient_resources
            .iter()
            .map(|d| d.name.clone())
            .collect();
        for name in &transient_names {
            graph.create_resource_id(name);
        }
        for name in self.resources.keys() {
            graph.create_resource_id(name);
        }
        for pass_builder in &self.pass_builders {
            for read_name in &pass_builder.reads {
                graph.create_resource_id(read_name);
            }
            for write_name in &pass_builder.writes {
                graph.create_resource_id(write_name);
            }
        }

        let mut global_resource_map = HashMap::new();
        for (name, &resource_id) in &graph.resource_by_name {
            global_resource_map.insert(name.clone(), GraphResourceHandle::new(resource_id.0));
        }
        for (name, handle) in &self.resources {
            global_resource_map.insert(name.clone(), *handle);
        }

        for pass_builder in self.pass_builders {
            let pass_data = (pass_builder.build_fn)(&global_resource_map)?;

            let read_ids: Vec<ResourceId> = pass_builder
                .reads
                .iter()
                .map(|name| {
                    graph
                        .resource_by_name
                        .get(name)
                        .copied()
                        .unwrap_or_else(|| graph.create_resource_id(name))
                })
                .collect();

            let write_ids: Vec<ResourceId> = pass_builder
                .writes
                .iter()
                .map(|name| {
                    graph
                        .resource_by_name
                        .get(name)
                        .copied()
                        .unwrap_or_else(|| graph.create_resource_id(name))
                })
                .collect();

            let mut pass = PassDesc::new(
                pass_builder.name,
                pass_builder.pass_type,
                read_ids,
                write_ids,
            );

            pass.pipeline = pass_builder.pipeline;
            pass.tonemap_params = pass_builder.tonemap_params;
            pass.overlay_params = pass_builder.overlay_params;
            pass.material = pass_builder.material;
            pass.output_format = pass_builder.output_format;
            pass.uses_depth = pass_builder.uses_depth;
            pass.depth_attachment = pass_builder.depth_attachment;
            pass.kind = pass_builder.kind;

            if let Some(geom_data) = pass_data.downcast_ref::<GeometryPassData>() {
                for (handle, format, load_op, store_op, clear_value) in &geom_data.colors {
                    pass.color_attachments.push((
                        ResourceId(handle.index()),
                        *format,
                        *load_op,
                        *store_op,
                        *clear_value,
                    ));
                }
            } else if let Some(dp_data) =
                pass_data
                    .downcast_ref::<crate::render_graph::passes::depth_prepass::DepthPrepassData>()
            {
                for (handle, format, load_op, store_op, clear_value) in &dp_data.colors {
                    pass.color_attachments.push((
                        ResourceId(handle.index()),
                        *format,
                        *load_op,
                        *store_op,
                        *clear_value,
                    ));
                }
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
        let builder = FrameGraphBuilder::new().import_resource("ext", GraphResourceHandle::new(42));

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
}
