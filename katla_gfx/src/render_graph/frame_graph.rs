use std::cell::RefCell;
use std::collections::HashMap;

use super::builder::{InternalPassBuilder, PassBuilder};
use super::compiler::{ExecutionPlan, GraphCompiler};
use super::error::RenderGraphError;
use super::pass::PassDesc;
use super::passes::geometry::GeometryPassData;
use super::resource::{GraphResourceDesc, GraphResourceHandle};
use super::transient_texture::TransientTexture;
use crate::renderer::VulkanRenderer;
use crate::sync::VkImageView;
use ash::vk;

/// Special resource name for the swapchain backbuffer.
pub const BACKBUFFER_NAME: &str = "backbuffer";

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
    pub particle_debug_readback: bool,
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
            particle_debug_readback: false,
        }
    }
}

/// Executable render graph.
///
/// Built once from a [`FrameGraphBuilder`], executed many times per frame.
pub struct FrameGraph {
    /// Pass descriptors in execution order.
    pub(super) passes: Vec<PassDesc>,

    /// String -> handle mapping for resources.
    pub(super) resource_names: HashMap<String, GraphResourceHandle>,

    /// Pass name -> index mapping for execution context.
    pub(super) pass_names: HashMap<String, usize>,

    /// Compiled execution plan (sorted passes, barriers).
    execution_plan: Option<ExecutionPlan>,

    /// Whether the graph has been compiled.
    compiled: bool,

    /// Transient resource descriptors (for lazy Vulkan resource creation).
    pub(super) transient_resources: Vec<GraphResourceDesc>,

    /// Created transient textures (frame_idx -> name -> texture).
    /// Double-buffered to match FRAMES_IN_FLIGHT - prevents race conditions
    /// where frame N+1 modifies layout tracking while frame N is still executing.
    pub(super) transient_textures: Vec<HashMap<String, TransientTexture>>,

    /// Base bindless index for LDR texture (actual index = base + frame_idx).
    ldr_texture_base_index: Option<u32>,

    /// Per-frame parameters set before each `execute()` call.
    /// These are logically separate from the graph structure itself,
    /// which is "built once, executed many times."
    pub(super) params: FrameParams,

