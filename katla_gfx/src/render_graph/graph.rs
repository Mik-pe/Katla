//! Frame graph execution types.
//!
//! This module provides the executable [`FrameGraph`] and [`Frame`]
//! types for render graph execution.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::builder::{InternalPassBuilder, PassBuilder};
use super::compiler::{ExecutionPlan, GraphCompiler};
use super::error::RenderGraphError;
use super::pass::PassDesc;
use super::passes::ViewportRect;
use super::passes::geometry::GeometryPassData;
use super::resource::{GraphResourceDesc, GraphResourceHandle};
use crate::handle::PipelineHandle;
use crate::renderer::VulkanRenderer;
use crate::renderer::types::DrawList;
use crate::sync::VkImageView;
use crate::vulkan::context::VulkanContext;
use ash::vk;
use gpu_allocator::vulkan::Allocation;

/// Special resource name for the swapchain backbuffer.
pub const BACKBUFFER_NAME: &str = "backbuffer";

/// Transient texture created and managed by the frame graph.
pub struct TransientTexture {
    /// Vulkan context for cleanup.
    context: Rc<VulkanContext>,
    /// Vulkan image handle.
    pub image: vk::Image,
    /// Memory allocation for the image.
    pub allocation: Option<Allocation>,
    /// Image view for rendering/sampling.
    pub image_view: VkImageView,
    /// Image format.
    pub format: vk::Format,
    /// Image extent.
    pub extent: vk::Extent2D,
    /// Bindless texture slot (if registered with bindless system).
    /// This is used to update the descriptor when the texture is recreated.
    bindless_slot: Option<u32>,
    /// Current GPU layout - tracked to ensure correct barrier old_layout.
    ///
    /// This is CRITICAL for correct synchronization. Using the wrong old_layout
    /// in a barrier causes undefined behavior, including black screens.
    ///
    /// Uses RefCell for interior mutability so layout can be updated during
    /// frame execution even though Frame only has an immutable borrow of FrameGraph.
    current_layout: RefCell<vk::ImageLayout>,
}

impl TransientTexture {
    /// Create a new transient texture.
    pub(crate) fn new(
        context: Rc<VulkanContext>,
        image: vk::Image,
        allocation: Option<Allocation>,
        image_view: VkImageView,
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> Self {
        Self {
            context,
            image,
            allocation,
            image_view,
            format,
            extent,
            bindless_slot: None,
            // Images are created with UNDEFINED layout
            current_layout: RefCell::new(vk::ImageLayout::UNDEFINED),
        }
    }

    /// Get the current tracked GPU layout.
    pub fn current_layout(&self) -> vk::ImageLayout {
        *self.current_layout.borrow()
    }

    /// Update the tracked layout after a barrier transition.
    pub(crate) fn set_layout(&self, new_layout: vk::ImageLayout) {
        *self.current_layout.borrow_mut() = new_layout;
    }

    /// Get the raw Vulkan image view handle.
    pub fn image_view_vk(&self) -> vk::ImageView {
        self.image_view.vk()
    }
}

impl Drop for TransientTexture {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_image_view(self.image_view.vk(), None);
            self.context.device.destroy_image(self.image, None);
            if let Some(allocation) = self.allocation.take() {
                // Use try_borrow_mut to avoid panic if already borrowed
                if let Ok(mut allocator) = self.context.allocator.try_borrow_mut() {
                    allocator.free(allocation).ok();
                }
                // If we can't borrow the allocator, it's already being destroyed,
                // and the memory will be cleaned up when the allocator is dropped
            }
        }
    }
}

/// Executable render graph.
///
/// Built once from a [`FrameGraphBuilder`], executed many times per frame.
pub struct FrameGraph {
    /// Pass descriptors in execution order.
    passes: Vec<PassDesc>,

    /// String -> handle mapping for resources.
    resource_names: HashMap<String, GraphResourceHandle>,

    /// Pass name -> index mapping for execution context.
    pass_names: HashMap<String, usize>,

    /// Compiled execution plan (sorted passes, barriers).
    execution_plan: Option<ExecutionPlan>,

    /// Whether the graph has been compiled.
    compiled: bool,

    /// Transient resource descriptors (for lazy Vulkan resource creation).
    transient_resources: Vec<GraphResourceDesc>,

    /// Created transient textures (frame_idx -> name -> texture).
    /// Double-buffered to match FRAMES_IN_FLIGHT - prevents race conditions
    /// where frame N+1 modifies layout tracking while frame N is still executing.
    transient_textures: Vec<HashMap<String, TransientTexture>>,

    /// Base bindless index for LDR texture (actual index = base + frame_idx).
    ldr_texture_base_index: Option<u32>,

    /// Delta time for this frame (used for particle simulation).
    delta_time: f32,

    /// Global frame counter for this frame (used as random seed for particle simulation).
    frame_count: usize,

    /// Particle rendering pipeline handle.
    particle_pipeline: Option<crate::handle::PipelineHandle>,

    /// Particle emit workgroup count for this frame.
    /// Calculated each frame based on particles to emit.
    particle_emit_workgroup_count: u32,

    /// Particle simulate workgroup count for this frame.
    /// Calculated each frame based on alive particle count.
    particle_simulate_workgroup_count: u32,

    /// Flag to trigger particle debug readback this frame.
    particle_debug_readback: bool,
}

impl FrameGraph {
    /// Create a new empty frame graph.
    pub(crate) fn new() -> Self {
        Self {
            passes: Vec::new(),
            resource_names: HashMap::new(),
            pass_names: HashMap::new(),
            execution_plan: None,
            compiled: false,
            transient_resources: Vec::new(),
            transient_textures: Vec::new(),
            ldr_texture_base_index: None,
            delta_time: 0.0,
            frame_count: 0,
            particle_pipeline: None,
            particle_emit_workgroup_count: 1,
            particle_simulate_workgroup_count: 1,
            particle_debug_readback: false,
        }
    }

    /// Add a pass to the graph.
    pub fn add_pass(&mut self, pass: PassDesc) {
        let index = self.passes.len();
        self.pass_names.insert(pass.name.clone(), index);
        self.passes.push(pass);
        self.compiled = false;
        self.execution_plan = None;
    }

    /// Import a resource into the graph.
    pub(crate) fn import_resource(&mut self, name: impl Into<String>, handle: GraphResourceHandle) {
        self.resource_names.insert(name.into(), handle);
        self.compiled = false;
        self.execution_plan = None;
    }

    /// Compile the graph for execution.
    pub(crate) fn compile(&mut self) -> Result<(), RenderGraphError> {
        if self.compiled {
            return Ok(());
        }

        // Use the graph compiler to analyze dependencies
        let compiler = GraphCompiler::from_pass_descs(&self.passes);
        let execution_plan = compiler.compile()?;

        self.execution_plan = Some(execution_plan);
        self.compiled = true;
        Ok(())
    }

