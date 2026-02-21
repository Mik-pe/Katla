use ash::vk;

use crate::render_graph::renderer_context::RendererContext;
use crate::resource::CompiledResource;
use crate::sync::{VkBuffer, VkFramebuffer, VkImage, VkImageView};
use crate::types::{ClearValue, Extent2D, PipelineBindPoint};
use crate::{CommandBuffer, ResourceId, ResourceUsage};
use std::collections::HashMap;
use std::rc::Rc;

/// Category of a render pass for filtering and categorization.
/// Used to replace magic string comparisons like `pass.name == "ui_pass"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PassCategory {
    /// Scene rendering passes (geometry, sky, particles, etc.)
    #[default]
    Scene,
    /// UI overlay passes
    Ui,
    /// Presentation/copy passes (blit to swapchain)
    Present,
    /// Compute dispatches
    Compute,
    /// Transfer/copy operations
    Transfer,
}

impl PassCategory {
    /// Auto-detect category from pass name for backward compatibility.
    /// Uses naming conventions:
    /// - "ui_pass" -> Ui
    /// - "present_pass" -> Present
    /// - Names starting with "compute_" -> Compute
    /// - Names starting with "copy_", "transfer_", "blit_" -> Transfer
    /// - Everything else -> Scene
    pub fn from_name(name: &str) -> Self {
        match name {
            "ui_pass" => PassCategory::Ui,
            "present_pass" => PassCategory::Present,
            n if n.starts_with("compute_") => PassCategory::Compute,
            n if n.starts_with("copy_") || n.starts_with("transfer_") || n.starts_with("blit_") => {
                PassCategory::Transfer
            }
            _ => PassCategory::Scene,
        }
    }
}

/// Describes the type of attachment when writing to a resource.
/// This enum allows the render graph to correctly configure the attachment
/// for either color or depth-stencil output.
pub enum Attachment {
    /// Color attachment output (e.g., swapchain image, offscreen render target)
    Color(ResourceId),
    /// Depth-stencil attachment output (e.g., depth buffer for depth testing)
    DepthStencil(ResourceId),
}

/// Type alias for execute closures to avoid repeating complex trait object syntax.
/// This approach isolates the trait object definition to help Rust's type inference.
/// ExecutionRegistry stores pass execution closures by pass name.
/// This approach avoids trait object lifetime issues by storing closures
/// separately from pass metadata in a HashMap.
pub struct ExecutionRegistry<'a> {
    #[allow(clippy::type_complexity)]
    closures: HashMap<String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'a>>,
}

impl<'a> ExecutionRegistry<'a> {
    pub fn new() -> Self {
        Self {
            closures: HashMap::new(),
        }
    }

    /// Register a closure for a pass with the given name.
    pub fn register<F>(&mut self, name: String, f: F)
    where
        F: FnMut(Rc<PassExecutionContext>) + 'static,
    {
        self.closures.insert(name, Box::new(f));
    }

    /// Execute the closure for a pass with the given name.
    pub fn execute(&mut self, name: &str, ctx: Rc<PassExecutionContext>) {
        if let Some(closure) = self.closures.get_mut(name) {
            closure(ctx);
        }
    }

    /// Get the number of registered closures.
    pub fn len(&self) -> usize {
        self.closures.len()
    }

    /// Check if the registry has any registered closures.
    pub fn is_empty(&self) -> bool {
        self.closures.is_empty()
    }
}

impl<'a> Default for ExecutionRegistry<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// PassExecute wraps a closure that executes a pass.
/// To avoid trait object lifetime issues with generic types, we use a name-based lookup.
/// The closure is stored in an ExecutionRegistry and looked up by pass name at runtime.
pub struct PassExecute {
    pass_name: String,
}

impl PassExecute {
    pub fn new(pass_name: String) -> Self {
        Self { pass_name }
    }

    pub fn pass_name(&self) -> &str {
        &self.pass_name
    }

    /// Execute this pass using the provided ExecutionRegistry.
    pub fn execute(&self, ctx: Rc<PassExecutionContext>, registry: &mut ExecutionRegistry) {
        let name = self.pass_name();
        if !name.is_empty() {
            registry.execute(name, ctx);
        }
    }
}