    /// Per-frame compositing descriptor sets (one per frame in flight).
    /// Pre-allocated and reused each frame via update_textures().
    pub(super) compositing_descriptor_sets:
        RefCell<Vec<Option<Box<crate::render_graph::descriptor_sets::CompositingDescriptorSet>>>>,
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
            params: FrameParams::default(),
            compositing_descriptor_sets: RefCell::new(vec![None, None]),
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
            if let Some(material_handle) = pass.material {
                // Use pass output_format if available, otherwise use B8G8R8A8Srgb
                // (the default for materials compiled without explicit format)
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
        renderer: &mut VulkanRenderer,
        image_index: u32,
        f: impl FnOnce(&mut super::frame::Frame),
    ) -> Result<(), RenderGraphError> {
        if !self.compiled {
            self.compile()?;
        }

        // Initialize transient textures on first use
        self.initialize_transient_textures(renderer)?;

        // Get the frame-in-flight index (single source of truth from storage_manager)
        let frame_idx = renderer.current_frame();

        log::trace!(
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

            // Update overlay params for wallhack overlay passes.
            if let Some(ref params) = pass.overlay_params {
                let ldr_idx = params
                    .ldr_texture_index
                    .map(|base| base + frame_idx as u32)
                    .unwrap_or(0);
                let indicator_idx = params
                    .stencil_indicator_index
                    .map(|base| base + frame_idx as u32)
                    .unwrap_or(0);

                renderer.storage_manager.update_object_bindless(
                    frame_idx,
                    0,
                    &[
                        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
                        1.0,
                    ],
                    &[ldr_idx as f32, indicator_idx as f32, 0.0, 0.0],
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

        let mut frame = super::frame::Frame::new(self, renderer, image_index, frame_idx);
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
        self.params.delta_time = delta_time;
    }

    /// Set the global frame counter for this frame (used for particle simulation).
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

    /// Set whether to trigger particle debug readback this frame.
    pub fn set_particle_debug_readback(&mut self, enabled: bool) {
        self.params.particle_debug_readback = enabled;
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
    ///
    /// Returns pass indices in topologically sorted order (dependencies first).
    /// Falls back to insertion order if the graph hasn't been compiled.
    pub(crate) fn execution_order(&self) -> Vec<usize> {
        self.execution_plan
            .as_ref()
            .map(|plan| plan.sorted_passes.clone())
            .unwrap_or_else(|| (0..self.passes.len()).collect())
    }

    /// Get a transient texture by name for a specific frame.
    ///
    /// Transient textures are double-buffered to match FRAMES_IN_FLIGHT.
    /// Each frame has its own set of textures to prevent race conditions.
    pub fn transient_texture(&self, name: &str, frame_idx: usize) -> Option<&TransientTexture> {
        self.transient_textures.get(frame_idx)?.get(name)
    }

    /// Get the ImageView of a transient texture by name (frame 0).
    ///
    /// Useful for external systems that need to reference transient textures
    /// in descriptor sets (e.g., shadow atlas).
    pub fn transient_texture_view(&self, name: &str) -> Option<vk::ImageView> {
        self.transient_texture(name, 0).map(|t| t.image_view.vk())
    }

    /// Get the ImageView of a transient texture by name for a specific frame.
    pub fn transient_texture_view_for_frame(
        &self,
        name: &str,
        frame_idx: usize,
    ) -> Option<vk::ImageView> {
        self.transient_texture(name, frame_idx)
            .map(|t| t.image_view.vk())
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
                        super::resource::GraphResourceType::DepthAttachment { sampled, .. } => {
                            let mut usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
                            if sampled {
                                usage |= vk::ImageUsageFlags::SAMPLED;
                            }
                            usage
                        }
                        super::resource::GraphResourceType::SampledImage => {
                            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST
                        }
                    })
                    .sharing_mode(vk::SharingMode::EXCLUSIVE);

                let (image, allocation) = renderer
                    .context
                    .create_image(image_info, gpu_allocator::MemoryLocation::GpuOnly)
                    .expect("Failed to create graph image");

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

        // Ensure per-frame compositing descriptor set storage matches frame count
        while self.compositing_descriptor_sets.borrow().len() < FRAMES_IN_FLIGHT {
            self.compositing_descriptor_sets.borrow_mut().push(None);
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
        // Only resize textures that track swapchain size (skip fixed-size like shadow atlas)
        for desc in &mut self.transient_resources {
            if desc.tracks_swapchain_size {
                desc.width = new_width;
                desc.height = new_height;
            }
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
                log::trace!("  Frame {}: slot {}", frame_idx, slot);
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

    /// Set overlay texture indices for the wallhack overlay pass.
    pub fn set_overlay_texture_indices(
        &mut self,
        pass_name: &str,
        ldr_texture_index: u32,
        stencil_indicator_index: u32,
    ) -> Result<(), RenderGraphError> {
        let pass_idx = self.pass_names.get(pass_name).ok_or_else(|| {
            RenderGraphError::ResourceNotFound(format!("Pass '{}' not found", pass_name))
        })?;

        if let Some(ref mut params) = self.passes[*pass_idx].overlay_params {
            params.ldr_texture_index = Some(ldr_texture_index);
            params.stencil_indicator_index = Some(stencil_indicator_index);
            Ok(())
        } else {
            Err(RenderGraphError::VulkanError(format!(
                "Pass '{}' is not an overlay pass (no overlay_params found)",
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
            pass.overlay_params = pass_builder.overlay_params;
            pass.material = pass_builder.material;
            pass.output_format = pass_builder.output_format;
            pass.uses_depth = pass_builder.uses_depth;
            pass.depth_attachment = pass_builder.depth_attachment;
            pass.kind = pass_builder.kind;

            // Extract color attachment info from pass data (for geometry and depth prepass passes)
            if let Some(geom_data) = pass_data.downcast_ref::<GeometryPassData>() {
                // Convert resolved handles back to resource names for color attachments
                for (handle, format, load_op, store_op, clear_value) in &geom_data.colors {
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
            } else if let Some(dp_data) =
                pass_data
                    .downcast_ref::<crate::render_graph::passes::depth_prepass::DepthPrepassData>()
            {
                for (handle, format, load_op, store_op, clear_value) in &dp_data.colors {
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

#[cfg(test)]
mod tests {
    use super::super::pass::PassType;
    use super::*;

    #[test]
    fn test_frame_graph_add_and_index_passes() {
        let mut graph = FrameGraph::new();
        let p1 = PassDesc::new("a", PassType::Graphics, vec![], vec!["r1".to_string()]);
        let p2 = PassDesc::new(
            "b",
            PassType::Graphics,
            vec!["r1".to_string()],
            vec!["r2".to_string()],
        );

        graph.add_pass(p1);
        graph.add_pass(p2);

        assert_eq!(graph.pass_count(), 2);
        assert_eq!(graph.pass_index("a"), Some(0));
        assert_eq!(graph.pass_index("b"), Some(1));
        assert_eq!(graph.pass_index("nonexistent"), None);
    }

    #[test]
    fn test_frame_graph_insert_pass_reindexes() {
        let mut graph = FrameGraph::new();
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
        let mut graph = FrameGraph::new();
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
}