    /// Resolve deferred materials - compile materials for their pass formats.
    ///
    /// For each pass with a material that was created with `ImageFormat::Auto`,
    /// compile the material for the pass's output format.
    fn resolve_materials(&mut self, renderer: &mut VulkanRenderer) -> Result<(), RenderGraphError> {
        for pass in &self.passes {
            if let Some(material_handle) = pass.material
                && let Some(format) = pass.output_format
            {
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
        renderer: &mut VulkanRenderer,
        image_index: u32,
        f: impl FnOnce(&mut Frame),
    ) -> Result<(), RenderGraphError> {
        if !self.compiled {
            self.compile()?;
        }

        // Initialize transient textures on first use
        self.initialize_transient_textures(renderer)?;

        // Get the frame-in-flight index (single source of truth from storage_manager)
        let frame_idx = renderer.current_frame();

        log::debug!(
            "Frame graph execute: frame_idx={}, image_index={}",
            frame_idx,
            image_index
        );

        // Update tonemap params for fullscreen passes BEFORE creating frame.
        //
        // Note: This must happen here (not during pass execution) because we need &mut VulkanRenderer
        // to update storage buffers. Once Frame is created, we only have &VulkanRenderer.
        //
        // IMPORTANT: hdr_texture_index is a BASE slot. We add frame_idx to get the actual slot
        // for this frame's texture (since transient textures are now per-frame).
        for pass in &self.passes {
            if let Some(ref params) = pass.tonemap_params
                && let Some(hdr_base_index) = params.hdr_texture_index
            {
                // Add frame_idx to base slot to get the correct per-frame texture
                let actual_hdr_index = hdr_base_index + frame_idx as u32;

                let mode_value = params.mode as u32;
                renderer.storage_manager.update_object_bindless(
                    frame_idx,
                    0,
                    &[
                        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
                        1.0,
                    ],
                    &[
                        params.exposure,
                        params.gamma,
                        mode_value as f32,
                        actual_hdr_index as f32,
                    ],
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    [0, 0, 0, 0],
                );
            }
        }

        // Resolve deferred materials - compile materials for their pass formats
        self.resolve_materials(renderer)?;

        let mut frame = Frame::new(self, renderer, image_index, frame_idx);
        f(&mut frame);
        frame.execute_passes()?;

        Ok(())
    }

    /// Get a pass index by name.
    pub(crate) fn pass_index(&self, name: &str) -> Option<usize> {
        self.pass_names.get(name).copied()
    }

    /// Get the base bindless index for the LDR (tonemapped) texture.
    ///
    /// With per-frame transient textures, the actual index is `base + frame_idx`.
    /// Returns None if the LDR texture hasn't been registered with bindless.
    pub fn get_ldr_texture_base_index(&self) -> Option<u32> {
        self.ldr_texture_base_index
    }

    /// Set the base bindless index for the LDR texture.
    pub fn set_ldr_texture_base_index(&mut self, index: u32) {
        self.ldr_texture_base_index = Some(index);
    }

    /// Set the delta time for this frame (used for particle simulation).
    pub fn set_delta_time(&mut self, delta_time: f32) {
        self.delta_time = delta_time;
    }

    /// Set the global frame counter for this frame (used for particle simulation).
    pub fn set_frame_count(&mut self, frame_count: usize) {
        self.frame_count = frame_count;
    }

    /// Set the particle rendering pipeline.
    pub fn set_particle_pipeline(&mut self, pipeline: crate::handle::PipelineHandle) {
        self.particle_pipeline = Some(pipeline);
    }

    /// Set the particle emit workgroup count for this frame.
    ///
    /// This should be calculated each frame based on particles to emit.
    pub fn set_particle_emit_workgroup_count(&mut self, count: u32) {
        self.particle_emit_workgroup_count = count;
    }

    /// Set the particle simulate workgroup count for this frame.
    ///
    /// This should be calculated each frame based on alive particle count.
    pub fn set_particle_simulate_workgroup_count(&mut self, count: u32) {
        self.particle_simulate_workgroup_count = count;
    }

    /// Set whether to trigger particle debug readback this frame.
    pub fn set_particle_debug_readback(&mut self, enabled: bool) {
        self.particle_debug_readback = enabled;
    }

    /// Cleanup and destroy all transient textures.
    ///
    /// This should be called before the VulkanRenderer/VulkanContext is destroyed
    /// to ensure proper cleanup order and avoid heap corruption during shutdown.
    ///
    /// Transient textures hold Rc<VulkanContext> and try to free memory in their Drop,
    /// which can cause issues if the VulkanContext is already being destroyed.
    pub fn cleanup(&mut self) {
        log::info!(
            "Cleaning up frame graph transient textures ({} frames)",
            self.transient_textures.len()
        );
        let total_textures: usize = self.transient_textures.iter().map(|m| m.len()).sum();
        log::info!("  Total textures to clean up: {}", total_textures);
        self.transient_textures.clear();
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

    /// Get a transient texture by name for a specific frame.
    ///
    /// Transient textures are double-buffered to match FRAMES_IN_FLIGHT.
    /// Each frame has its own set of textures to prevent race conditions.
    pub fn transient_texture(&self, name: &str, frame_idx: usize) -> Option<&TransientTexture> {
        self.transient_textures.get(frame_idx)?.get(name)
    }

    /// Initialize transient textures (create Vulkan resources).
    ///
    /// Called internally on first use. Can be called explicitly to pre-initialize
    /// transient textures before frame execution (e.g., for bindless registration).
    ///
    /// Creates FRAMES_IN_FLIGHT sets of textures - one per frame index.
    /// This prevents race conditions where frame N+1 modifies layout tracking
    /// while frame N is still executing on the GPU.
    pub fn initialize_transient_textures(
        &mut self,
        renderer: &VulkanRenderer,
    ) -> Result<(), RenderGraphError> {
        if !self.transient_textures.is_empty() {
            return Ok(()); // Already initialized
        }

        const FRAMES_IN_FLIGHT: usize = 2;

        log::info!(
            "Initializing {} transient textures ({} frames in flight)",
            self.transient_resources.len(),
            FRAMES_IN_FLIGHT
        );

        // Create one set of textures per frame
        for _frame_idx in 0..FRAMES_IN_FLIGHT {
            let mut frame_textures = HashMap::new();

            for desc in &self.transient_resources {
                let vk_format: vk::Format = desc.format.into();

                // Create image
                let image_info = vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .extent(vk::Extent3D {
                        width: desc.width,
                        height: desc.height,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .format(vk_format)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .usage(match desc.resource_type {
                        super::resource::GraphResourceType::ColorAttachment { .. } => {
                            vk::ImageUsageFlags::COLOR_ATTACHMENT
                                | vk::ImageUsageFlags::SAMPLED
                                | vk::ImageUsageFlags::INPUT_ATTACHMENT
                        }
                        super::resource::GraphResourceType::DepthAttachment { .. } => {
                            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                        }
                        super::resource::GraphResourceType::SampledImage => {
                            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST
                        }
                    })
                    .sharing_mode(vk::SharingMode::EXCLUSIVE);

                let (image, allocation) = renderer
                    .context
                    .create_image(image_info, gpu_allocator::MemoryLocation::GpuOnly);

                // Create image view
                let view_info = vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(vk_format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: if matches!(
                            desc.resource_type,
                            super::resource::GraphResourceType::DepthAttachment { .. }
                        ) {
                            vk::ImageAspectFlags::DEPTH
                        } else {
                            vk::ImageAspectFlags::COLOR
                        },
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                let image_view = unsafe {
                    renderer
                        .context
                        .device
                        .create_image_view(&view_info, None)
                        .map_err(|e| {
                            RenderGraphError::VulkanError(format!(
                                "Failed to create image view: {}",
                                e
                            ))
                        })?
                };

                let texture = TransientTexture::new(
                    renderer.context.clone(),
                    image,
                    Some(allocation),
                    VkImageView::new(image_view),
                    vk_format,
                    vk::Extent2D {
                        width: desc.width,
                        height: desc.height,
                    },
                );

                frame_textures.insert(desc.name.clone(), texture);
            }

            self.transient_textures.push(frame_textures);
        }

        Ok(())
    }

    /// Recreate transient textures with new dimensions.
    ///
    /// This should be called when the window is resized to ensure transient textures
    /// match the new swapchain dimensions. Old textures are destroyed and new ones
    /// are created with the updated dimensions.
    ///
    /// # Arguments
    /// * `renderer` - The VulkanRenderer
    /// * `new_width` - New width in pixels
    /// * `new_height` - New height in pixels
    ///
    /// # Returns
    /// A vector of (texture_name, bindless_slot) tuples for all recreated textures.
    /// The caller should update any references to bindless slots (e.g., tonemap params).
    ///
    /// # Example
    /// ```ignore
    /// // On window resize
    /// let extent = renderer.swapchain_extent();
    /// let recreated = frame_graph.recreate_transient_textures(&mut renderer, extent.width, extent.height)?;
    ///
    /// // Update tonemap pass with new HDR texture slot
    /// for (name, slot) in recreated {
    ///     if name == "hdr_color" {
    ///         frame_graph.set_tonemap_texture_index("tonemap", slot)?;
    ///     }
    /// }
    /// ```
    pub fn recreate_transient_textures(
        &mut self,
        renderer: &mut VulkanRenderer,
        new_width: u32,
        new_height: u32,
    ) -> Result<Vec<(String, u32)>, RenderGraphError> {
        // Collect existing bindless slots before destroying textures
        let mut existing_slots: std::collections::HashMap<String, Vec<u32>> =
            std::collections::HashMap::new();

        for frame_textures in &self.transient_textures {
            for (name, texture) in frame_textures {
                if let Some(slot) = texture.bindless_slot {
                    existing_slots.entry(name.clone()).or_default().push(slot);
                }
            }
        }

        // Clear existing transient textures (Drop handles cleanup)
        self.transient_textures.clear();

        // Update resource descriptors with new dimensions
        for desc in &mut self.transient_resources {
            desc.width = new_width;
            desc.height = new_height;
        }

        // Recreate textures with new dimensions
        self.initialize_transient_textures(renderer)?;

        // Update all transient textures with their existing bindless slots
        let mut result = Vec::new();
        for (name, slots) in &existing_slots {
            // Update each frame's texture with its existing slot
            for (frame_idx, slot) in slots.iter().enumerate() {
                if let Some(frame_textures) = self.transient_textures.get_mut(frame_idx)
                    && let Some(texture) = frame_textures.get_mut(name)
                {
                    renderer
                        .update_bindless_texture(*slot, texture.image_view.vk())
                        .map_err(|e| {
                            RenderGraphError::VulkanError(format!(
                                "Failed to update bindless texture '{}' frame {}: {}",
                                name, frame_idx, e
                            ))
                        })?;

                    // Store the slot in the new texture
                    texture.bindless_slot = Some(*slot);
                }
            }

            // Return base slot (frame 0) for caller reference
            if let Some(&base_slot) = slots.first() {
                result.push((name.clone(), base_slot));
            }
        }

        // Register any new textures that didn't have existing slots
        let new_texture_names: Vec<String> = self
            .transient_resources
            .iter()
            .filter(|desc| !existing_slots.contains_key(&desc.name))
            .map(|desc| desc.name.clone())
            .collect();

        for name in new_texture_names {
            let slot = self.register_transient_texture_bindless(renderer, &name)?;
            result.push((name, slot));
        }

        Ok(result)
    }

    /// Register a transient texture with the bindless texture system.
    ///
    /// Registers ALL per-frame instances of the texture (one per FRAMES_IN_FLIGHT).
    /// Returns the base slot index; frame N's texture is at `base_slot + N`.
    ///
    /// # Arguments
    /// * `renderer` - The VulkanRenderer (owns the bindless manager), mutably borrowed
    /// * `name` - Name of the transient texture to register
    ///
    /// # Returns
    /// The base bindless texture slot index (u32). Add frame_idx to get the actual slot.
    ///
    /// # Example
    /// ```ignore
    /// // Initialize transient textures first
    /// frame_graph.initialize_transient_textures(&renderer)?;
    ///
    /// // Register HDR texture for tonemapping
    /// let hdr_base_slot = frame_graph.register_transient_texture_bindless(&mut renderer, "hdr_color")?;
    ///
    /// // In shader, use: hdr_base_slot + frame_idx
    /// ```
    pub fn register_transient_texture_bindless(
        &mut self,
        renderer: &mut VulkanRenderer,
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

        // Register each frame's texture and store the slot in the texture
        for frame_idx in 0..num_frames {
            if let Some(frame_textures) = self.transient_textures.get_mut(frame_idx)
                && let Some(texture) = frame_textures.get_mut(name)
            {
                let slot = renderer
                    .register_bindless_texture(texture.image_view.vk())
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Failed to register bindless texture '{}' frame {}: {}",
                            name, frame_idx, e
                        ))
                    })?;

                // Store the slot in the texture for later updates
                texture.bindless_slot = Some(slot);
                log::debug!("  Frame {}: slot {}", frame_idx, slot);
            }
        }

        // Get base slot from frame 0 for the return value
        let base_slot = self
            .transient_textures
            .first()
            .and_then(|textures| textures.get(name))
            .and_then(|texture| texture.bindless_slot)
            .ok_or_else(|| RenderGraphError::ResourceNotFound(name.to_string()))?;

        // Track base index for LDR texture (needed for viewport rendering)
        if name == "ldr_color" {
            self.ldr_texture_base_index = Some(base_slot);
        }

        Ok(base_slot)
    }

    /// Update tonemap parameters for a pass.
    ///
    /// This allows setting the HDR texture index after frame graph compilation,
    /// since the texture needs to be registered first (which happens after build).
    ///
    /// # Arguments
    /// * `pass_name` - Name of the pass to update
    /// * `texture_index` - Bindless texture slot index for the HDR texture
    ///
    /// # Example
    /// ```ignore
    /// // Build frame graph
    /// let graph = FrameGraph::builder()
    ///     .add_pass(tonemap_pass.tonemap(TonemapParams::default()))
    ///     .build(&renderer)?;
    ///
    /// // Register HDR texture and update params
    /// let hdr_slot = graph.register_transient_texture_bindless(&mut renderer, "hdr_color")?;
    /// graph.set_tonemap_texture_index("tonemap", hdr_slot)?;
    /// ```
    pub fn set_tonemap_texture_index(
        &mut self,
        pass_name: &str,
        texture_index: u32,
    ) -> Result<(), RenderGraphError> {
        let pass_idx = self.pass_names.get(pass_name).ok_or_else(|| {
            RenderGraphError::ResourceNotFound(format!("Pass '{}' not found", pass_name))
        })?;

        if let Some(ref mut params) = self.passes[*pass_idx].tonemap_params {
            params.hdr_texture_index = Some(texture_index);
            Ok(())
        } else {
            Err(RenderGraphError::VulkanError(format!(
                "Pass '{}' is not a tonemap pass (no tonemap_params found)",
                pass_name
            )))
        }
    }
}

impl Default for FrameGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing a frame graph.
///
/// Created by [`VulkanRenderer::create_frame_graph()`].
/// Provides a fluent API for adding passes before building the executable [`FrameGraph`].
pub struct FrameGraphBuilder {
    /// Internal pass builders from pass templates.
    pass_builders: Vec<InternalPassBuilder>,

    /// Resource declarations (name -> handle mapping).
    resources: HashMap<String, GraphResourceHandle>,

    /// Transient resource descriptors (created/managed by frame graph).
    transient_resources: Vec<GraphResourceDesc>,
}

impl FrameGraphBuilder {
    /// Create a new frame graph builder.
    pub(crate) fn new() -> Self {
        Self {
            pass_builders: Vec::new(),
            resources: HashMap::new(),
            transient_resources: Vec::new(),
        }
    }

    /// Add a pass to the graph.
    ///
    /// Takes any type implementing the [`PassBuilder`] trait.
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
    ///
    /// The resource will be created when the graph is built and managed
    /// by the frame graph for its lifetime.
    pub fn create_resource(mut self, desc: GraphResourceDesc) -> Self {
        self.transient_resources.push(desc);
        self
    }

    /// Build the frame graph.
    pub fn build(self) -> Result<FrameGraph, RenderGraphError> {
        let mut graph = FrameGraph::new();

        // Import external resources
        for (name, handle) in &self.resources {
            graph.import_resource(name, *handle);
        }

        // Store transient resource descriptors
        graph.transient_resources = self.transient_resources;

        // Build a global resource map that includes all transient resources
        // This ensures consistent handle assignment across all passes
        let mut global_resource_map = HashMap::new();

        // First, add all transient resources to the global map
        for desc in &graph.transient_resources {
            if !global_resource_map.contains_key(&desc.name) {
                global_resource_map.insert(
                    desc.name.clone(),
                    GraphResourceHandle::new(global_resource_map.len() as u32),
                );
            }
        }

        // Add external resources
        for (name, handle) in &self.resources {
            if !global_resource_map.contains_key(name) {
                global_resource_map.insert(name.clone(), *handle);
            }
        }

        // Now add backbuffer and any other implicit resources
        for pass_builder in &self.pass_builders {
            for read_name in &pass_builder.reads {
                if !global_resource_map.contains_key(read_name) {
                    global_resource_map.insert(
                        read_name.clone(),
                        GraphResourceHandle::new(global_resource_map.len() as u32),
                    );
                }
            }
            for write_name in &pass_builder.writes {
                if !global_resource_map.contains_key(write_name) {
                    global_resource_map.insert(
                        write_name.clone(),
                        GraphResourceHandle::new(global_resource_map.len() as u32),
                    );
                }
            }
        }

        // Import all resources from global map into graph
        for (name, handle) in &global_resource_map {
            graph.import_resource(name.clone(), *handle);
        }

        // Build passes using the global resource map
        for pass_builder in self.pass_builders {
            // Call the build function to validate resource references and get pass data
            // Use the global resource map for consistent handle assignment
            let pass_data = (pass_builder.build_fn)(&global_resource_map)?;

            // Create PassDesc with string-based resource references
            let mut pass = PassDesc::new(
                pass_builder.name,
                pass_builder.pass_type,
                pass_builder.reads.clone(),
                pass_builder.writes.clone(),
            );

            pass.pipeline = pass_builder.pipeline;
            pass.tonemap_params = pass_builder.tonemap_params;
            pass.material = pass_builder.material;
            pass.output_format = pass_builder.output_format;
            pass.uses_depth = pass_builder.uses_depth;

            // Extract color attachment info from pass data (for geometry passes)
            if let Some(geom_data) = pass_data.downcast_ref::<GeometryPassData>() {
                // Convert resolved handles back to resource names for color attachments
                for (handle, format, load_op, store_op, clear_value) in &geom_data.colors {
                    // Find the resource name for this handle
                    for (name, candidate_handle) in &global_resource_map {
                        if *candidate_handle == *handle {
                            pass.color_attachments.push((
                                name.clone(),
                                *format,
                                *load_op,
                                *store_op,
                                *clear_value,
                            ));
                            break;
                        }
                    }
                }
            }

            // Extract compositing viewport data (for compositing passes)
            if let Some(comp_data) =
                pass_data.downcast_ref::<crate::render_graph::passes::CompositePassData>()
            {
                // Store viewport data directly (handles are already resolved)
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

/// Frame context for submitting work to passes.
///
/// Passed to the closure in [`VulkanRenderer::render()`]. Provides a simple
/// API for submitting draw lists to named passes.
pub struct Frame<'a> {
    /// Reference to the frame graph.
    graph: &'a FrameGraph,

    /// Reference to the Vulkan renderer.
    /// Mutable reference allows access to per-frame resources like UI buffers.
    renderer: &'a mut VulkanRenderer,

    /// Current swapchain image index being rendered to.
    image_index: u32,

    /// Pending pass execution data.
    pending: HashMap<usize, PassExecutionData>,

    /// Current state of transient resources (name -> state).
    resource_states: HashMap<String, super::resource::ResourceState>,

    /// Per-frame temporary buffers (allocated during this frame, cleaned up after GPU completion).
    temporary_buffers: Vec<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,

    /// Compositing descriptor set for this frame (created once per frame, cleaned up on drop).
    compositing_descriptor_set:
        Option<Box<crate::render_graph::descriptor_sets::CompositingDescriptorSet>>,
}

/// Data for a single pass execution.
#[derive(Default, Clone)]
struct PassExecutionData {
    /// Draw lists to render in this pass.
    draw_lists: Vec<DrawList>,

    /// UI draw lists to render in this pass.
    ui_draw_lists: Vec<crate::renderer::types::UIDrawList>,

    /// Whether dispatch was requested.
    dispatch: Option<(u32, u32, u32)>,

    /// Custom uniform data.
    uniform_data: Vec<u8>,
}

impl<'a> Frame<'a> {
    /// Create a new frame context.
    pub(crate) fn new(
        graph: &'a FrameGraph,
        renderer: &'a mut VulkanRenderer,
        image_index: u32,
        _frame_idx: usize,
    ) -> Self {
        // Initialize all transient resources as Undefined
        let resource_states: HashMap<String, super::resource::ResourceState> = graph
            .transient_resources
            .iter()
            .map(|desc| (desc.name.clone(), super::resource::ResourceState::Undefined))
            .collect();

        Self {
            graph,
            renderer,
            image_index,
            pending: HashMap::new(),
            resource_states,
            temporary_buffers: Vec::new(),
            compositing_descriptor_set: None,
        }
    }

    /// Get the current frame index from the renderer.
    /// This is the authoritative source for which frame's resources to use.
    fn current_frame(&self) -> usize {
        self.renderer.current_frame()
    }

    /// Submit a draw list to a pass.
    pub fn submit(&mut self, pass: &str, draw_list: &DrawList) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        self.pending
            .entry(index)
            .or_default()
            .draw_lists
            .push(draw_list.clone());
        self
    }

    /// Submit a UI draw list to a pass.
    pub fn submit_ui(
        &mut self,
        pass: &str,
        ui_draw_list: &crate::renderer::types::UIDrawList,
    ) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        let cmd_count = ui_draw_list.commands.len();
        self.pending
            .entry(index)
            .or_default()
            .ui_draw_lists
            .push(ui_draw_list.clone());

        log::trace!(
            "submit_ui: pass='{}', index={}, commands={}, pending UI lists now={}",
            pass,
            index,
            cmd_count,
            self.pending[&index].ui_draw_lists.len()
        );

        self
    }

    /// Dispatch compute workgroups for a pass.
    pub fn dispatch(&mut self, pass: &str, x: u32, y: u32, z: u32) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        self.pending.entry(index).or_default().dispatch = Some((x, y, z));
        self
    }

    /// Push uniform data for a pass.
    pub fn push_uniform(&mut self, pass: &str, data: &[u8]) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        self.pending
            .entry(index)
            .or_default()
            .uniform_data
            .extend_from_slice(data);
        self
    }

    /// Execute all passes in order.
    fn execute_passes(&mut self) -> Result<(), RenderGraphError> {
        // Use storage_manager.current_frame() consistently for all frame resource selection
        let frame_idx = self.current_frame();
        log::debug!(
            "=== execute_passes: frame_idx={}, {} passes to execute ===",
            frame_idx,
            self.graph.passes.len()
        );
        for (idx, pass) in self.graph.passes.iter().enumerate() {
            log::trace!(
                "  Pass {}: '{}' (type={:?})",
                idx,
                pass.name,
                pass.pass_type
            );
        }

        // Clone the command buffer to avoid borrowing issues
        let cmd = self.renderer.frame_context.command_buffers[frame_idx].clone();

        // === PHASE 1: Execute compute dispatches (BEFORE any render passes) ===
        // Vulkan doesn't allow compute dispatches inside a render pass, so we must
        // execute all particle simulation compute shaders before beginning any rendering.
        // NOTE: Particle compute is now handled by the render graph via ComputePass.
        // The particle_compute pass executes before all graphics passes automatically.

        // === PHASE 2: Execute graphics passes ===
        for (index, pass) in self.graph.passes.iter().enumerate() {
            let data = self.pending.remove(&index).unwrap_or_default();

            if pass.name == "ui" {
                log::trace!(
                    "UI pass execution: index={}, frame_idx={}, ui_draw_lists={}, commands={}",
                    index,
                    self.current_frame(),
                    data.ui_draw_lists.len(),
                    data.ui_draw_lists
                        .iter()
                        .map(|l| l.commands.len())
                        .sum::<usize>()
                );
            }

            log::trace!(
                "Executing pass '{}' (index {}): pipeline={:?}, draw_lists={}, writes={:?}",
                pass.name,
                index,
                pass.pipeline,
                data.draw_lists.len(),
                pass.writes
            );

            // Track which writes happened this frame (for debugging black screen issues)
            if !pass.writes.is_empty() {
                log::trace!("Pass '{}' writes to: {:?}", pass.name, pass.writes);
            }

            // CRITICAL: Track backbuffer state BEFORE pass execution
            // This allows subsequent passes that write to backbuffer to use LOAD instead of CLEAR
            // For example: compositing pass writes to backbuffer, then UI pass should LOAD that content
            if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
                log::debug!(
                    "Pass '{}' will write to backbuffer, tracking state BEFORE execution",
                    pass.name
                );
                log::debug!(
                    "Current resource_states: {:?}",
                    self.resource_states.keys().collect::<Vec<_>>()
                );
                self.resource_states.insert(
                    BACKBUFFER_NAME.to_string(),
                    super::resource::ResourceState::ColorAttachment,
                );
                log::debug!(
                    "After tracking, resource_states: {:?}",
                    self.resource_states.keys().collect::<Vec<_>>()
                );
            }

            // Insert pre-pass barriers
            self.insert_barriers(&cmd, index)?;

            // Execute pass based on type
            match pass.pass_type {
                super::pass::PassType::Graphics => {
                    // Check if this is a compositing pass (has material AND compositing_viewports)
                    if let Some(material_handle) = pass.material {
                        if pass.compositing_viewports.is_some() && data.draw_lists.is_empty() {
                            log::debug!("'{}' -> compositing pass", pass.name);
                            self.execute_compositing_pass(&cmd, pass, material_handle)?;
                        } else {
                            // Pass has material but is NOT compositing (e.g., UI pass)
                            // Fall through to graphics pass execution
                            log::debug!(
                                "'{}' -> graphics pass with material (draw_lists={}, ui_draw_lists={})",
                                pass.name,
                                data.draw_lists.len(),
                                data.ui_draw_lists.len()
                            );
                            self.execute_graphics_pass(&cmd, pass, data)?;
                        }
                    }
                    // Check if this is a fullscreen pass (has pipeline, no draw lists)
                    else if pass.pipeline.is_some() && data.draw_lists.is_empty() {
                        log::debug!("'{}' -> fullscreen pass", pass.name);
                        if let Some(pipeline) = pass.pipeline {
                            self.execute_fullscreen_pass(&cmd, pass, pipeline)?;
                        }
                    } else {
                        log::debug!(
                            "'{}' -> graphics pass (draw_lists={}, ui_draw_lists={})",
                            pass.name,
                            data.draw_lists.len(),
                            data.ui_draw_lists.len()
                        );
                        self.execute_graphics_pass(&cmd, pass, data)?;
                    }
                }
                super::pass::PassType::Compute => {
                    // Compute pass (e.g., particle simulation)
                    log::debug!("'{}' -> compute pass", pass.name);
                    if let Some(pipeline) = pass.pipeline {
                        self.execute_compute_pass(&cmd, pass, pipeline)?;
                    } else {
                        log::warn!("Compute pass '{}' has no pipeline", pass.name);
                    }
                }
            }

            // Insert post-pass barriers for transient textures that will be read by subsequent passes
            // This ensures proper synchronization between write and read operations
            self.insert_post_pass_barriers(&cmd, index)?;

            // Render particles after tonemap pass (particles render on top of tonemapped output)
            if pass.name == "tonemap" {
                if let Some(ref particle_system) = self.renderer.particle_system {
                    let alive_count = particle_system.alive_count();

                    if alive_count > 0 {
                        // Get viewport_0 texture info
                        if let Some(viewport_texture) = self
                            .graph
                            .transient_textures
                            .get(frame_idx)
                            .and_then(|m| m.get("viewport_0"))
                        {
                            // Render particles to viewport texture
                            if let Err(e) = self.render_particles_to_texture(&cmd, viewport_texture)
                            {
                                log::error!("Failed to render particles: {}", e);
                            }
                        }
                    } else {
                    }
                }
            }
        }

        Ok(())
    }