/// PassBuilder provides an ergonomic fluent API for constructing render passes.
/// It allows you to define inputs, outputs, clear values, and execution logic
/// for a pass in a declarative way.
pub struct PassBuilder {
    name: String,
    inputs: Vec<ResourceId>,
    outputs: Vec<ResourceId>,
    usages: Vec<ResourceUsage>,
    bind_point: PipelineBindPoint,
    extent: Option<Extent2D>,
    category: PassCategory,
    execute: Option<PassExecute>,
    #[allow(clippy::type_complexity)]
    pending_execute: Option<(String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'static>)>,
    /// Pre-execute callback runs BEFORE begin_rendering() for custom barrier setup.
    /// This is needed because pipeline barriers with image transitions cannot be called
    /// inside a render pass (VUID-vkCmdPipelineBarrier2-None-09553).
    #[allow(clippy::type_complexity)]
    pending_pre_execute: Option<(String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'static>)>,
    pre_execute_name: Option<String>,
}

impl PassBuilder {
    /// Create a new pass builder with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let category = PassCategory::from_name(&name);
        Self {
            name,
            inputs: Vec::new(),
            outputs: Vec::new(),
            usages: Vec::new(),
            bind_point: PipelineBindPoint::Graphics,
            extent: None,
            category,
            execute: None,
            pending_execute: None,
            pending_pre_execute: None,
            pre_execute_name: None,
        }
    }

    /// Set the pass category explicitly.
    /// If not called, category is auto-detected from the pass name.
    pub fn category(mut self, category: PassCategory) -> Self {
        self.category = category;
        self
    }

    /// Mark a resource as a read input for this pass.
    /// This adds the resource to the inputs list and creates appropriate usage information.
    pub fn read(&mut self, resource_id: ResourceId) -> &mut Self {
        self.inputs.push(resource_id);

        let usage = ResourceUsage::new(resource_id)
            .with_read(
                crate::types::Access::ShaderRead,
                crate::types::PipelineStage::FragmentShader,
            )
            .with_layout(crate::types::ImageLayout::ShaderReadOnlyOptimal);
        self.usages.push(usage);
        self
    }

    /// Mark a resource as a write output for this pass.
    /// This adds the resource to the outputs list and creates appropriate usage information
    /// based on the attachment type (color or depth-stencil).
    pub fn write(&mut self, attachment: Attachment) -> &mut Self {
        match attachment {
            Attachment::Color(resource_id) => {
                self.outputs.push(resource_id);

                let usage = ResourceUsage::new(resource_id)
                    .with_write(
                        crate::types::Access::ColorAttachmentWrite,
                        crate::types::PipelineStage::ColorAttachmentOutput,
                    )
                    .with_load_op(crate::types::AttachmentLoadOp::Load)
                    .with_store_op(crate::types::AttachmentStoreOp::Store)
                    .with_layout(crate::types::ImageLayout::ColorAttachmentOptimal);
                self.usages.push(usage);
            }
            Attachment::DepthStencil(resource_id) => {
                self.outputs.push(resource_id);

                let usage = ResourceUsage::new(resource_id)
                    .with_write(
                        crate::types::Access::DepthStencilAttachmentWrite,
                        crate::types::PipelineStage::EarlyFragmentTests,
                    )
                    .with_load_op(crate::types::AttachmentLoadOp::Load)
                    .with_store_op(crate::types::AttachmentStoreOp::Store)
                    .with_layout(crate::types::ImageLayout::DepthStencilAttachmentOptimal);
                self.usages.push(usage);
            }
        }
        self
    }

    /// Mark a resource as transfer read for copy/blit operations.
    /// This is for vkCmdBlitImage/vkCmdCopyImage source images.
    pub fn read_transfer(&mut self, resource_id: ResourceId) -> &mut Self {
        self.inputs.push(resource_id);

        let usage = ResourceUsage::new(resource_id)
            .with_read(
                crate::types::Access::TransferRead,
                crate::types::PipelineStage::Transfer,
            )
            .with_layout(crate::types::ImageLayout::TransferSrcOptimal);
        self.usages.push(usage);
        self
    }

    /// Mark a resource as transfer write for copy/blit operations.
    /// This is for vkCmdBlitImage/vkCmdCopyImage destination images.
    pub fn write_transfer(&mut self, resource_id: ResourceId) -> &mut Self {
        self.outputs.push(resource_id);

        let usage = ResourceUsage::new(resource_id)
            .with_write(
                crate::types::Access::TransferWrite,
                crate::types::PipelineStage::Transfer,
            )
            .with_layout(crate::types::ImageLayout::TransferDstOptimal);
        self.usages.push(usage);
        self
    }

    /// Mark a buffer resource as storage read for compute passes.
    ///
    /// This is for compute shader reads from storage buffers (SSBOs).
    pub fn read_storage(&mut self, resource_id: ResourceId) -> &mut Self {
        self.inputs.push(resource_id);

        let usage = ResourceUsage::new(resource_id).with_read(
            crate::types::Access::ShaderRead,
            crate::types::PipelineStage::ComputeShader,
        );
        self.usages.push(usage);
        self
    }

    /// Mark a buffer resource as storage write for compute passes.
    ///
    /// This is for compute shader writes to storage buffers (SSBOs).
    pub fn write_storage(&mut self, resource_id: ResourceId) -> &mut Self {
        self.outputs.push(resource_id);

        let usage = ResourceUsage::new(resource_id).with_write(
            crate::types::Access::ShaderWrite,
            crate::types::PipelineStage::ComputeShader,
        );
        self.usages.push(usage);
        self
    }

    /// Mark a buffer resource as read_write storage for compute passes.
    ///
    /// This is for compute shader read-write access to storage buffers (SSBOs).
    /// Modern GPUs handle single-buffer read_write well (Unreal Niagara, Unity VFX Graph).
    pub fn read_write_storage(&mut self, resource_id: ResourceId) -> &mut Self {
        self.inputs.push(resource_id);
        self.outputs.push(resource_id);

        // Combined read/write usage for compute
        let usage = ResourceUsage::new(resource_id)
            .with_read(
                crate::types::Access::ShaderRead,
                crate::types::PipelineStage::ComputeShader,
            )
            .with_write(
                crate::types::Access::ShaderWrite,
                crate::types::PipelineStage::ComputeShader,
            );
        self.usages.push(usage);
        self
    }

    /// Specify a clear color value for a color attachment.
    /// This should be called after write() for the resource you want to clear.
    pub fn clear_color(&mut self, resource_id: ResourceId, color: [f32; 4]) -> &mut Self {
        if let Some(usage) = self
            .usages
            .iter_mut()
            .find(|u| u.resource_id == resource_id)
        {
            usage.clear_value = Some(ClearValue::color(color[0], color[1], color[2], color[3]));
            usage.load_op = crate::types::AttachmentLoadOp::Clear.into();
        }
        self
    }

    /// Specify clear values for depth and stencil attachments.
    /// This should be called after write() for the depth/stencil resource you want to clear.
    pub fn clear_depth_stencil(
        &mut self,
        resource_id: ResourceId,
        depth: f32,
        stencil: u32,
    ) -> &mut Self {
        if let Some(usage) = self
            .usages
            .iter_mut()
            .find(|u| u.resource_id == resource_id)
        {
            usage.clear_value = Some(ClearValue::depth(depth, stencil));
            usage.load_op = crate::types::AttachmentLoadOp::Clear.into();
            // Depth attachments typically don't need to store, so use DONT_CARE
            usage.store_op = crate::types::AttachmentStoreOp::DontCare.into();
        }
        self
    }

    // ========================================================================
    // Convenience Methods for RenderTarget (high-level API)
    // These methods accept &RenderTarget instead of raw ResourceId
    // ========================================================================

    /// Write to a color attachment using a RenderTarget.
    ///
    /// This is a convenience method that wraps `write(Attachment::Color(...))`.
    pub fn write_color(&mut self, target: &super::frame_resources::RenderTarget) -> &mut Self {
        self.write(Attachment::Color(target.resource_id()))
    }

    /// Write to a depth attachment using a RenderTarget.
    ///
    /// This is a convenience method that wraps `write(Attachment::DepthStencil(...))`.
    pub fn write_depth(&mut self, target: &super::frame_resources::RenderTarget) -> &mut Self {
        self.write(Attachment::DepthStencil(target.resource_id()))
    }

    /// Clear a color attachment using a RenderTarget.
    ///
    /// This should be called after `write_color()` for the target you want to clear.
    pub fn clear_color_target(
        &mut self,
        target: &super::frame_resources::RenderTarget,
        color: [f32; 4],
    ) -> &mut Self {
        self.clear_color(target.resource_id(), color)
    }

    /// Clear a depth attachment using a RenderTarget.
    ///
    /// This should be called after `write_depth()` for the target you want to clear.
    pub fn clear_depth_target(
        &mut self,
        target: &super::frame_resources::RenderTarget,
        depth: f32,
    ) -> &mut Self {
        self.clear_depth_stencil(target.resource_id(), depth, 0)
    }

    /// Set up a blit operation from one target to another.
    ///
    /// This configures the pass for a transfer blit (commonly used for present passes).
    /// The source is read as transfer source, the destination is written as transfer destination.
    pub fn blit(
        &mut self,
        src: &super::frame_resources::RenderTarget,
        dst: &super::frame_resources::RenderTarget,
    ) -> &mut Self {
        self.read_transfer(src.resource_id())
            .write_transfer(dst.resource_id())
    }

    /// Set the extent (dimensions) for this pass.
    /// If not set, it will be derived from the resources used in the pass.
    pub fn extent(&mut self, width: u32, height: u32) -> &mut Self {
        self.extent = Some(Extent2D::new(width, height));
        self
    }

    /// Set the pipeline bind point (graphics or compute) for this pass.
    pub fn bind_point(&mut self, bind_point: PipelineBindPoint) -> &mut Self {
        self.bind_point = bind_point;
        self
    }

    /// Specify the execution logic for this pass.
    /// The provided closure will be stored temporarily and registered
    /// with the ExecutionRegistry during graph building.
    /// Pass name is used to look up the closure during execution.
    pub fn execute<F>(&mut self, name: impl Into<String>, f: F) -> &mut Self
    where
        F: FnMut(Rc<PassExecutionContext>) + 'static,
    {
        let name = name.into();
        self.pending_execute = Some((name.clone(), Box::new(f)));
        self.execute = Some(PassExecute::new(name));
        self
    }

    /// Specify pre-execution logic that runs BEFORE begin_rendering().
    /// Use this for custom pipeline barriers that need to happen outside a render pass.
    /// Pipeline barriers with image layout transitions cannot be called inside a render pass
    /// (VUID-vkCmdPipelineBarrier2-None-09553).
    pub fn pre_execute<F>(&mut self, name: impl Into<String>, f: F) -> &mut Self
    where
        F: FnMut(Rc<PassExecutionContext>) + 'static,
    {
        let name = name.into();
        self.pending_pre_execute = Some((format!("pre_{}", name), Box::new(f)));
        self.pre_execute_name = Some(format!("pre_{}", name));
        self
    }

    /// Build the pass from the builder configuration.
    pub fn build(self) -> Pass {
        Pass::from_builder(self)
    }
}

/// PassExecutionContext provides all the information needed during pass execution.
/// This is passed to the execute closure defined in the PassBuilder.
pub struct PassExecutionContext {
    /// Command buffer to record commands into
    pub command_buffer: std::rc::Rc<CommandBuffer>,
    /// Compiled resources available for this pass (wrapped in RefCell for per-frame updates)
    pub resources:
        std::rc::Rc<std::cell::RefCell<std::collections::HashMap<ResourceId, CompiledResource>>>,
    /// The framebuffer for this pass (legacy render pass only)
    pub framebuffer: VkFramebuffer,
    /// The current subpass index (0 for simple passes)
    pub subpass: u32,
    /// The render extent (width and height)
    pub extent: Extent2D,
    /// Whether this pass uses dynamic rendering (Vulkan 1.3)
    pub uses_dynamic_rendering: bool,
    /// Optional renderer context for accessing renderer state (eliminates unsafe pointers)
    renderer_context: Option<Rc<RendererContext>>,
}

impl PassExecutionContext {
    /// Create a new PassExecutionContext.
    pub fn new(
        command_buffer: CommandBuffer,
        resources: std::rc::Rc<
            std::cell::RefCell<std::collections::HashMap<ResourceId, CompiledResource>>,
        >,
        framebuffer: vk::Framebuffer,
        extent: Extent2D,
    ) -> Self {
        Self {
            command_buffer: std::rc::Rc::new(command_buffer),
            resources,
            framebuffer: VkFramebuffer::new(framebuffer),
            subpass: 0,
            extent,
            uses_dynamic_rendering: false,
            renderer_context: None,
        }
    }

