//! Frame graph execution types.
//!
//! This module provides the executable [`FrameGraph`] and [`Frame`]
//! types for render graph execution.

use std::collections::HashMap;
use std::rc::Rc;

use super::builder::{InternalPassBuilder, PassBuilder};
use super::compiler::{ExecutionPlan, GraphCompiler};
use super::error::RenderGraphError;
use super::pass::PassDesc;
use super::passes::geometry::GeometryPassData;
use super::resource::{GraphResourceDesc, GraphResourceHandle};
use crate::handle::{PipelineHandle, TextureHandle};
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
}

impl TransientTexture {
    /// Create a new transient texture.
    pub(crate) fn new(
        context: Rc<VulkanContext>,
        image: vk::Image,
        allocation: Option<Allocation>,
        image_view: VkImageView,
        _format: vk::Format,
    ) -> Self {
        Self {
            context,
            image,
            allocation,
            image_view,
        }
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
                self.context.allocator.borrow_mut().free(allocation).ok();
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

    /// Created transient textures (name -> texture).
    transient_textures: HashMap<String, TransientTexture>,
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
            transient_textures: HashMap::new(),
        }
    }

    /// Add a pass to the graph.
    pub(crate) fn add_pass(&mut self, pass: PassDesc) {
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

        // Update tonemap params for fullscreen passes BEFORE creating frame.
        //
        // Note: This must happen here (not during pass execution) because we need &mut VulkanRenderer
        // to update storage buffers. Once Frame is created, we only have &VulkanRenderer.
        for pass in &self.passes {
            if let Some(ref params) = pass.tonemap_params
                && let Some(hdr_index) = params.hdr_texture_index
            {
                let mode_value = params.mode as u32;
                renderer.storage_manager.update_object_bindless(
                    0,
                    &[
                        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
                        1.0,
                    ],
                    &[
                        params.exposure,
                        params.gamma,
                        mode_value as f32,
                        hdr_index as f32,
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

        let mut frame = Frame::new(self, renderer, image_index);
        f(&mut frame);
        frame.execute_passes()?;

        Ok(())
    }

    /// Get a pass index by name.
    pub(crate) fn pass_index(&self, name: &str) -> Option<usize> {
        self.pass_names.get(name).copied()
    }

    /// Get the number of passes in the graph.
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    /// Get a pass by index.
    pub(crate) fn pass(&self, index: usize) -> Option<&PassDesc> {
        self.passes.get(index)
    }

    /// Get a transient texture by name.
    pub(crate) fn transient_texture(&self, name: &str) -> Option<&TransientTexture> {
        self.transient_textures.get(name)
    }

    /// Initialize transient textures (create Vulkan resources).
    ///
    /// Called internally on first use. Can be called explicitly to pre-initialize
    /// transient textures before frame execution (e.g., for bindless registration).
    pub fn initialize_transient_textures(
        &mut self,
        renderer: &VulkanRenderer,
    ) -> Result<(), RenderGraphError> {
        if !self.transient_textures.is_empty() {
            return Ok(()); // Already initialized
        }

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
                        RenderGraphError::VulkanError(format!("Failed to create image view: {}", e))
                    })?
            };

            self.transient_textures.insert(
                desc.name.clone(),
                TransientTexture::new(
                    renderer.context.clone(),
                    image,
                    Some(allocation),
                    VkImageView::new(image_view),
                    vk_format,
                ),
            );
        }

        Ok(())
    }

    /// Register a transient texture with the bindless texture system.
    ///
    /// This is a convenience method that encapsulates the pattern of:
    /// 1. Looking up a transient texture by name
    /// 2. Registering its image view with the bindless system
    /// 3. Returning the bindless slot index
    ///
    /// # Arguments
    /// * `renderer` - The VulkanRenderer (owns the bindless manager), mutably borrowed
    /// * `name` - Name of the transient texture to register
    ///
    /// # Returns
    /// The bindless texture slot index (u32)
    ///
    /// # Example
    /// ```ignore
    /// // Initialize transient textures first
    /// frame_graph.initialize_transient_textures(&renderer)?;
    ///
    /// // Register HDR texture for tonemapping
    /// let hdr_slot = frame_graph.register_transient_texture_bindless(&mut renderer, "hdr_color")?;
    /// ```
    pub fn register_transient_texture_bindless(
        &self,
        renderer: &mut VulkanRenderer,
        name: &str,
    ) -> Result<u32, RenderGraphError> {
        let texture = self
            .transient_texture(name)
            .ok_or_else(|| RenderGraphError::ResourceNotFound(name.to_string()))?;

        renderer
            .register_bindless_texture(texture.image_view.vk())
            .map_err(|e| {
                RenderGraphError::VulkanError(format!("Failed to register bindless texture: {}", e))
            })
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
        for (name, handle) in self.resources {
            graph.import_resource(name, handle);
        }

        // Store transient resource descriptors
        graph.transient_resources = self.transient_resources;

        // Build passes
        for pass_builder in self.pass_builders {
            let mut resource_map = HashMap::new();
            for read_name in &pass_builder.reads {
                if !resource_map.contains_key(read_name) {
                    resource_map.insert(
                        read_name.clone(),
                        GraphResourceHandle::new(resource_map.len() as u32),
                    );
                }
            }
            for write_name in &pass_builder.writes {
                if !resource_map.contains_key(write_name) {
                    resource_map.insert(
                        write_name.clone(),
                        GraphResourceHandle::new(resource_map.len() as u32),
                    );
                }
            }

            // Call the build function to validate resource references and get pass data
            let pass_data = (pass_builder.build_fn)(&resource_map)?;

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
                    for (name, candidate_handle) in &resource_map {
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
        }
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

        self.pending
            .entry(index)
            .or_default()
            .ui_draw_lists
            .push(ui_draw_list.clone());
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
        let frame_idx = self.renderer.swap_data.current_frame();
        // Clone the command buffer to avoid borrowing issues
        let cmd = self.renderer.frame_context.command_buffers[frame_idx].clone();

        for (index, pass) in self.graph.passes.iter().enumerate() {
            let data = self.pending.remove(&index).unwrap_or_default();

            log::debug!(
                "Executing pass '{}' (index {}): pipeline={:?}, draw_lists={}, writes={:?}",
                pass.name,
                index,
                pass.pipeline,
                data.draw_lists.len(),
                pass.writes
            );

            // Insert pre-pass barriers
            self.insert_barriers(&cmd, index)?;

            // Execute pass based on type
            match pass.pass_type {
                super::pass::PassType::Graphics => {
                    // Check if this is a fullscreen pass (needs mutable renderer access)
                    if pass.pipeline.is_some() && data.draw_lists.is_empty() {
                        if let Some(pipeline) = pass.pipeline {
                            self.execute_fullscreen_pass(&cmd, pass, pipeline)?;
                        }
                    } else {
                        self.execute_graphics_pass(&cmd, pass, data)?;
                    }
                }
            }

            // Track backbuffer state if this pass wrote to it
            if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
                log::debug!("Pass '{}' wrote to backbuffer, tracking state", pass.name);
                self.resource_states.insert(
                    BACKBUFFER_NAME.to_string(),
                    super::resource::ResourceState::ColorAttachment,
                );
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

        let cmd_vk = cmd.vk_command_buffer();
        let device = &self.renderer.context.device;

        // Process writes first (color attachments)
        for write_name in &pass.writes {
            // Skip backbuffer - it's managed by the swapchain
            if write_name == BACKBUFFER_NAME {
                continue;
            }

            // Check if this is a transient texture
            let Some(transient) = self.graph.transient_texture(write_name) else {
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

                // Transition from current state to required
                match current_state {
                    super::resource::ResourceState::Undefined => {
                        ImageBarrier::transition_from_undefined(
                            &cmd_vk,
                            device,
                            transient.image,
                            required_layout,
                        );
                    }
                    super::resource::ResourceState::ShaderRead => {
                        ImageBarrier::transition(
                            &cmd_vk,
                            device,
                            transient.image,
                            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            required_layout,
                        );
                    }
                    _ => {
                        // For other states, transition from undefined (discard contents)
                        ImageBarrier::transition_from_undefined(
                            &cmd_vk,
                            device,
                            transient.image,
                            required_layout,
                        );
                    }
                }

                // Update tracked state
                self.resource_states
                    .insert(write_name.clone(), required_state);
            }
        }

        // Process reads (shader resources)
        for read_name in &pass.reads {
            // Skip backbuffer - not read by shaders
            if read_name == BACKBUFFER_NAME {
                continue;
            }

            // Check if this is a transient texture
            let Some(transient) = self.graph.transient_texture(read_name) else {
                continue;
            };

            let current_state = self
                .resource_states
                .get(read_name)
                .copied()
                .unwrap_or(super::resource::ResourceState::Undefined);

            let required_state = super::resource::ResourceState::ShaderRead;

            if current_state != required_state {
                let required_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;

                // Transition from current state to required
                match current_state {
                    super::resource::ResourceState::Undefined => {
                        ImageBarrier::transition_from_undefined(
                            &cmd_vk,
                            device,
                            transient.image,
                            required_layout,
                        );
                    }
                    super::resource::ResourceState::ColorAttachment => {
                        ImageBarrier::transition(
                            &cmd_vk,
                            device,
                            transient.image,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            required_layout,
                        );
                    }
                    _ => {
                        // For other states, transition from undefined (discard contents)
                        ImageBarrier::transition_from_undefined(
                            &cmd_vk,
                            device,
                            transient.image,
                            required_layout,
                        );
                    }
                }

                // Update tracked state
                self.resource_states
                    .insert(read_name.clone(), required_state);
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
        log::debug!("execute_graphics_pass: beginning render pass");

        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Determine color attachment:
        // 1. If pass writes to "backbuffer", use swapchain directly
        // 2. If pass writes to a transient texture, use that
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
                    "Pass '{}': Using LOAD for backbuffer (previous pass wrote to it)",
                    pass.name
                );
                vk::AttachmentLoadOp::LOAD
            } else {
                log::debug!(
                    "Pass '{}': Using CLEAR for backbuffer (first write)",
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
            if let Some(transient) = self.graph.transient_texture(color_name) {
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

    /// Execute a draw list.
    fn execute_draw_list(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        draw_list: &DrawList,
    ) -> Result<(), RenderGraphError> {
        for draw_call in &draw_list.draws {
            self.execute_draw_call(cmd, draw_call)?;
        }
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
        let frame_idx = self.renderer.storage_manager.current_frame();
        let (vertex_buffer, index_buffer) =
            self.get_or_update_ui_buffers(frame_idx, ui_draw_list)?;

        // Bind vertex and index buffers
        cmd.bind_vertex_buffer(vertex_buffer.0, 0);
        cmd.bind_index_buffer(index_buffer, 0, vk::IndexType::UINT32);

        // Get swapchain extent for scissor (physical pixels)
        let extent = self.renderer.frame_context.swapchain.get_extent();

        // Get font atlas handle for fallback when texture is NONE
        let font_atlas_handle = self
            .renderer
            .ui_renderer
            .font_atlas_handle()
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(
                    "UI font atlas not initialized".to_string(),
                )
            })?;

        // Bind UI descriptor sets (font atlas, sampler, uniforms)
        // Use screen_size from draw list (logical pixels, matches vertex coordinates)
        // Bind set 0 once (font atlas, uniforms don't change per frame)
        self.bind_ui_descriptor_sets(
            cmd,
            pipeline_handle,
            pipeline_layout,
            ui_draw_list.screen_size,
        )?;

        // Track current texture to avoid redundant descriptor updates
        let mut current_texture = None;  // Use None to indicate no texture has been bound yet

        // Execute each draw command with scissor clipping and dynamic texture binding
        for draw_cmd in &ui_draw_list.commands {
            // Update dynamic texture (set 1) if it changed or this is the first draw
            if current_texture.is_none() || draw_cmd.texture != current_texture.unwrap() {
                let texture_handle = if draw_cmd.texture == TextureHandle::NONE {
                    font_atlas_handle // Fall back to font atlas for solid color rendering
                } else {
                    draw_cmd.texture
                };

                self.push_ui_dynamic_texture(
                    cmd,
                    pipeline_layout,
                    texture_handle,
                )?;
                current_texture = Some(draw_cmd.texture);
            }

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

    /// Get or create per-frame UI vertex and index buffers, updating them with new data.
    ///
    /// This reuses buffers across frames to avoid memory leaks. Buffers are resized
    /// if needed to accommodate larger data.
    fn get_or_update_ui_buffers(
        &mut self,
        frame_idx: usize,
        ui_draw_list: &crate::renderer::types::UIDrawList,
    ) -> Result<((vk::Buffer, u32), vk::Buffer), RenderGraphError> {
        use crate::vulkan::{IndexBuffer, IndexType, VertexBuffer};

        let vertex_bytes = bytemuck::cast_slice(&ui_draw_list.vertices);
        let index_bytes = bytemuck::cast_slice(&ui_draw_list.indices);

        // Access UI resources through UIRenderer
        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

        // Ensure we have storage for this frame (grow as needed)
        while ui_resources.vertex_buffers.len() <= frame_idx {
            ui_resources.vertex_buffers.push(None);
            ui_resources.index_buffers.push(None);
        }

        // Get or create vertex buffer
        let vb_handle = if let Some(ref mut vb) = ui_resources.vertex_buffers[frame_idx] {
            vb.upload_data(vertex_bytes);
            (vb.object(), vb.count())
        } else {
            let mut vb = VertexBuffer::new(
                self.renderer.context.clone(),
                1024 * 1024, // 1MB initial size
                65536,
            );
            vb.upload_data(vertex_bytes);
            ui_resources.vertex_buffers[frame_idx] = Some(vb);
            let vb_ref = ui_resources.vertex_buffers[frame_idx].as_ref().unwrap();
            (vb_ref.object(), vb_ref.count())
        };

        // Get or create index buffer
        let ib_handle = if let Some(ref mut ib) = ui_resources.index_buffers[frame_idx] {
            ib.upload_data(index_bytes);
            ib.object()
        } else {
            let mut ib = IndexBuffer::new(
                self.renderer.context.clone(),
                1024 * 1024,
                IndexType::Uint32,
                65536,
            );
            ib.upload_data(index_bytes);
            ui_resources.index_buffers[frame_idx] = Some(ib);
            ui_resources.index_buffers[frame_idx]
                .as_ref()
                .unwrap()
                .object()
        };

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
        let frame_idx = self.renderer.storage_manager.current_frame();
        let descriptor_set =
            self.get_or_create_ui_descriptor_set(frame_idx, descriptor_set_layout, screen_size)?;

        // Bind descriptor set 0 (font atlas, sampler, uniforms)
        cmd.bind_descriptor_sets(pipeline_layout, 0, &[descriptor_set], &[]);

        Ok(())
    }

    /// Push a dynamic texture to descriptor set 1 for UI rendering.
    /// This is called per draw command to bind the correct texture (font atlas or viewport).
    fn push_ui_dynamic_texture(
        &self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        texture_handle: TextureHandle,
    ) -> Result<(), RenderGraphError> {
        let texture = self
            .renderer
            .texture_manager
            .get_texture_rc(texture_handle)
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(format!(
                    "Texture not found for handle {:?}",
                    texture_handle
                ))
            })?;

        let image_info = vk::DescriptorImageInfo::default()
            .sampler(texture.image_sampler.vk())
            .image_view(texture.image_view.vk())
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        let push_write = vk::WriteDescriptorSet::default()
            .dst_set(vk::DescriptorSet::null()) // Ignored for push descriptors
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(std::slice::from_ref(&image_info));

        if let Some(push_descriptor_khr) = &self.renderer.context.push_descriptor_khr {
            cmd.push_descriptor_set_khr(push_descriptor_khr, pipeline_layout, 1, &[push_write]);
        }

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

    /// Update UI descriptor set with font atlas, sampler, and uniforms.
    fn update_ui_descriptor_set(
        &mut self,
        descriptor_set: vk::DescriptorSet,
        screen_size: [f32; 2],
    ) -> Result<(), RenderGraphError> {
        // Get font atlas texture
        let font_atlas_handle = self
            .renderer
            .ui_renderer
            .font_atlas_handle()
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration("UI font atlas not initialized".to_string())
            })?;

        let font_texture = self
            .renderer
            .texture_manager
            .get_texture_rc(font_atlas_handle)
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration("Font atlas texture not found".to_string())
            })?;

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
            .sampler(font_texture.image_sampler.vk())
            .image_view(font_texture.image_view.vk())
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        let writes = [
            // Binding 0: font atlas texture
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(std::slice::from_ref(&image_info)),
            // Binding 1: sampler
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(std::slice::from_ref(&image_info)),
            // Binding 3: screen size uniform
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
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
        let skeleton = draw_call.skeleton;

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
        let storage_ds = self.renderer.storage_descriptor_sets
            [self.renderer.storage_manager.current_frame()]
        .vk_set();
        cmd.bind_descriptor_sets(pipeline_layout, 0, &[storage_ds], &[]);

        // Set 1: Bindless textures (all current materials use bindless)
        let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(pipeline_layout, 1, &[bindless_ds], &[]);

        // Set 2: Skeleton joint matrices (only when draw_call has skeleton)
        if let Some(skeleton_handle) = draw_call.skeleton {
            let skeleton_ds = self
                .renderer
                .get_skeleton_descriptor(skeleton_handle)
                .ok_or(RenderGraphError::InvalidSkeletonHandle(skeleton_handle))?;
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
        log::debug!("execute_fullscreen_pass: beginning render pass");

        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Determine color attachment:
        // 1. If pass writes to "backbuffer", use swapchain directly
        // 2. If pass writes to a transient texture, use that
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
            // Check if this is a transient texture
            if let Some(transient) = self.graph.transient_texture(color_name) {
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
        let storage_ds = self.renderer.storage_descriptor_sets
            [self.renderer.storage_manager.current_frame()]
        .vk_set();
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