    /// Insert barriers for a pass.
    ///
    /// Computes required resource states based on pass reads/writes and
    /// inserts layout transitions as needed.
    fn insert_barriers(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pass_index: usize,
    ) -> Result<(), RenderGraphError> {
        use crate::barrier::ImageBarrier;

        let Some(pass) = self.graph.pass(pass_index) else {
            return Ok(());
        };

        log::debug!(
            "[BARRIER] Pre-pass barriers for '{}': reads={:?}, writes={:?}",
            pass.name,
            pass.reads,
            pass.writes
        );

        let cmd_vk = cmd.vk_command_buffer();
        let device = &self.renderer.context.device;

        // Process writes first (color attachments)
        for write_name in &pass.writes {
            // Skip backbuffer - it's managed by the swapchain
            if write_name == BACKBUFFER_NAME {
                continue;
            }

            // Check if this is a transient texture
            let Some(transient) = self
                .graph
                .transient_texture(write_name, self.current_frame())
            else {
                continue;
            };

            let current_state = self
                .resource_states
                .get(write_name)
                .copied()
                .unwrap_or(super::resource::ResourceState::Undefined);

            let required_state = super::resource::ResourceState::ColorAttachment;

            if current_state != required_state {
                let required_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;

                // Get the ACTUAL GPU layout from the transient texture
                // This persists across frames via RefCell
                let old_layout = transient.current_layout();

                log::trace!(
                    "[Barrier] Pass '{}' write '{}': {:?} -> {:?}",
                    pass.name,
                    write_name,
                    old_layout,
                    required_layout
                );

                // Transition using the actual tracked old_layout
                ImageBarrier::transition(
                    &cmd_vk,
                    device,
                    transient.image,
                    old_layout,
                    required_layout,
                );

                // Update tracked state AND GPU layout (persist to TransientTexture for next frame)
                self.resource_states
                    .insert(write_name.clone(), required_state);
                transient.set_layout(required_layout);
            }
        }

        // Process reads (shader resources)
        for read_name in &pass.reads {
            // Skip backbuffer - not read by shaders
            if read_name == BACKBUFFER_NAME {
                continue;
            }

            // Check if this is a transient texture
            let Some(transient) = self
                .graph
                .transient_texture(read_name, self.current_frame())
            else {
                continue;
            };

            log::debug!(
                "[BARRIER] Pass '{}' reading transient texture '{}': current_layout={:?}, format={:?}",
                pass.name,
                read_name,
                transient.current_layout(),
                transient.format
            );

            let current_state = self
                .resource_states
                .get(read_name)
                .copied()
                .unwrap_or(super::resource::ResourceState::Undefined);

            let required_state = super::resource::ResourceState::ShaderRead;

            if current_state != required_state {
                let required_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;

                // Get the ACTUAL GPU layout from the transient texture
                // This persists across frames via RefCell
                let old_layout = transient.current_layout();

                log::debug!(
                    "[BARRIER] Pass '{}' transitioning '{}' from {:?} to {:?}",
                    pass.name,
                    read_name,
                    old_layout,
                    required_layout
                );

                // Transition using the actual tracked old_layout
                ImageBarrier::transition(
                    &cmd_vk,
                    device,
                    transient.image,
                    old_layout,
                    required_layout,
                );

                // Update tracked state AND GPU layout (persist to TransientTexture for next frame)
                self.resource_states
                    .insert(read_name.clone(), required_state);
                transient.set_layout(required_layout);
            }
        }

        Ok(())
    }