    /// Create a new PassExecutionContext for dynamic rendering (Vulkan 1.3).
    pub fn new_dynamic(
        command_buffer: CommandBuffer,
        resources: std::rc::Rc<
            std::cell::RefCell<std::collections::HashMap<ResourceId, CompiledResource>>,
        >,
        extent: Extent2D,
    ) -> Self {
        Self {
            command_buffer: std::rc::Rc::new(command_buffer),
            resources,
            framebuffer: VkFramebuffer::new(vk::Framebuffer::null()),
            subpass: 0,
            extent,
            uses_dynamic_rendering: true,
            renderer_context: None,
        }
    }

    /// Create a new PassExecutionContext with renderer context.
    pub fn with_renderer_context(
        command_buffer: CommandBuffer,
        resources: std::rc::Rc<
            std::cell::RefCell<std::collections::HashMap<ResourceId, CompiledResource>>,
        >,
        extent: Extent2D,
        renderer_context: Rc<RendererContext>,
    ) -> Self {
        Self {
            command_buffer: std::rc::Rc::new(command_buffer),
            resources,
            framebuffer: VkFramebuffer::new(vk::Framebuffer::null()),
            subpass: 0,
            extent,
            uses_dynamic_rendering: true,
            renderer_context: Some(renderer_context),
        }
    }

    /// Set the renderer context after creation.
    pub fn set_renderer_context(&mut self, ctx: Rc<RendererContext>) {
        self.renderer_context = Some(ctx);
    }

    /// Get the renderer context for accessing renderer state safely.
    pub fn renderer_context(&self) -> Option<&RendererContext> {
        self.renderer_context.as_deref()
    }

    /// Get a compiled image resource by ID (works for both Image and ExternalImage)
    /// Returns wrapper types for the image and image view.
    pub fn get_image(&self, resource_id: ResourceId) -> Option<(VkImage, VkImageView)> {
        match self.resources.borrow().get(&resource_id) {
            Some(CompiledResource::Image {
                image, image_view, ..
            }) => Some((*image, *image_view)),
            Some(CompiledResource::ExternalImage {
                image, image_view, ..
            }) => Some((*image, *image_view)),
            _ => None,
        }
    }

    /// Get a compiled buffer resource by ID (works for both Buffer and ExternalBuffer)
    /// Returns wrapper type for the buffer.
    pub fn get_buffer(&self, resource_id: ResourceId) -> Option<VkBuffer> {
        match self.resources.borrow().get(&resource_id) {
            Some(CompiledResource::Buffer { buffer, .. }) => Some(*buffer),
            Some(CompiledResource::ExternalBuffer { buffer }) => Some(*buffer),
            _ => None,
        }
    }

    // ========================================================================
    // Convenience Drawing Methods
    // These wrap common command buffer operations for cleaner pass execution
    // ========================================================================

    /// Bind a graphics pipeline for rendering.
    ///
    /// This is a convenience method that wraps command_buffer.bind_graphics_pipeline().
    pub fn bind_graphics_pipeline(&self, pipeline: &crate::vulkan::material::MaterialPipeline) {
        self.command_buffer.bind_graphics_pipeline(pipeline);
    }