    /// Insert post-pass barriers to ensure proper synchronization.
    ///
    /// This method transitions textures written by the current pass to SHADER_READ_ONLY
    /// if subsequent passes will read them. This fixes the black screen issue during
    /// high load where the UI samples from ldr_color before it's properly transitioned.
    ///
    /// # Synchronization Details
    ///
    /// Uses an execution + memory barrier with:
    /// - srcStage: COLOR_ATTACHMENT_OUTPUT (waits for color attachment writes)
    /// - dstStage: FRAGMENT_SHADER (blocks shader sampling)
    /// - srcAccess: COLOR_ATTACHMENT_WRITE (flush color attachment writes)
    /// - dstAccess: SHADER_READ (invalidate shader read caches)
    ///
    /// This ensures that all color attachment writes are visible before any subsequent
    /// fragment shader tries to sample from the texture.
    fn insert_post_pass_barriers(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pass_index: usize,
    ) -> Result<(), RenderGraphError> {
        use crate::barrier::ImageBarrier;

        let Some(current_pass) = self.graph.pass(pass_index) else {
            return Ok(());
        };

        let cmd_vk = cmd.vk_command_buffer();
        let device = &self.renderer.context.device;

        // Check if any subsequent pass reads from textures written by this pass
        for write_name in &current_pass.writes {
            // Skip backbuffer
            if write_name == BACKBUFFER_NAME {
                continue;
            }

            // Check if this is a transient texture
            let Some(transient) = self
                .graph
                .transient_texture(write_name, self.current_frame())
            else {
                continue;
            };

            // Check if any subsequent pass reads from this texture
            let will_be_read = self.graph.passes[pass_index + 1..]
                .iter()
                .any(|pass| pass.reads.contains(write_name));

            if will_be_read {
                let current_state = self
                    .resource_states
                    .get(write_name)
                    .copied()
                    .unwrap_or(super::resource::ResourceState::ColorAttachment);

                // Transition to SHADER_READ_ONLY for subsequent reads
                // Be more aggressive - transition if state is ColorAttachment OR Undefined
                // (Undefined means it was just written and hasn't been tracked yet)
                if current_state == super::resource::ResourceState::ColorAttachment
                    || current_state == super::resource::ResourceState::Undefined
                {
                    // Get the ACTUAL GPU layout from the transient texture
                    // This persists across frames via RefCell
                    let old_layout = transient.current_layout();

                    log::trace!(
                        "[PostBarrier] Pass '{}' -> subsequent reads '{}': {:?} -> SHADER_READ_ONLY",
                        current_pass.name,
                        write_name,
                        old_layout
                    );

                    // Use the tracked old_layout for correct synchronization
                    ImageBarrier::transition(
                        &cmd_vk,
                        device,
                        transient.image,
                        old_layout,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    );

                    // Update tracked state AND GPU layout (persist to TransientTexture for next frame)
                    self.resource_states.insert(
                        write_name.clone(),
                        super::resource::ResourceState::ShaderRead,
                    );
                    transient.set_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
                }
            }
        }

        Ok(())
    }