    /// Bind a graphics pipeline with its primary descriptor set.
    ///
    /// This combines pipeline binding and descriptor binding in one call,
    /// which is the most common pattern.
    pub fn bind_graphics_pipeline_with_descriptors(
        &self,
        pipeline: &crate::vulkan::material::MaterialPipeline,
        descriptor_set: crate::sync::VkDescriptorSet,
    ) {
        self.command_buffer
            .bind_graphics_pipeline_with_descriptors(pipeline, descriptor_set.into());
    }

    /// Bind an index buffer for indexed drawing.
    ///
    /// Uses the IndexType wrapper instead of raw vk::IndexType.
    pub fn bind_index_buffer(
        &self,
        buffer: VkBuffer,
        offset: u64,
        index_type: crate::IndexType,
    ) {
        self.command_buffer
            .bind_index_buffer(buffer.into(), offset, index_type);
    }

    /// Bind vertex buffers for rendering.
    ///
    /// Uses VkBuffer wrapper instead of raw vk::Buffer.
    pub fn bind_vertex_buffers(&self, first_binding: u32, buffers: &[VkBuffer], offsets: &[u64]) {
        let raw_buffers: Vec<vk::Buffer> = buffers.iter().map(|b| (*b).into()).collect();
        self.command_buffer
            .bind_vertex_buffers(first_binding, &raw_buffers, offsets);
    }

    /// Draw indexed geometry.
    ///
    /// This is the primary draw call for mesh rendering with an index buffer.
    pub fn draw_indexed(
        &self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        self.command_buffer
            .draw_indexed(index_count, instance_count, first_index, vertex_offset, first_instance);
    }

    /// Draw geometry without an index buffer (array drawing).
    ///
    /// This is used for fullscreen passes (sky, grid, post-processing).
    pub fn draw_array(
        &self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        self.command_buffer
            .draw_array(vertex_count, instance_count, first_vertex, first_instance);
    }

    /// Draw a fullscreen triangle (3 vertices, 1 instance).
    ///
    /// Convenience method for sky, grid, and post-processing passes.
    pub fn draw_fullscreen(&self) {
        self.draw_array(3, 1, 0, 0);
    }

    /// Get the render extent for this pass.
    ///
    /// Returns (width, height) for viewport/scissor setup.
    pub fn extent(&self) -> (u32, u32) {
        (self.extent.width, self.extent.height)
    }

    /// Blit from one image to another.
    ///
    /// This is a convenience method for the present pass.
    /// Performs a linear-filtered blit from src to dst covering the full images.
    pub fn blit_images(
        &self,
        src_image: VkImage,
        dst_image: VkImage,
        width: u32,
        height: u32,
    ) {
        let src: vk::Image = src_image.into();
        let dst: vk::Image = dst_image.into();

        let blit_region = vk::ImageBlit::default()
            .src_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D { x: width as i32, y: height as i32, z: 1 },
            ])
            .dst_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .dst_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D { x: width as i32, y: height as i32, z: 1 },
            ]);

        // Get device from renderer context
        if let Some(ctx) = &self.renderer_context {
            if let Some(device) = ctx.vk_device() {
                unsafe {
                    device.cmd_blit_image(
                        self.command_buffer.vk_command_buffer(),
                        src,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        dst,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[blit_region],
                        vk::Filter::LINEAR,
                    );
                }
            }
        }
    }

    // ========================================================================
    // Descriptor Set Binding Methods (no ash::vk exposure)
    // ========================================================================

    /// Bind a descriptor set at set index 0 for graphics pipelines.
    ///
    /// This is the most common pattern - bind a single descriptor set at set 0.
    pub fn bind_graphics_descriptor_set(
        &self,
        pipeline: &crate::vulkan::material::MaterialPipeline,
        descriptor_set: crate::sync::VkDescriptorSet,
    ) {
        self.command_buffer.bind_graphics_descriptors(
            pipeline.vk_layout(),
            &[descriptor_set.into()],
        );
    }

    /// Bind a descriptor set at a specific set index for graphics pipelines.
    ///
    /// This allows binding to set 1, 2, etc. for pipelines with multiple descriptor sets.
    /// Use this for bindless textures (set 1), skeleton data (set 2), etc.
    pub fn bind_graphics_descriptor_set_at(
        &self,
        pipeline: &crate::vulkan::material::MaterialPipeline,
        descriptor_set: crate::sync::VkDescriptorSet,
        first_set: u32,
    ) {
        self.command_buffer.bind_graphics_descriptors_at(
            pipeline.vk_layout(),
            first_set,
            &[descriptor_set.into()],
        );
    }

    // ========================================================================
    // High-Level Drawing API - Application-friendly methods
    // ========================================================================

    /// Draw all meshes from the draw list.
    ///
    /// This is a generic drawing method - it draws whatever is in the draw list.
    /// The application must have set a draw list via `renderer.set_draw_list()`.
    /// Katla_vulkan doesn't know or care what "geometry" means - it just draws
    /// the meshes it's given.
    ///
    /// Handles all the complexity internally:
    /// - Binding pipelines and descriptor sets
    /// - Updating storage uniforms
    /// - Binding vertex/index buffers
    /// - Issuing draw calls
    pub fn draw_draw_list(&self) {
        let Some(ctx) = &self.renderer_context else {
            return;
        };

        // Get draw list
        let draw_list = match &ctx.draw_list {
            Some(cell) => cell.borrow_mut().take(),
            None => return,
        };
        let Some(draw_list) = draw_list else {
            return;
        };

        // Get asset registry via pointer accessor
        let Some(registry) = ctx.asset_registry() else {
            return;
        };

        // Get storage descriptor set
        let storage_descriptor = ctx.storage_descriptor();

        // Get bindless manager via pointer accessor
        let bindless_descriptor = ctx.bindless_manager()
            .and_then(|bm| bm.as_ref().map(|m| m.vk_descriptor_set()));

        // Get skeleton descriptors via pointer accessor
        let Some(skeleton_descriptors) = ctx.skeleton_descriptors() else {
            return;
        };

        let mut next_object_index: u32 = 0;
        let mut bindless_bound = false;

        for draw in &draw_list.draws {
            let instance_count = draw.instance_count();
            let first_instance = next_object_index;
            next_object_index += instance_count;

            // Update uniforms first - get storage_manager fresh for each iteration
            if let Some(material) = registry.get_material(draw.material) {
                if let Some(storage_manager_opt) = ctx.storage_manager() {
                    if let Some(manager) = storage_manager_opt.as_mut() {
                        let model: [[f32; 4]; 4] = bytemuck::cast(draw.model_matrix);
                        let color = draw.color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                        manager.update_object_bindless(
                            first_instance as usize,
                            &model,
                            &color,
                            draw.metallic,
                            draw.roughness,
                            draw.ao,
                            material.emission_index as f32,
                            material.texture_indices,
                        );
                    }
                }
            }

            // Get mesh and material for drawing
            let mesh = registry.get_mesh(draw.mesh);
            let material = registry.get_material(draw.material);

            if let (Some(mesh), Some(material)) = (mesh, material) {
                let pipeline_ref = material.pipeline.borrow();
                self.command_buffer.bind_graphics_pipeline(&pipeline_ref);

                // Bind storage descriptor (set 0)
                if let Some(desc_set) = storage_descriptor {
                    self.command_buffer.bind_graphics_descriptors(
                        pipeline_ref.vk_layout(),
                        &[desc_set.into()],
                    );
                }

                // Bind bindless textures (set 1) - only once
                if let Some(bindless_set) = bindless_descriptor {
                    if !bindless_bound {
                        self.command_buffer.bind_graphics_descriptors_at(
                            pipeline_ref.vk_layout(),
                            1,
                            &[bindless_set],
                        );
                        bindless_bound = true;
                    }
                }

                // Bind skeleton descriptor (set 2) if present
                if let Some(skeleton_handle) = draw.skeleton {
                    if let Some(Some(skeleton_desc)) = skeleton_descriptors.get(skeleton_handle.0 as usize) {
                        self.command_buffer.bind_graphics_descriptors_at(
                            pipeline_ref.vk_layout(),
                            2,
                            &[skeleton_desc.vk_set()],
                        );
                    }
                }

                drop(pipeline_ref);

                // Draw
                if let Some(ref ib) = mesh.index_buffer {
                    self.command_buffer.bind_index_buffer(ib.object(), 0, ib.index_type);
                    if let Some(ref vb) = mesh.vertex_buffer {
                        self.command_buffer.bind_vertex_buffers(0, &[vb.object()], &[0]);
                        self.command_buffer.draw_indexed(ib.count(), instance_count, 0, 0, first_instance);
                    }
                }
            }
        }
    }

    /// Draw UI from the renderer's UI data.
    ///
    /// This handles all UI rendering internally:
    /// - Binding the UI pipeline
    /// - Binding font atlas textures
    /// - Updating vertex/index buffers
    /// - Drawing indexed geometry
    ///
    /// The application just needs to have set UI data via `renderer.set_ui_data()`.
    pub fn draw_ui(&self, ui_pipeline: &std::rc::Rc<std::cell::RefCell<crate::vulkan::material::MaterialPipeline>>) {
        let Some(ctx) = &self.renderer_context else {
            return;
        };

        // Get UI data via pointer accessor
        let Some(ui_data_cell) = ctx.ui_data() else {
            return;
        };
        let ui_data = ui_data_cell.borrow();
        let Some(ui_data) = ui_data.as_ref() else {
            return;
        };

        // Get UI textures via pointer accessor
        let Some(ui_textures_opt) = ctx.ui_textures() else {
            return;
        };
        let Some(ui_textures) = ui_textures_opt.as_ref() else {
            return;
        };

        if ui_data.vertex_data.is_empty() || ui_data.index_data.is_empty() {
            return;
        }

        // Get frame index via pointer accessor
        let Some(frame_idx) = ctx.ui_frame_index() else {
            return;
        };

        // Get UI buffers via pointer accessor
        let Some(ui_buffers) = ctx.ui_buffers() else {
            return;
        };

        // Bind pipeline
        let pipeline = ui_pipeline.borrow();
        self.bind_graphics_pipeline(&pipeline);

        // Bind UI descriptor set (font atlas)
        self.command_buffer.bind_graphics_descriptors(
            pipeline.vk_layout(),
            &[ui_textures.vk_set()],
        );
        drop(pipeline);

        // Update and draw
        if let Some(ui_buffer) = ui_buffers.get(frame_idx) {
            ui_buffer.update_vertices(&ui_data.vertex_data);
            ui_buffer.update_indices(&ui_data.index_data);

            self.command_buffer.bind_vertex_buffers(0, &[ui_buffer.vertex_buffer], &[0]);
            self.command_buffer.bind_index_buffer(ui_buffer.index_buffer, 0, crate::IndexType::Uint32);

            self.command_buffer.draw_indexed(
                (ui_data.index_data.len() / 4) as u32,
                1, 0, 0, 0,
            );
        }
    }

    /// Draw a fullscreen quad with a material.
    ///
    /// This is the simplest drawing method - just renders a fullscreen triangle
    /// using the given material. Commonly used for sky, grid, and post-processing passes.
    ///
    /// The descriptor set binding is handled automatically if the RendererContext
    /// has a storage descriptor set available.
    pub fn draw_fullscreen_with_material(&self, material: &std::rc::Rc<std::cell::RefCell<crate::vulkan::material::MaterialPipeline>>) {
        let pipeline = material.borrow();
        self.bind_graphics_pipeline(&pipeline);

        // Bind storage descriptor if available (for frame uniforms)
        if let Some(ctx) = &self.renderer_context {
            if let Some(desc_set) = ctx.storage_descriptor() {
                self.command_buffer.bind_graphics_descriptors(
                    pipeline.vk_layout(),
                    &[desc_set.into()],
                );
            }
        }

        drop(pipeline);

        // Draw fullscreen triangle (3 vertices)
        self.draw_fullscreen();
    }
}