    /// Execute a graphics pass with dynamic rendering.
    fn execute_graphics_pass(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pass: &PassDesc,
        data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        log::debug!(
            "🎨 [GRAPHICS] PASS '{}' with frame_idx={}, draw_lists={}, ui_draw_lists={}",
            pass.name,
            self.current_frame(),
            data.draw_lists.len(),
            data.ui_draw_lists.len()
        );

        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Determine color attachment:
        // 1. If pass writes to "backbuffer", use swapchain directly
        // 2. If pass writes to a transient texture, use that (frame-indexed)
        // 3. Use load_op from pass.color_attachments if available, otherwise default to CLEAR
        //    For backbuffer: use LOAD if a previous pass already wrote to it
        let color_attachment = if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
            // Explicit backbuffer write - use swapchain
            let swapchain_view =
                self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();

            // Check if a previous pass already wrote to the backbuffer
            let backbuffer_written = self.resource_states.contains_key(BACKBUFFER_NAME);
            let load_op = if backbuffer_written {
                log::debug!(
                    "✅ PASS '{}': Using LOAD for backbuffer (previous pass wrote to it)",
                    pass.name
                );
                vk::AttachmentLoadOp::LOAD
            } else {
                log::warn!(
                    "⚠️  PASS '{}': Using CLEAR for backbuffer (first write) - WILL OVERWRITE PREVIOUS CONTENT!",
                    pass.name
                );
                vk::AttachmentLoadOp::CLEAR
            };

            vk::RenderingAttachmentInfo::default()
                .image_view(swapchain_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(load_op)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.1, 0.1, 0.1, 1.0],
                    },
                })
        } else if let Some(color_name) = pass.writes.first() {
            // Check if this is a transient texture
            if let Some(transient) = self
                .graph
                .transient_texture(color_name, self.current_frame())
            {
                // Check if pass specified load/store ops for this attachment
                let (load_op, store_op, clear_value) = pass
                    .color_attachments
                    .iter()
                    .find(|(name, ..)| name == color_name)
                    .map(|(_, _, load_op, store_op, clear_value)| {
                        (
                            match load_op {
                                crate::render_pass::LoadOp::Load => vk::AttachmentLoadOp::LOAD,
                                crate::render_pass::LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
                                crate::render_pass::LoadOp::DontCare => {
                                    vk::AttachmentLoadOp::NONE_EXT
                                }
                            },
                            match store_op {
                                crate::render_pass::StoreOp::Store => vk::AttachmentStoreOp::STORE,
                                crate::render_pass::StoreOp::DontCare => {
                                    vk::AttachmentStoreOp::NONE_EXT
                                }
                            },
                            match clear_value {
                                crate::render_pass::ClearValue::Color(c) => {
                                    vk::ClearColorValue { float32: *c }
                                }
                                _ => vk::ClearColorValue {
                                    float32: [0.0, 0.0, 0.0, 1.0],
                                },
                            },
                        )
                    })
                    .unwrap_or((
                        vk::AttachmentLoadOp::CLEAR,
                        vk::AttachmentStoreOp::STORE,
                        vk::ClearColorValue {
                            float32: [0.1, 0.1, 0.1, 1.0],
                        },
                    ));

                vk::RenderingAttachmentInfo::default()
                    .image_view(transient.image_view.vk())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(load_op)
                    .store_op(store_op)
                    .clear_value(vk::ClearValue { color: clear_value })
            } else {
                return Err(RenderGraphError::ResourceNotFound(format!(
                    "Color target '{}' not found. Use 'backbuffer' for swapchain or create a transient resource.",
                    color_name
                )));
            }
        } else {
            return Err(RenderGraphError::InvalidConfiguration(
                "Pass has no color outputs. Use .write_color() for transient textures or declare output explicitly".to_string()
            ));
        };

        // Depth attachment (only for passes that use depth testing)
        let depth_attachment = if pass.uses_depth {
            let depth_view = self
                .renderer
                .frame_context
                .depth_render_texture
                .image_view
                .vk();
            Some(
                vk::RenderingAttachmentInfo::default()
                    .image_view(depth_view)
                    .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue {
                        depth_stencil: vk::ClearDepthStencilValue {
                            // Reverse Z: clear to 0.0 (farthest)
                            depth: 0.0,
                            stencil: 0,
                        },
                    }),
            )
        } else {
            None
        };

        // Begin dynamic rendering
        cmd.begin_rendering(
            &[color_attachment],
            depth_attachment.as_ref(),
            None,
            render_area,
            1,
        );

        // Set viewport and scissor
        cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
            0.0,
            0.0,
            extent.width as f32,
            extent.height as f32,
        )]);
        cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
            extent.width,
            extent.height,
        )]);

        // Execute draw lists
        for draw_list in &data.draw_lists {
            self.execute_draw_list(cmd, draw_list)?;
        }

        // Execute UI draw lists
        for ui_draw_list in &data.ui_draw_lists {
            self.execute_ui_draw_list(cmd, pass, ui_draw_list)?;
        }

        // End rendering
        cmd.end_rendering();

        Ok(())
    }

    /// Render particles to a texture using the particle system.
    ///
    /// This starts a new render pass targeting the specified texture.
    fn render_particles_to_texture(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        texture: &TransientTexture,
    ) -> Result<(), RenderGraphError> {
        let _frame_idx = self.current_frame();
        use ash::vk;

        let particle_system = self.renderer.particle_system.as_ref().ok_or_else(|| {
            RenderGraphError::InvalidConfiguration("Particle system not initialized".to_string())
        })?;

        // Check if there are any particles to render
        let alive_count = particle_system.alive_count();
        if alive_count == 0 {
            return Ok(()); // No particles to render
        }

        // Create render pass begin info
        let color_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(texture.image_view.vk())
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD) // Load existing tonemap output
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.0; 4] },
            });

        let rendering_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: texture.extent,
            })
            .layer_count(1)
            .color_attachments(std::slice::from_ref(&color_attachment));

        unsafe {
            self.renderer
                .context
                .device
                .cmd_begin_rendering(cmd.vk_command_buffer(), &rendering_info);
        }

        // Set viewport and scissor
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: texture.extent.width as f32,
            height: texture.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: texture.extent,
        };

        unsafe {
            self.renderer.context.device.cmd_set_viewport(
                cmd.vk_command_buffer(),
                0,
                std::slice::from_ref(&viewport),
            );
            self.renderer.context.device.cmd_set_scissor(
                cmd.vk_command_buffer(),
                0,
                std::slice::from_ref(&scissor),
            );
        }

        // Render particles using the particle system
        // Get storage descriptor set first to avoid borrow conflicts
        let storage_descriptor_set = if self.renderer.particle_system.is_some() {
            Some(self.renderer.storage_descriptor_sets[self.renderer.current_frame()].vk_set())
        } else {
            None
        };

        // Get current frame index before mutable borrow
        let current_frame = self.current_frame();

        if let Some(ref mut particle_system) = self.renderer.particle_system {
            if let Some(pipeline_handle) = particle_system.render_pipeline_handle() {
                // Get the pipeline from the registry
                let pipeline_asset = self
                    .renderer
                    .asset_registry
                    .get_pipeline(pipeline_handle)
                    .ok_or_else(|| {
                        RenderGraphError::InvalidConfiguration(format!(
                            "Particle pipeline {:?} not found in registry",
                            pipeline_handle
                        ))
                    })?;

                let vk_pipeline = pipeline_asset.vk_pipeline();
                let vk_layout = pipeline_asset.vk_layout();

                // Get the storage descriptor set (Set 1) from renderer for FrameUniforms
                let storage_ds = storage_descriptor_set.ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(
                        "Storage descriptor set not available".to_string(),
                    )
                })?;

                // Call particle system render method
                particle_system
                    .render(
                        cmd.vk_command_buffer(),
                        vk::RenderPass::null(), // Using dynamic rendering, not needed
                        vk_pipeline,
                        vk_layout,
                        storage_ds,
                        current_frame,
                    )
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!("Particle render failed: {}", e))
                    })?;

                log::debug!("Drew {} particles successfully", alive_count);
            } else {
                log::warn!("Particle render pipeline not created, skipping particle rendering");
            }
        } else {
            log::warn!("Particle system not available, skipping particle rendering");
        }

        // End render pass
        unsafe {
            self.renderer
                .context
                .device
                .cmd_end_rendering(cmd.vk_command_buffer());
        }

        // Transition texture back to shader read-only for UI sampling
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(texture.image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let dependency_info =
            vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));

        unsafe {
            self.renderer
                .context
                .device
                .cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dependency_info);
        }

        texture.set_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        Ok(())
    }
    /// Execute a draw list.
    fn execute_draw_list(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        draw_list: &DrawList,
    ) -> Result<(), RenderGraphError> {
        log::debug!(
            "execute_draw_list: {} draw calls to execute",
            draw_list.draws.len()
        );

        // Execute regular draw calls
        for draw_call in &draw_list.draws {
            log::debug!(
                "Executing draw call: mesh={:?}, material={:?}",
                draw_call.mesh,
                draw_call.material
            );
            self.execute_draw_call(cmd, draw_call)?;
        }

        log::debug!(
            "execute_draw_list: completed {} draw calls",
            draw_list.draws.len()
        );

        Ok(())
    }

    /// Execute a UI draw list.
    fn execute_ui_draw_list(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pass: &PassDesc,
        ui_draw_list: &crate::renderer::types::UIDrawList,
    ) -> Result<(), RenderGraphError> {
        // Early exit if empty
        if ui_draw_list.is_empty() {
            return Ok(());
        }

        // Get the UI material from the pass
        let material_handle = pass.material.ok_or(RenderGraphError::InvalidConfiguration(
            "UI pass has no material specified. Use .material() on UIPass.".to_string(),
        ))?;

        // Get material asset from registry
        let material = self
            .renderer
            .asset_registry
            .get_material(material_handle)
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;

        // Get pipeline handle from material
        let pipeline_handle = material
            .pipeline
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;
        let (pipeline, pipeline_layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Get or create per-frame UI buffers and upload data
        let frame_idx = self.renderer.current_frame();
        let (vertex_buffer, index_buffer) =
            self.get_or_update_ui_buffers(frame_idx, ui_draw_list)?;

        // Bind vertex and index buffers
        cmd.bind_vertex_buffer(vertex_buffer.0, 0);
        cmd.bind_index_buffer(index_buffer, 0, vk::IndexType::UINT32);

        // Get swapchain extent for scissor (physical pixels)
        let extent = self.renderer.frame_context.swapchain.get_extent();

        // Check font atlas is available
        if self.renderer.ui_renderer.font_atlas_handle().is_none() {
            return Err(RenderGraphError::InvalidConfiguration(
                "UI font atlas not initialized".to_string(),
            ));
        }

        // Bind UI descriptor sets (sampler, uniforms, bindless textures)
        // Use screen_size from draw list (logical pixels, matches vertex coordinates)
        // Bind set 0 once (sampler, uniforms don't change per frame)
        // Bind set 1 once (bindless texture array, shared with 3D materials)
        self.bind_ui_descriptor_sets(
            cmd,
            pipeline_handle,
            pipeline_layout,
            ui_draw_list.screen_size,
        )?;

        // Execute each draw command with scissor clipping
        for draw_cmd in &ui_draw_list.commands {
            // Set scissor for clipping (if specified)
            // clip_rect is in logical pixels, convert to physical pixels for Vulkan scissor
            if let Some([x, y, width, height]) = draw_cmd.clip_rect {
                let scale = ui_draw_list.scale_factor;
                let scissor = crate::sync::Rect2D::new(
                    (x * scale).max(0.0) as i32,
                    (y * scale).max(0.0) as i32,
                    (width * scale).max(0.0) as u32,
                    (height * scale).max(0.0) as u32,
                );
                cmd.set_scissor(&[scissor]);
            } else {
                // No clipping - reset to full screen
                cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
                    extent.width,
                    extent.height,
                )]);
            }

            // Draw indexed
            unsafe {
                self.renderer.context.device.cmd_draw_indexed(
                    cmd.vk_command_buffer(),
                    draw_cmd.index_count,
                    1,
                    draw_cmd.index_offset,
                    0,
                    0,
                );
            }
        }

        // Reset scissor to full screen for next pass
        cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
            extent.width,
            extent.height,
        )]);

        Ok(())
    }

    /// Update per-frame UI vertex and index buffers with new data.
    ///
    /// This reuses buffers across frames to avoid memory leaks. Buffers are resized
    /// if needed to accommodate larger data.
    fn get_or_update_ui_buffers(
        &mut self,
        frame_idx: usize,
        ui_draw_list: &crate::renderer::types::UIDrawList,
    ) -> Result<((vk::Buffer, u32), vk::Buffer), RenderGraphError> {
        let vertex_bytes = bytemuck::cast_slice(&ui_draw_list.vertices);
        let index_bytes = bytemuck::cast_slice(&ui_draw_list.indices);

        // Access UI resources through UIRenderer
        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

        // Update vertex buffer
        let vb = &mut ui_resources.vertex_buffers[frame_idx];
        vb.upload_data(vertex_bytes);
        let vb_handle = (vb.object(), vb.count());

        // Update index buffer
        let ib = &mut ui_resources.index_buffers[frame_idx];
        ib.upload_data(index_bytes);
        let ib_handle = ib.object();

        Ok((vb_handle, ib_handle))
    }

    /// Bind UI descriptor sets (Set 0: font atlas, sampler, uniforms).
    fn bind_ui_descriptor_sets(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pipeline_handle: PipelineHandle,
        pipeline_layout: vk::PipelineLayout,
        screen_size: [f32; 2],
    ) -> Result<(), RenderGraphError> {
        // Get the pipeline to access its descriptor set layouts (separate borrow to avoid conflicts)
        let descriptor_set_layout = {
            let pipeline = self
                .renderer
                .asset_registry
                .get_pipeline(pipeline_handle)
                .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

            let descriptor_set_layouts = pipeline.descriptor_set_layouts();
            if descriptor_set_layouts.is_empty() {
                return Err(RenderGraphError::InvalidConfiguration(
                    "UI pipeline has no descriptor set layouts".to_string(),
                ));
            }

            descriptor_set_layouts[0]
        };

        // Now we can mutate the renderer state
        let frame_idx = self.renderer.current_frame();
        let descriptor_set =
            self.get_or_create_ui_descriptor_set(frame_idx, descriptor_set_layout, screen_size)?;

        // Bind descriptor set 0 (sampler, uniforms)
        cmd.bind_descriptor_sets(pipeline_layout, 0, &[descriptor_set], &[]);

        // Bind descriptor set 1 (bindless texture array - shared with 3D materials)
        let bindless_descriptor_set = self.renderer.bindless_manager.descriptor_set();
        cmd.bind_descriptor_sets(pipeline_layout, 1, &[bindless_descriptor_set.vk()], &[]);

        Ok(())
    }

    /// Get or create per-frame UI descriptor set.
    fn get_or_create_ui_descriptor_set(
        &mut self,
        frame_idx: usize,
        layout: vk::DescriptorSetLayout,
        screen_size: [f32; 2],
    ) -> Result<vk::DescriptorSet, RenderGraphError> {
        // Check if we already have a descriptor set for this frame with the same layout
        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

        // Ensure we have storage for this frame
        while ui_resources.descriptor_sets.len() <= frame_idx {
            ui_resources.descriptor_sets.push(None);
        }

        // Check if we already have a descriptor set for this frame
        let descriptor_set_handle = ui_resources.descriptor_sets[frame_idx]
            .as_ref()
            .map(|ds| ds.vk());

        let _ = ui_resources; // Release borrow before calling update

        if let Some(ds_handle) = descriptor_set_handle {
            // Update uniform buffer with new screen size
            self.update_ui_descriptor_set(ds_handle, screen_size)?;
            return Ok(ds_handle);
        }

        // Create new descriptor set pool and descriptor set
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1),
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        let descriptor_pool = unsafe {
            self.renderer
                .context
                .device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Failed to create UI descriptor pool: {:?}",
                        e
                    ))
                })?
        };

        let layouts = [layout];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe {
            self.renderer
                .context
                .device
                .allocate_descriptor_sets(&allocate_info)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Failed to allocate UI descriptor set: {:?}",
                        e
                    ))
                })?
        };

        let descriptor_set = descriptor_sets[0];

        // Wrap in DescriptorSet for automatic cleanup (owns pool and layout)
        let descriptor_set_wrapper = crate::vulkan::descriptor_set::DescriptorSet::from_raw(
            descriptor_set,
            descriptor_pool,
            None, // Layout is owned by Pipeline, not by the descriptor set
            self.renderer.context.device.clone(),
        );

        // Store descriptor set (owns pool, automatic cleanup)
        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();
        if frame_idx < ui_resources.descriptor_sets.len() {
            ui_resources.descriptor_sets[frame_idx] = Some(descriptor_set_wrapper);
        }
        let _ = ui_resources;

        // Update descriptor set with resources
        self.update_ui_descriptor_set(descriptor_set, screen_size)?;

        Ok(descriptor_set)
    }

    /// Update UI descriptor set with sampler and uniforms.
    fn update_ui_descriptor_set(
        &mut self,
        descriptor_set: vk::DescriptorSet,
        screen_size: [f32; 2],
    ) -> Result<(), RenderGraphError> {
        // Get shared sampler from bindless manager
        let sampler = self.renderer.bindless_manager.shared_sampler();

        // Create or update uniform buffer for screen size
        let uniform_data = [screen_size[0], screen_size[1], 0.0, 0.0];
        let uniform_bytes = bytemuck::cast_slice(&uniform_data);

        // Access UI resources through RefCell
        let uniform_buffer = {
            let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

            // Create or reuse uniform buffer
            if ui_resources.uniform_buffer.is_none() {
                let uniform_buffer_info = vk::BufferCreateInfo::default()
                    .size(uniform_bytes.len() as vk::DeviceSize)
                    .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE);

                let (uniform_buffer, uniform_allocation) = self.renderer.context.allocate_buffer(
                    &uniform_buffer_info,
                    gpu_allocator::MemoryLocation::CpuToGpu,
                );
                ui_resources.uniform_buffer = Some((uniform_buffer, uniform_allocation));
            }

            // Get uniform buffer handle (vk::Buffer is Copy)
            ui_resources.uniform_buffer.as_ref().unwrap().0
        };

        // Now get the allocation for mapping
        let uniform_ptr = {
            let allocation = &self
                .renderer
                .ui_renderer
                .ui_resources_mut()
                .uniform_buffer
                .as_ref()
                .unwrap()
                .1;
            self.renderer.context.map_buffer(allocation)
        };

        // Update uniform data
        unsafe {
            std::ptr::copy_nonoverlapping(uniform_bytes.as_ptr(), uniform_ptr, uniform_bytes.len());
        }

        // Prepare descriptor writes
        let buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(uniform_buffer)
            .offset(0)
            .range(uniform_bytes.len() as vk::DeviceSize);

        let image_info = vk::DescriptorImageInfo::default()
            .sampler(sampler.vk())
            .image_view(vk::ImageView::null()) // Null for sampler-only write
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        let writes = [
            // Binding 1: sampler
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .image_info(std::slice::from_ref(&image_info)),
            // Binding 3: screen size uniform
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .buffer_info(std::slice::from_ref(&buffer_info)),
        ];

        unsafe {
            self.renderer
                .context
                .device
                .update_descriptor_sets(&writes, &[]);
        }

        Ok(())
    }

    /// Execute a single draw call.
    fn execute_draw_call(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        draw_call: &crate::renderer::types::DrawCall,
    ) -> Result<(), RenderGraphError> {
        // Get mesh from registry
        let mesh = self
            .renderer
            .asset_registry
            .get_mesh(draw_call.mesh)
            .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?;

        // Get material from registry
        let material = self
            .renderer
            .asset_registry
            .get_material(draw_call.material)
            .ok_or(RenderGraphError::InvalidMaterialHandle(draw_call.material))?;

        // Clone pipeline_handle to avoid holding borrow across bind_descriptor_sets
        let pipeline_handle = material
            .pipeline
            .ok_or(RenderGraphError::InvalidMaterialHandle(draw_call.material))?;

        // Get pipeline handles from registry
        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Bind vertex buffer
        if let Some(vb) = &mesh.vertex_buffer {
            cmd.bind_vertex_buffer(vb.object(), 0);
        }

        // Bind index buffer
        if let Some(ib) = &mesh.index_buffer {
            cmd.bind_index_buffer(ib.object(), 0, vk::IndexType::UINT32);
        }

        // Extract needed data before borrows end
        let index_count = mesh.index_buffer.as_ref().map(|ib| ib.count()).unwrap_or(0);

        // Material borrow ends here, allowing &mut self call below
        let _ = material;

        // Bind descriptor sets
        self.bind_descriptor_sets(cmd, layout, draw_call)?;

        // Draw indexed (instance_index is used instead of push constants)
        cmd.draw_indexed(index_count, 1, 0, 0, draw_call.instance_index);

        Ok(())
    }

    /// Bind descriptor sets for a draw call.
    ///
    /// Descriptor set layout:
    /// - Set 0: Storage uniforms (frame_data + objects array) - always bound
    /// - Set 1: Bindless textures - always bound for current materials
    /// - Set 2: Skeleton joint matrices - bound only for skinned mesh draws
    fn bind_descriptor_sets(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        draw_call: &crate::renderer::types::DrawCall,
    ) -> Result<(), RenderGraphError> {
        // Set 0: Storage uniforms (frame_data + objects array) - use per-frame descriptor set
        let storage_ds =
            self.renderer.storage_descriptor_sets[self.renderer.current_frame()].vk_set();
        cmd.bind_descriptor_sets(pipeline_layout, 0, &[storage_ds], &[]);

        // Set 1: Bindless textures (all current materials use bindless)
        let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(pipeline_layout, 1, &[bindless_ds], &[]);

        // Set 2: Skeleton joint matrices (only when draw_call has skeleton)
        if !draw_call.skeleton.is_none() {
            let skeleton_ds = self
                .renderer
                .get_skeleton_descriptor(draw_call.skeleton)
                .ok_or(RenderGraphError::InvalidSkeletonHandle(draw_call.skeleton))?;
            cmd.bind_descriptor_sets(pipeline_layout, 2, &[skeleton_ds.vk_set()], &[]);
        }

        Ok(())
    }

    /// Execute a fullscreen pass (draws a fullscreen triangle).
    fn execute_fullscreen_pass(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pass: &PassDesc,
        pipeline_handle: crate::handle::PipelineHandle,
    ) -> Result<(), RenderGraphError> {
        let current_frame = self.current_frame();
        log::debug!(
            "[FULLSCREEN] Pass '{}' execution: frame_idx={}, writes={:?}, reads={:?}",
            pass.name,
            current_frame,
            pass.writes,
            pass.reads
        );

        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Determine color attachment:
        // 1. If pass writes to "backbuffer", use swapchain directly
        // 2. If pass writes to a transient texture, use that (frame-indexed)
        // 3. Use load_op from pass.color_attachments if available, otherwise default to CLEAR
        let color_attachment = if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
            // Explicit backbuffer write - use swapchain
            let swapchain_view =
                self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();
            vk::RenderingAttachmentInfo::default()
                .image_view(swapchain_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.1, 0.1, 0.1, 1.0],
                    },
                })
        } else if let Some(color_name) = pass.writes.first() {
            // Check if this is a transient texture (fullscreen pass like tonemap)
            let frame_idx = self.current_frame();
            if let Some(transient) = self.graph.transient_texture(color_name, frame_idx) {
                log::debug!(
                    "[FULLSCREEN] Pass '{}' writing to transient texture '{}' at frame_idx={}, format={:?}, extent={}x{}",
                    pass.name,
                    color_name,
                    frame_idx,
                    transient.format,
                    transient.extent.width,
                    transient.extent.height
                );

                // Check if pass specified load/store ops for this attachment
                let (load_op, store_op, clear_value) = pass
                    .color_attachments
                    .iter()
                    .find(|(name, ..)| name == color_name)
                    .map(|(_, _, load_op, store_op, clear_value)| {
                        (
                            match load_op {
                                crate::render_pass::LoadOp::Load => vk::AttachmentLoadOp::LOAD,
                                crate::render_pass::LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
                                crate::render_pass::LoadOp::DontCare => {
                                    vk::AttachmentLoadOp::NONE_EXT
                                }
                            },
                            match store_op {
                                crate::render_pass::StoreOp::Store => vk::AttachmentStoreOp::STORE,
                                crate::render_pass::StoreOp::DontCare => {
                                    vk::AttachmentStoreOp::NONE_EXT
                                }
                            },
                            match clear_value {
                                crate::render_pass::ClearValue::Color(c) => {
                                    vk::ClearColorValue { float32: *c }
                                }
                                _ => vk::ClearColorValue {
                                    float32: [0.0, 0.0, 0.0, 1.0],
                                },
                            },
                        )
                    })
                    .unwrap_or((
                        vk::AttachmentLoadOp::CLEAR,
                        vk::AttachmentStoreOp::STORE,
                        vk::ClearColorValue {
                            float32: [0.1, 0.1, 0.1, 1.0],
                        },
                    ));

                vk::RenderingAttachmentInfo::default()
                    .image_view(transient.image_view.vk())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(load_op)
                    .store_op(store_op)
                    .clear_value(vk::ClearValue { color: clear_value })
            } else {
                return Err(RenderGraphError::ResourceNotFound(format!(
                    "Color target '{}' not found. Use 'backbuffer' for swapchain or create a transient resource.",
                    color_name
                )));
            }
        } else {
            return Err(RenderGraphError::InvalidConfiguration(
                "Pass has no color outputs. Use 'backbuffer' for swapchain or create a transient resource.".to_string()
            ));
        };

        // Begin dynamic rendering
        cmd.begin_rendering(
            &[color_attachment],
            None, // No depth attachment for fullscreen passes
            None,
            render_area,
            1,
        );

        // Set viewport and scissor
        cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
            0.0,
            0.0,
            extent.width as f32,
            extent.height as f32,
        )]);
        cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
            extent.width,
            extent.height,
        )]);

        // Get pipeline from registry
        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Bind descriptor sets (storage uniforms + bindless textures) - use per-frame descriptor set
        let storage_ds =
            self.renderer.storage_descriptor_sets[self.renderer.current_frame()].vk_set();
        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

        let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(layout, 1, &[bindless_ds], &[]);

        // Draw fullscreen triangle (3 vertices, no index buffer)
        // Vertex shader generates fullscreen triangle from vertex ID
        cmd.draw_array(3, 1, 0, 0);

        // End rendering
        cmd.end_rendering();

        Ok(())
    }

    /// Execute a compute pass (GPU compute work).
    ///
    /// Compute passes perform general-purpose GPU computation without rendering to attachments.
    /// Used for particle simulation, physics, and other compute-intensive tasks.
    ///
    /// # Compute-Specific Behavior
    ///
    /// 1. **Bind compute pipeline**: Set pipeline for compute work
    /// 2. **Bind descriptor sets**: Set 0 (static buffers) + Set 1 (push descriptors if needed)
    /// 3. **Dispatch compute shader**: Execute with specified workgroup count
    fn execute_compute_pass(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pass: &PassDesc,
        pipeline_handle: crate::handle::PipelineHandle,
    ) -> Result<(), RenderGraphError> {
        let current_frame = self.current_frame();
        log::debug!(
            "[COMPUTE] Pass '{}' execution: frame_idx={}, pipeline={:?}",
            pass.name,
            current_frame,
            pipeline_handle
        );

        let device = &self.renderer.context.device;

        // Get compute pipeline from registry
        let compute_pipeline = self
            .renderer
            .asset_registry
            .get_pipeline(pipeline_handle)
            .ok_or_else(|| {
                RenderGraphError::PipelineNotSet(format!(
                    "Pipeline {:?} not found",
                    pipeline_handle
                ))
            })?;

        let vk_pipeline = compute_pipeline.vk_pipeline();

        // Bind compute pipeline
        unsafe {
            device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::COMPUTE,
                vk_pipeline,
            );
        }

        // Get current frame index before any mutable borrows
        let current_frame = self.current_frame();

        // Bind descriptor sets if particle system is active
        // Note: Particle system manages its own descriptor sets
        if let Some(ref mut particle_system) = self.renderer.particle_system
            && pass.name.contains("particle")
        {
            log::debug!("Executing particle compute pass '{}'", pass.name);

            // Use pre-calculated workgroup count from frame graph
            // These were calculated in renderer.rs based on current particle state
            let workgroup_count = if pass.name.contains("emit") {
                self.graph.particle_emit_workgroup_count
            } else if pass.name.contains("simulate") {
                self.graph.particle_simulate_workgroup_count
            } else {
                log::warn!(
                    "Unknown particle compute pass '{}', using default workgroup count",
                    pass.name
                );
                1
            };

            // Before recording dispatch
            if workgroup_count == 0 {
                log::warn!(
                    "Skipping particle compute pass '{}' - workgroup_count is 0",
                    pass.name
                );
                return Ok(()); // Skip dispatch
            }

            // Record the appropriate dispatch based on pass name
            if pass.name.contains("emit") {
                // Update compute descriptor bindings for EMIT pass
                // CRITICAL: Emit needs binding 3 to point to alive_current[frame_index]
                // so that newly emitted particles are written where simulate will read them
                particle_system
                    .update_compute_descriptor_binding_for_emit(current_frame)
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Failed to update particle compute descriptor binding for emit: {}",
                            e
                        ))
                    })?;
                particle_system
                    .record_emit_dispatch(
                        cmd.vk_command_buffer(),
                        &self.renderer.asset_registry,
                        workgroup_count,
                    )
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Particle emit dispatch failed: {}",
                            e
                        ))
                    })?;

                // Add memory barrier after emit to ensure simulate sees the writes
                particle_system
                    .emit_to_simulate_barrier(cmd.vk_command_buffer())
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Particle emit barrier failed: {}",
                            e
                        ))
                    })?;
            } else if pass.name.contains("simulate") {
                // Update compute descriptor bindings for SIMULATE pass
                // CRITICAL: Simulate needs binding 3 to point to alive_next buffer
                // so that survivors are written to the correct location for swapping
                particle_system
                    .update_compute_descriptor_binding_for_simulate(current_frame)
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Failed to update particle compute descriptor binding for simulate: {}",
                            e
                        ))
                    })?;
                particle_system
                    .record_simulate_dispatch(
                        cmd.vk_command_buffer(),
                        &self.renderer.asset_registry,
                        workgroup_count,
                    )
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Particle simulate dispatch failed: {}",
                            e
                        ))
                    })?;

                // Swap alive lists after simulate pass completes
                // This copies alive_next (written by simulate) to alive_current (read by emit next frame)
                // The alive_count counter is preserved from the simulate pass (no reset, no GPU update)
                log::debug!("About to call swap_alive_lists()...");
                particle_system
                    .swap_alive_lists(cmd.vk_command_buffer(), current_frame)
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!("Particle buffer swap failed: {}", e))
                    })?;

                log::debug!("swap_alive_lists() completed successfully");

                // Record particle debug readback if requested this frame
                // SAFETY: We need to access the graph's debug readback flag through the Frame's graph reference
                // This is safe because we're in the middle of frame execution and have exclusive access
                let graph_ptr = self.graph as *const FrameGraph as *mut FrameGraph;
                unsafe {
                    if (*graph_ptr).particle_debug_readback {
                        log::info!("Recording particle debug readback after simulate pass");
                        particle_system
                            .record_debug_readback(cmd.vk_command_buffer())
                            .map_err(|e| {
                                RenderGraphError::VulkanError(format!(
                                    "Particle debug readback failed: {}",
                                    e
                                ))
                            })?;
                        // Reset flag after recording
                        (*graph_ptr).particle_debug_readback = false;
                    }
                }
            }

            return Ok(());
        }

        // Generic compute dispatch for non-particle compute passes
        // TODO: Calculate workgroup count based on work size
        unsafe {
            device.cmd_dispatch(cmd.vk_command_buffer(), 64, 1, 1);
        }

        log::debug!("Compute pass '{}' executed successfully", pass.name);
        Ok(())
    }

    /// Execute a compositing pass (multi-viewport fullscreen pass).
    ///
    /// Compositing passes sample from multiple viewport textures and composite them
    /// onto the final output using viewport rectangles for positioning.
    ///
    /// # Compositing-Specific Behavior
    ///
    /// 1. **Update compositing uniforms**: Upload viewport rectangles to storage buffer
    /// 2. **Bind compositing descriptor set**: Set 2 with viewport texture array
    /// 3. **Draw fullscreen triangle**: Standard fullscreen draw with compositing shader
    fn execute_compositing_pass(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pass: &PassDesc,
        material_handle: crate::handle::MaterialHandle,
    ) -> Result<(), RenderGraphError> {
        let current_frame = self.current_frame();
        let viewports =
            pass.compositing_viewports
                .as_ref()
                .ok_or(RenderGraphError::InvalidConfiguration(
                    "Compositing pass missing viewport data".to_string(),
                ))?;

        log::debug!(
            "[COMPOSITING] Pass '{}' execution: frame_idx={}, viewport_count={}, writes={:?}",
            pass.name,
            current_frame,
            viewports.len(),
            pass.writes
        );

        let extent = self.renderer.frame_context.swapchain.get_extent();

        // Update compositing uniforms (viewport rectangles and screen size)
        // We use objects[0] for fullscreen/post-processing passes (similar to tonemap)
        let viewport_count = viewports.len() as u32;
        let screen_size = [extent.width as f32, extent.height as f32];

        // Get viewport texture bindless index
        // With per-frame transient textures, the actual index is base + frame_idx
        let viewport_bindless_idx = if let Some(base_idx) = self.graph.get_ldr_texture_base_index()
        {
            base_idx + current_frame as u32
        } else {
            log::warn!(
                "[COMPOSITING] LDR texture not registered with bindless system, using index 0"
            );
            0
        };

        // Encode viewport count, screen size, and bindless index in objects[0]
        // base_color.rg = screen_size (width, height)
        // base_color.a = viewport bindless texture index
        // material_params.x = viewport count
        self.renderer.storage_manager.update_object_bindless(
            current_frame,
            0,          // Slot 0 for fullscreen passes
            &[0.0; 16], // Identity matrix (unused)
            &[
                screen_size[0],               // base_color.r = screen width
                screen_size[1],               // base_color.g = screen height
                0.0,                          // base_color.b = unused
                viewport_bindless_idx as f32, // base_color.a = viewport bindless index
            ],
            viewport_count as f32, // material_params.x = viewport count
            0.0,                   // material_params.y = unused
            0.0,                   // material_params.z = unused
            0.0,                   // material_params.w = unused
            [0, 0, 0, 0],          // texture_indices = unused
        );

        // TODO: Pass viewport rectangles via proper uniform buffer
        // For now, the shader uses a simple hardcoded split-screen layout
        // This will be enhanced in a follow-up to support arbitrary viewport rectangles

        // Create or update compositing descriptor set with viewport textures
        let compositing_desc_set =
            self.get_or_create_compositing_descriptor_set(viewports, current_frame)?;

        // Get swapchain extent for rendering
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Determine color attachment (backbuffer or transient texture)
        let color_attachment = if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
            // Write to backbuffer
            let swapchain_view =
                self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();
            vk::RenderingAttachmentInfo::default()
                .image_view(swapchain_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
                    },
                })
        } else if let Some(color_name) = pass.writes.first() {
            // Write to transient texture
            let frame_idx = self.current_frame();
            if let Some(transient) = self.graph.transient_texture(color_name, frame_idx) {
                vk::RenderingAttachmentInfo::default()
                    .image_view(transient.image_view.vk())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    })
            } else {
                return Err(RenderGraphError::ResourceNotFound(format!(
                    "Output target '{}' not found",
                    color_name
                )));
            }
        } else {
            return Err(RenderGraphError::InvalidConfiguration(
                "Compositing pass has no output target".to_string(),
            ));
        };

        // Begin dynamic rendering
        cmd.begin_rendering(
            &[color_attachment],
            None, // No depth attachment for compositing
            None,
            render_area,
            1,
        );

        // Set viewport and scissor
        cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
            0.0,
            0.0,
            extent.width as f32,
            extent.height as f32,
        )]);
        cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
            extent.width,
            extent.height,
        )]);

        // Get material and pipeline from registry
        let material = self
            .renderer
            .asset_registry
            .get_material(material_handle)
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;

        let pipeline_handle = material
            .pipeline
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;

        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Bind descriptor sets
        // Set 0: Storage uniforms (frame_data + objects array)
        let storage_ds = self.renderer.storage_descriptor_sets[current_frame].vk_set();
        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

        // Set 1: Bindless textures (shared with all materials)
        let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(layout, 1, &[bindless_ds], &[]);

        // Set 2: Compositing descriptor set (viewport texture array)
        cmd.bind_descriptor_sets(layout, 2, &[compositing_desc_set], &[]);

        // Draw fullscreen triangle (3 vertices)
        cmd.draw_array(3, 1, 0, 0);

        // End rendering
        cmd.end_rendering();

        Ok(())
    }

    /// Get or create compositing descriptor set for current frame.
    ///
    /// Creates or updates a descriptor set with the viewport texture array.
    /// The descriptor set is cached per-frame and updated when viewport textures change.
    fn get_or_create_compositing_descriptor_set(
        &mut self,
        viewports: &[(GraphResourceHandle, ViewportRect)],
        frame_idx: usize,
    ) -> Result<vk::DescriptorSet, RenderGraphError> {
        use crate::render_graph::descriptor_sets::CompositingDescriptorSet;
        use std::rc::Rc;

        // Collect viewport texture image views
        let mut texture_views = Vec::with_capacity(viewports.len());
        for (handle, _rect) in viewports {
            // Find the texture resource name from the handle
            let resource_name = self
                .graph
                .resource_names
                .iter()
                .find(|&(_, h)| *h == *handle)
                .map(|(name, _)| name.clone())
                .ok_or_else(|| {
                    RenderGraphError::ResourceNotFound(format!(
                        "Viewport texture handle {} not found in resource names",
                        handle.index()
                    ))
                })?;

            log::debug!(
                "[COMPOSITING] Looking up viewport texture: '{}' (handle={})",
                resource_name,
                handle.index()
            );

            // Get the transient texture
            let transient = self
                .graph
                .transient_texture(&resource_name, frame_idx)
                .ok_or_else(|| {
                    log::error!(
                        "[COMPOSITING] Failed to find viewport texture '{}' for frame {}",
                        resource_name,
                        frame_idx
                    );
                    RenderGraphError::ResourceNotFound(format!(
                        "Viewport texture '{}' not found for frame {}",
                        resource_name, frame_idx
                    ))
                })?;

            log::debug!(
                "[COMPOSITING] Found viewport texture '{}': format={:?}, extent={}x{}",
                resource_name,
                transient.format,
                transient.extent.width,
                transient.extent.height
            );

            texture_views.push(transient.image_view.vk());
        }

        // Create descriptor set and store it in the frame for cleanup
        let context = Rc::clone(&self.renderer.context);
        let desc_set = Box::new(
            CompositingDescriptorSet::new(&context, &texture_views).map_err(|e| {
                RenderGraphError::VulkanError(format!(
                    "Failed to create compositing descriptor set: {}",
                    e
                ))
            })?,
        );

        let vk_set = desc_set.vk_set();
        self.compositing_descriptor_set = Some(desc_set);

        Ok(vk_set)
    }
}

impl<'a> Drop for Frame<'a> {
    fn drop(&mut self) {
        // Clean up temporary buffers created during this frame
        for (buffer, allocation) in self.temporary_buffers.drain(..) {
            unsafe {
                self.renderer.context.device.destroy_buffer(buffer, None);
            }
            self.renderer
                .context
                .allocator
                .borrow_mut()
                .free(allocation)
                .ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_graph_new() {
        let graph = FrameGraph::new();
        assert_eq!(graph.pass_count(), 0);
        assert!(!graph.compiled);
    }

    #[test]
    fn test_frame_graph_default() {
        let graph = FrameGraph::default();
        assert_eq!(graph.pass_count(), 0);
    }

    #[test]
    fn test_frame_graph_builder_new() {
        let builder = FrameGraphBuilder::new();
        assert!(builder.pass_builders.is_empty());
    }

    #[test]
    fn test_frame_graph_builder_default() {
        let builder = FrameGraphBuilder::default();
        assert!(builder.pass_builders.is_empty());
    }

    #[test]
    fn test_pass_execution_data_default() {
        let data = PassExecutionData::default();
        assert!(data.draw_lists.is_empty());
        assert!(data.dispatch.is_none());
        assert!(data.uniform_data.is_empty());
    }
}