/// Represents a render pass in the render graph.
/// Passes are constructed using PassBuilder and then added to the RenderGraph.
pub struct Pass {
    name: String,
    inputs: Vec<ResourceId>,
    outputs: Vec<ResourceId>,
    usages: Vec<ResourceUsage>,
    bind_point: PipelineBindPoint,
    extent: Option<Extent2D>,
    category: PassCategory,
    execute: Option<PassExecute>,
    #[allow(clippy::type_complexity)]
    pending_execute: Option<(String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'static>)>,
    /// Pre-execute callback runs BEFORE begin_rendering() for custom barrier setup.
    #[allow(clippy::type_complexity)]
    pending_pre_execute: Option<(String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'static>)>,
    pre_execute_name: Option<String>,
}

impl Pass {
    /// Create a pass from a PassBuilder.
    /// This is called internally by PassBuilder::build().
    pub fn from_builder(builder: PassBuilder) -> Self {
        Self {
            name: builder.name,
            inputs: builder.inputs,
            outputs: builder.outputs,
            usages: builder.usages,
            bind_point: builder.bind_point,
            extent: builder.extent,
            category: builder.category,
            execute: builder.execute,
            pending_execute: builder.pending_execute,
            pending_pre_execute: builder.pending_pre_execute,
            pre_execute_name: builder.pre_execute_name,
        }
    }

    /// Get the name of this pass
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get all input resource IDs for this pass
    pub fn inputs(&self) -> &[ResourceId] {
        &self.inputs
    }

    /// Get all output resource IDs for this pass
    pub fn outputs(&self) -> &[ResourceId] {
        &self.outputs
    }

    /// Get the usage information for all resources in this pass
    pub fn usages(&self) -> &[ResourceUsage] {
        &self.usages
    }

    /// Get the pipeline bind point for this pass
    pub fn bind_point(&self) -> PipelineBindPoint {
        self.bind_point
    }

    /// Get the extent for this pass, if explicitly set
    pub fn extent(&self) -> Option<Extent2D> {
        self.extent
    }

    /// Get the category of this pass
    pub fn category(&self) -> PassCategory {
        self.category
    }

    /// Execute this pass with the given context and registry
    pub fn execute(&self, ctx: Rc<PassExecutionContext>, registry: &mut ExecutionRegistry<'_>) {
        if let Some(execute) = &self.execute {
            registry.execute(&execute.pass_name, ctx);
        }
    }

    /// Execute pre-execute callback before begin_rendering()
    pub fn pre_execute(&self, ctx: Rc<PassExecutionContext>, registry: &mut ExecutionRegistry<'_>) {
        if let Some(name) = &self.pre_execute_name {
            registry.execute(name, ctx);
        }
    }

    /// Get the name of this pass for looking up the execution closure
    pub fn execute_name(&self) -> Option<&str> {
        self.execute.as_ref().map(|e| e.pass_name())
    }

    /// Get the execute name for this pass.
    /// This is used during compilation to transfer the name to the compiled pass.
    pub fn take_execute_name(&mut self) -> String {
        self.execute
            .take()
            .map(|e| e.pass_name().to_string())
            .unwrap_or_default()
    }

    /// Get the pre-execute name for this pass.
    /// This is used during compilation to transfer the name to the compiled pass.
    pub fn take_pre_execute_name(&mut self) -> Option<String> {
        self.pre_execute_name.take()
    }

    /// Take the pending execution closure and name from this pass.
    /// This is used during graph building to register the closure with the ExecutionRegistry.
    /// Returns None if no execution closure was specified.
    #[allow(clippy::type_complexity)]
    pub fn take_pending_execute(
        &mut self,
    ) -> Option<(String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'static>)> {
        self.pending_execute.take()
    }

    /// Take the pending pre-execution closure and name from this pass.
    /// This is used during graph building to register the closure with the ExecutionRegistry.
    #[allow(clippy::type_complexity)]
    pub fn take_pending_pre_execute(
        &mut self,
    ) -> Option<(String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'static>)> {
        self.pending_pre_execute.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_builder_creation() {
        let builder = PassBuilder::new("test_pass");
        assert_eq!(builder.name, "test_pass");
        assert!(builder.inputs.is_empty());
        assert!(builder.outputs.is_empty());
    }

    #[test]
    fn test_pass_builder_add_input_output() {
        let mut builder = PassBuilder::new("test_pass");

        let resource_id1 = ResourceId(0);
        let resource_id2 = ResourceId(1);

        builder.read(resource_id1);
        builder.write(Attachment::Color(resource_id2));

        assert_eq!(builder.inputs.len(), 1);
        assert_eq!(builder.outputs.len(), 1);
        assert_eq!(builder.inputs[0], resource_id1);
        assert_eq!(builder.outputs[0], resource_id2);
    }

    #[test]
    fn test_pass_builder_clear_color() {
        let mut builder = PassBuilder::new("test_pass");

        let resource_id = ResourceId(0);

        builder
            .write(Attachment::Color(resource_id))
            .clear_color(resource_id, [1.0, 0.0, 0.0, 1.0]);

        assert_eq!(builder.usages.len(), 1);
        let usage = &builder.usages[0];
        assert_eq!(usage.load_op, crate::types::AttachmentLoadOp::Clear.into());
        assert!(usage.clear_value.is_some());
    }

    #[test]
    fn test_pass_builder_extent() {
        let mut builder = PassBuilder::new("test_pass");

        builder.extent(1920, 1080);

        assert_eq!(builder.extent, Some(Extent2D::new(1920, 1080)));
    }

    #[test]
    fn test_pass_builder_bind_point() {
        let mut builder = PassBuilder::new("test_pass");

        builder.bind_point(PipelineBindPoint::Compute);

        assert_eq!(builder.bind_point, PipelineBindPoint::Compute);
    }

    #[test]
    fn test_pass_build() {
        let mut builder = PassBuilder::new("test_pass");

        let resource_id = ResourceId(0);
        builder.write(Attachment::Color(resource_id));

        let pass = builder.build();

        assert_eq!(pass.name(), "test_pass");
        assert_eq!(pass.outputs().len(), 1);
    }

    #[test]
    fn test_pass_accessors() {
        let mut builder = PassBuilder::new("test_pass");

        let input_id = ResourceId(0);
        let output_id = ResourceId(1);

        builder
            .read(input_id)
            .write(Attachment::Color(output_id))
            .extent(1920, 1080);

        let pass = builder.build();

        assert_eq!(pass.inputs(), &[input_id]);
        assert_eq!(pass.outputs(), &[output_id]);
        assert_eq!(pass.extent(), Some(Extent2D::new(1920, 1080)));
        assert_eq!(pass.bind_point(), PipelineBindPoint::Graphics);
        assert_eq!(pass.usages().len(), 2);
    }

    #[test]
    fn test_pass_builder_storage_buffer_methods() {
        let mut builder = PassBuilder::new("compute_pass");

        let buffer_id = ResourceId(0);

        builder
            .bind_point(PipelineBindPoint::Compute)
            .read_write_storage(buffer_id);

        assert_eq!(builder.bind_point, PipelineBindPoint::Compute);
        assert!(builder.inputs.contains(&buffer_id));
        assert!(builder.outputs.contains(&buffer_id));
        assert_eq!(builder.usages.len(), 1);
    }

    #[test]
    fn test_pass_builder_compute_pass() {
        let mut builder = PassBuilder::new("particle_sim");

        let particle_buffer = ResourceId(0);
        let config_buffer = ResourceId(1);

        builder
            .bind_point(PipelineBindPoint::Compute)
            .read_storage(config_buffer)
            .read_write_storage(particle_buffer);

        let pass = builder.build();

        assert_eq!(pass.bind_point(), PipelineBindPoint::Compute);
        assert!(pass.inputs().contains(&config_buffer));
        assert!(pass.inputs().contains(&particle_buffer));
        assert!(pass.outputs().contains(&particle_buffer));
    }
}
