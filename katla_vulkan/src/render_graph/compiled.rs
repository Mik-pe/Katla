use crate::sync::{
    AccessFlags2, DependencyInfo, ImageMemoryBarrier2, PipelineStage2Flags,
    COLOR_SUBRESOURCE_RANGE, DEPTH_SUBRESOURCE_RANGE,
};
use ash::vk;
use log::{debug, info};

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::render_graph::pass::{
    ExecutionRegistry, Pass, PassCategory, PassExecute, PassExecutionContext,
};
use crate::render_graph::renderer_context::RendererContext;
use crate::render_graph::resource::{
    CompiledResource, ResourceId, ResourceKind, ResourceLifetime, ResourceNameMap,
};
use crate::render_graph::types::{
    ClearValue, Extent2D, ImageLayout, Offset2D, Rect2D, RenderingAttachmentInfo, RenderingInfo, Viewport,
};
use crate::rendering::DrawList;
use crate::sync::{VkFramebuffer, VkImage, VkImageView};
use crate::CommandBuffer;
use crate::RenderGraphError;
use crate::VulkanContext;

/// CompiledRenderGraph represents a fully compiled render graph with all Vulkan objects created.
/// This is the result of the compilation process and can be executed each frame.
pub struct CompiledRenderGraph {
    pub context: Rc<VulkanContext>,
    pub passes: Vec<CompiledPass>,
    /// Resources map wrapped in RefCell for per-frame updates (e.g., viewport texture double-buffering)
    pub resources: Rc<RefCell<HashMap<ResourceId, CompiledResource>>>,
    /// Map from ResourceId to name for debugging
    resource_names: ResourceNameMap,
    /// Optional renderer context for safe access to renderer state
    renderer_context: Option<Rc<RendererContext>>,

    #[allow(dead_code)] // Needed for resource cleanup
    framebuffers: Vec<vk::Framebuffer>,
    pub registry: ExecutionRegistry<'static>,
    /// Cell for storing the draw list that will be processed during execution.
    /// This is set each frame before calling execute().
    draw_list_cell: Option<Rc<RefCell<Option<DrawList>>>>,
    /// Viewport color resource ID (for double-buffering updates)
    viewport_color_resource_id: Option<ResourceId>,
    /// Viewport depth resource ID (for double-buffering updates)
    viewport_depth_resource_id: Option<ResourceId>,
    /// Swapchain resource ID (for per-frame image updates)
    swapchain_resource_id: Option<ResourceId>,
}

/// CompiledPass represents a single compiled pass with all necessary Vulkan objects.
/// The execute field now contains the pass name for looking up the closure in the ExecutionRegistry.
/// Multiple framebuffers are supported (e.g., one per swapchain image).
pub struct CompiledPass {
    pub name: String,
    /// Multiple framebuffers - one per swapchain image variant
    pub vk_framebuffers: Vec<VkFramebuffer>,
    pub extent: Extent2D,
    pub clear_values: Vec<ClearValue>,
    pub category: PassCategory,
    execute: PassExecute,
    /// Pre-execute callback runs BEFORE begin_rendering() for custom barrier setup
    pre_execute: Option<PassExecute>,
    /// Color attachments as (image_view, resource_id) tuples for dynamic rendering
    pub color_attachments: Vec<(VkImageView, ResourceId)>,
    /// Depth attachment as (image_view, resource_id) tuple for dynamic rendering
    pub depth_attachment: Option<(VkImageView, ResourceId)>,
    /// Transfer source resources (need transition to TRANSFER_SRC_OPTIMAL)
    pub transfer_src_resources: Vec<ResourceId>,
    /// Transfer destination resources (need transition to TRANSFER_DST_OPTIMAL)
    pub transfer_dst_resources: Vec<ResourceId>,
}

/// RenderPassGroup groups passes that share a Vulkan render pass.
pub struct RenderPassGroup {
    pass_indices: Vec<usize>,
    attachments: Vec<ResourceId>,
    subpasses: Vec<SubpassDescriptor>,
}

/// SubpassDescriptor describes a subpass within a render pass.
pub struct SubpassDescriptor {
    pass_index: usize,
    input_attachments: Vec<(u32, ResourceId)>,
    color_attachments: Vec<(u32, ResourceId)>,
    depth_stencil: Option<(u32, ResourceId)>,

    // TODO: Replace raw vk types with wrapper types
    // AttachmentReference needs wrapper type in sync module
    // Store Vulkan attachment references to ensure they live long enough
    vk_input_refs: Vec<vk::AttachmentReference>,
    vk_color_refs: Vec<vk::AttachmentReference>,
    vk_depth_ref: Option<vk::AttachmentReference>,
}

impl CompiledRenderGraph {
    /// Get the VkImage for a resource ID.
    /// Returns None if the resource doesn't exist or isn't an image.
    fn get_image(&self, resource_id: ResourceId) -> Option<VkImage> {
        let resources = self.resources.borrow();
        resources.get(&resource_id).and_then(|resource| {
            match resource {
                CompiledResource::ExternalImage { image, .. } => Some(*image),
                CompiledResource::Image { image, .. } => Some(*image),
                _ => None,
            }
        })
    }

    /// Create multiple framebuffers for passes that use external images.
    /// This is useful for swapchain rendering where you need one framebuffer per swapchain image.
    /// Returns an error if the graph has already been compiled with framebuffers.
    ///
    /// NOTE: For swapchain rendering with dynamic rendering (Vulkan 1.3), framebuffers
    /// are NOT created. Dynamic rendering uses vkCmdBeginRendering/vkCmdEndRendering
    /// instead of traditional render passes and framebuffers.
    pub fn create_swapchain_framebuffers(
        &mut self,
        _swapchain_images: &[(VkImage, VkImageView, Extent2D, vk::Format)],
    ) -> Result<(), RenderGraphError> {
        // For dynamic rendering with swapchain, we don't create traditional framebuffers
        // Just ensure the framebuffers vectors are initialized (may be empty for dynamic rendering)

        for pass_idx in 0..self.passes.len() {
            let _pass = &self.passes[pass_idx];

            // For swapchain rendering, use dynamic rendering (no framebuffers needed)
            if self.passes[pass_idx].vk_framebuffers.is_empty() {
                // Initialize with empty vector for dynamic rendering
                self.passes[pass_idx].vk_framebuffers = vec![];
            }
        }

        info!(
            "Swapchain framebuffers setup complete: {} passes using dynamic rendering",
            self.passes.len()
        );

        Ok(())
    }

    /// Compile a render graph into Vulkan objects.
    pub fn compile(
        mut graph: crate::RenderGraph,
        registry: ExecutionRegistry<'static>,
        context: &Rc<VulkanContext>,
    ) -> Result<Self, RenderGraphError> {
        // Step 1: Analyze resource usage and lifetimes
        let resource_lifetimes = Self::analyze_lifetimes(&graph);

        // Step 2: Determine render pass structure
        let pass_structure = Self::determine_render_passes(&graph, &resource_lifetimes);

        // Step 3: Allocate resources
        let resources = Self::allocate_resources(&graph, &resource_lifetimes, context)?;

        // Step 4: Create framebuffers
        let framebuffers = Self::create_framebuffers(&pass_structure, &resources, context)?;

        // Step 5: Compile passes with execution info
        let compiled_passes = Self::compile_passes(&mut graph.passes, &framebuffers, &resources)?;

        // Step 6: Build resource name map for debugging
        let mut resource_names = ResourceNameMap::new();
        for (id, resource) in &graph.resources {
            resource_names.insert(*id, resource.name());
        }

        Ok(Self {
            context: context.clone(),
            passes: compiled_passes,
            resources: Rc::new(RefCell::new(resources)),
            resource_names,
            renderer_context: None,
            framebuffers,
            registry,
            draw_list_cell: None,
            viewport_color_resource_id: None,
            viewport_depth_resource_id: None,
            swapchain_resource_id: None,
        })
    }

    /// Get the name of a resource by ID for debugging.
    pub fn resource_name(&self, id: ResourceId) -> &str {
        self.resource_names.get_or_fallback(id)
    }

    /// Set the renderer context for safe access to renderer state.
    pub fn set_renderer_context(&mut self, ctx: Rc<RendererContext>) {
        self.renderer_context = Some(ctx);
    }

    /// Set the viewport resource IDs for double-buffering updates.
    /// This should be called after compile if the graph uses viewport textures.
    pub fn set_viewport_resource_ids(&mut self, color_id: ResourceId, depth_id: ResourceId) {
        self.viewport_color_resource_id = Some(color_id);
        self.viewport_depth_resource_id = Some(depth_id);
    }

    /// Set the swapchain resource ID for per-frame image updates.
    /// This should be called after compile if the graph uses the swapchain.
    pub fn set_swapchain_resource_id(&mut self, swapchain_id: ResourceId) {
        self.swapchain_resource_id = Some(swapchain_id);
    }

    /// Update the swapchain image for the current frame.
    /// This must be called before execute() to ensure present_pass blits to the correct image.
    pub fn update_swapchain_image(&mut self, image: VkImage, image_view: VkImageView) {
        if let Some(swapchain_id) = self.swapchain_resource_id {
            let mut resources = self.resources.borrow_mut();
            if let Some(resource) = resources.get_mut(&swapchain_id) {
                if let CompiledResource::ExternalImage {
                    image: res_image,
                    image_view: res_image_view,
                    ..
                } = resource
                {
                    *res_image = image;
                    *res_image_view = image_view;
                }
            }
        }
    }

    /// Analyze resource lifetimes across all passes.
    fn analyze_lifetimes(graph: &crate::RenderGraph) -> HashMap<ResourceId, ResourceLifetime> {
        let mut lifetimes = HashMap::new();

        for (pass_index, pass) in graph.passes.iter().enumerate() {
            // Process outputs (writes)
            for resource_id in pass.outputs() {
                let entry = lifetimes
                    .entry(*resource_id)
                    .or_insert_with(|| ResourceLifetime::new(pass_index, pass_index, false));

                if entry.first_use > pass_index {
                    entry.first_use = pass_index;
                }
                if entry.last_use < pass_index {
                    entry.last_use = pass_index;
                }
            }

            // Process inputs (reads)
            for resource_id in pass.inputs() {
                let entry = lifetimes
                    .entry(*resource_id)
                    .or_insert_with(|| ResourceLifetime::new(pass_index, pass_index, true));

                if entry.first_use > pass_index {
                    entry.first_use = pass_index;
                }
                if entry.last_use < pass_index {
                    entry.last_use = pass_index;
                }
            }
        }

        // Mark external resources as non-transient
        for (resource_id, resource) in &graph.resources {
            match &resource.kind {
                ResourceKind::ExternalBuffer { .. } | ResourceKind::ExternalImage { .. } => {
                    if let Some(lifetime) = lifetimes.get_mut(resource_id) {
                        lifetime.is_transient = false;
                    }
                }
                _ => {}
            }
        }

        lifetimes
    }

    /// Determine render pass structure.
    /// For now, simple approach: one render pass per pass.
    fn determine_render_passes(
        graph: &crate::RenderGraph,
        _lifetimes: &HashMap<ResourceId, ResourceLifetime>,
    ) -> Vec<RenderPassGroup> {
        let mut groups = Vec::new();

        for (pass_index, pass) in graph.passes.iter().enumerate() {
            let mut group = RenderPassGroup {
                pass_indices: vec![pass_index],
                attachments: Vec::new(),
                subpasses: vec![SubpassDescriptor {
                    pass_index,
                    input_attachments: Vec::new(),
                    color_attachments: Vec::new(),
                    depth_stencil: None,
                    vk_input_refs: Vec::new(),
                    vk_color_refs: Vec::new(),
                    vk_depth_ref: None,
                }],
            };

            // Add all output resources as attachments
            // We check ResourceUsage.layout to determine if it's color or depth-stencil
            for (resource_id, usage) in pass.outputs().iter().zip(pass.usages()) {
                group.attachments.push(*resource_id);

                let attachment_index = (group.attachments.len() - 1) as u32;

                // Check the layout to determine attachment type
                let is_depth_stencil = usage.layout
                    == crate::render_graph::types::ImageLayout::DepthStencilAttachmentOptimal
                    || usage.layout
                        == crate::render_graph::types::ImageLayout::DepthStencilReadOnlyOptimal;

                if let Some(subpass) = group.subpasses.first_mut() {
                    if is_depth_stencil {
                        // Add as depth-stencil attachment (NOT as color attachment)
                        subpass.depth_stencil = Some((attachment_index, *resource_id));
                    } else {
                        // Add as color attachment
                        subpass
                            .color_attachments
                            .push((attachment_index, *resource_id));
                    }
                }
            }

            // Check for depth/stencil
            for resource_id in pass.outputs() {
                if let Some(resource) = graph.resources.get(resource_id) {
                    if let ResourceKind::Image { format, .. } = &resource.kind {
                        let is_depth = is_depth_or_stencil_format(*format);
                        if is_depth {
                            if let Some(subpass) = group.subpasses.first_mut() {
                                let attachment_index = group
                                    .attachments
                                    .iter()
                                    .position(|r| r == resource_id)
                                    .unwrap()
                                    as u32;
                                subpass.depth_stencil = Some((attachment_index, *resource_id));
                            }
                        }
                    }
                }
            }

            // Populate Vulkan attachment references
            if let Some(subpass) = group.subpasses.first_mut() {
                // Build color attachment references
                for (idx, _) in &subpass.color_attachments {
                    subpass.vk_color_refs.push(
                        vk::AttachmentReference::default()
                            .attachment(*idx)
                            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
                    );
                }

                // Build depth/stencil attachment reference
                if let Some((idx, _)) = subpass.depth_stencil {
                    subpass.vk_depth_ref = Some(
                        vk::AttachmentReference::default()
                            .attachment(idx)
                            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
                    );
                }

                // Build input attachment references
                for (idx, _) in &subpass.input_attachments {
                    subpass.vk_input_refs.push(
                        vk::AttachmentReference::default()
                            .attachment(*idx)
                            .layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                    );
                }
            }

            groups.push(group);
        }

        groups
    }

    /// Generate Vulkan render passes.
    fn generate_render_passes(
        groups: &[RenderPassGroup],
        graph: &crate::RenderGraph,
        _context: &Rc<VulkanContext>,
    ) -> Result<Vec<vk::RenderPass>, RenderGraphError> {
        let mut render_passes = Vec::new();

        for group in groups {
            let mut attachments = Vec::new();
            let mut subpasses = Vec::new();

            // Create attachment descriptions
            for resource_id in &group.attachments {
                let resource = graph
                    .resources
                    .get(resource_id)
                    .ok_or(RenderGraphError::ResourceNotFound(resource_id.0))?;

                let attachment = match &resource.kind {
                    ResourceKind::Image {
                        format,
                        samples,
                        initial_layout,
                        final_layout,
                        ..
                    } => {
                        let vk_format: vk::Format = (*format).into();
                        let vk_samples: vk::SampleCountFlags = (*samples).into();
                        let vk_initial_layout: vk::ImageLayout = (*initial_layout).into();
                        let vk_final_layout: vk::ImageLayout = (*final_layout).into();

                        // Find the usage for this attachment
                        let mut load_op = vk::AttachmentLoadOp::DONT_CARE;
                        let mut store_op = vk::AttachmentStoreOp::DONT_CARE;

                        // Look at the first pass that uses this resource as an output
                        for pass_idx in &group.pass_indices {
                            if let Some(pass) = graph.passes.get(*pass_idx) {
                                for usage in pass.usages() {
                                    if usage.resource_id == *resource_id {
                                        load_op = usage.load_op.into();
                                        store_op = usage.store_op.into();
                                        break;
                                    }
                                }
                            }
                        }

                        let (_final_layout_stencil, stencil_load_op, stencil_store_op) = {
                            if is_depth_or_stencil_format(*format) {
                                (
                                    vk_final_layout,
                                    vk::AttachmentLoadOp::DONT_CARE,
                                    vk::AttachmentStoreOp::DONT_CARE,
                                )
                            } else {
                                (vk::ImageLayout::UNDEFINED, load_op, store_op)
                            }
                        };

                        vk::AttachmentDescription::default()
                            .format(vk_format)
                            .samples(vk_samples)
                            .load_op(load_op)
                            .store_op(store_op)
                            .stencil_load_op(stencil_load_op)
                            .stencil_store_op(stencil_store_op)
                            .initial_layout(vk_initial_layout)
                            .final_layout(vk_final_layout)
                    }
                    ResourceKind::ExternalImage { format, .. } => {
                        let vk_format: vk::Format = (*format).into();

                        // Find the usage for this attachment (same as Image case)
                        let mut load_op = vk::AttachmentLoadOp::DONT_CARE;
                        let mut store_op = vk::AttachmentStoreOp::DONT_CARE;

                        // Look at the first pass that uses this resource as an output
                        for pass_idx in &group.pass_indices {
                            if let Some(pass) = graph.passes.get(*pass_idx) {
                                for usage in pass.usages() {
                                    if usage.resource_id == *resource_id {
                                        load_op = usage.load_op.into();
                                        store_op = usage.store_op.into();
                                        break;
                                    }
                                }
                            }
                        }

                        // Determine final layout based on format (swapchain vs depth)
                        let is_depth = is_depth_or_stencil_format(*format);
                        let final_layout = if is_depth {
                            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
                        } else {
                            vk::ImageLayout::PRESENT_SRC_KHR
                        };

                        info!("ExternalImage attachment: format={:?}, load_op={:?}, store_op={:?}, final_layout={:?}",
                            format, load_op, store_op, final_layout);

                        vk::AttachmentDescription::default()
                            .format(vk_format)
                            .samples(vk::SampleCountFlags::TYPE_1)
                            .load_op(load_op)
                            .store_op(store_op)
                            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                            .initial_layout(vk::ImageLayout::UNDEFINED)
                            .final_layout(final_layout)
                    }
                    _ => {
                        return Err(RenderGraphError::InvalidResourceUsage(format!(
                            "Resource {:?} is not an image",
                            resource_id
                        )))
                    }
                };

                attachments.push(attachment);
            }

            // Create subpass descriptions
            for subpass_desc in &group.subpasses {
                let pass = graph.passes.get(subpass_desc.pass_index).ok_or(
                    RenderGraphError::CompilationError(format!(
                        "Pass index {} not found",
                        subpass_desc.pass_index
                    )),
                )?;

                let bind_point = pass.bind_point();

                let mut subpass = vk::SubpassDescription::default()
                    .pipeline_bind_point(bind_point.into())
                    .color_attachments(&subpass_desc.vk_color_refs);

                // Only set input_attachments if we have any
                if !subpass_desc.vk_input_refs.is_empty() {
                    subpass = subpass.input_attachments(&subpass_desc.vk_input_refs);
                }

                let subpass = if let Some(ref depth_ref) = subpass_desc.vk_depth_ref {
                    subpass.depth_stencil_attachment(depth_ref)
                } else {
                    subpass
                };

                subpasses.push(subpass);
            }

            // Create subpass dependencies
            // Always create at least one dependency from EXTERNAL to subpass 0
            // This is required for proper synchronization with external operations
            let mut dependencies: Vec<vk::SubpassDependency> = Vec::new();
            if subpasses.len() > 1 {
                for i in 0..subpasses.len() - 1 {
                    let dependency = vk::SubpassDependency::default()
                        .src_subpass(i as u32)
                        .dst_subpass((i + 1) as u32)
                        .src_stage_mask(
                            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                        )
                        .dst_stage_mask(
                            vk::PipelineStageFlags::FRAGMENT_SHADER
                                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                        )
                        .src_access_mask(
                            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                        )
                        .dst_access_mask(vk::AccessFlags::INPUT_ATTACHMENT_READ);

                    dependencies.push(dependency);
                }
            }

            // Add dependency from EXTERNAL to first subpass for proper external synchronization
            let external_dependency = vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
            dependencies.push(external_dependency);

            // Dynamic rendering: use null render pass
            let render_pass = vk::RenderPass::null();
            render_passes.push(render_pass);
        }

        Ok(render_passes)
    }

    /// Allocate Vulkan resources.
    fn allocate_resources(
        graph: &crate::RenderGraph,
        _lifetimes: &HashMap<ResourceId, ResourceLifetime>,
        context: &Rc<VulkanContext>,
    ) -> Result<HashMap<ResourceId, CompiledResource>, RenderGraphError> {
        let mut resources = HashMap::new();

        for (resource_id, resource) in &graph.resources {
            match &resource.kind {
                ResourceKind::ExternalBuffer { buffer } => {
                    resources.insert(
                        *resource_id,
                        CompiledResource::ExternalBuffer { buffer: *buffer },
                    );
                }
                ResourceKind::ExternalImage {
                    image,
                    image_view,
                    format,
                    extent,
                    ..
                } => {
                    resources.insert(
                        *resource_id,
                        CompiledResource::ExternalImage {
                            image: *image,
                            image_view: *image_view,
                            format: *format,
                            extent: *extent,
                        },
                    );
                }
                ResourceKind::Buffer {
                    size,
                    usage,
                    memory_properties,
                } => {
                    let vk_usage = crate::render_graph::types::BufferUsage::all(usage.clone());
                    let vk_memory_props =
                        crate::render_graph::types::MemoryProperty::all(memory_properties.clone());

                    let buffer_info = vk::BufferCreateInfo::default()
                        .size(*size)
                        .usage(vk_usage)
                        .sharing_mode(vk::SharingMode::EXCLUSIVE);

                    let location =
                        if vk_memory_props.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL) {
                            gpu_allocator::MemoryLocation::GpuOnly
                        } else {
                            gpu_allocator::MemoryLocation::CpuToGpu
                        };

                    let (buffer, allocation) = context.allocate_buffer(&buffer_info, location);

                    resources.insert(
                        *resource_id,
                        CompiledResource::Buffer {
                            buffer: buffer.into(),
                            allocation,
                            size: *size,
                        },
                    );
                }
                ResourceKind::Image {
                    extent,
                    format,
                    usage,
                    tiling,
                    ..
                } => {
                    let vk_usage = crate::render_graph::types::ImageUsage::all(usage.clone());
                    let vk_format: vk::Format = (*format).into();
                    let vk_tiling: vk::ImageTiling = (*tiling).into();
                    let vk_extent: vk::Extent3D = (*extent).into();

                    let image_info = vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .extent(vk_extent)
                        .mip_levels(1)
                        .array_layers(1)
                        .format(vk_format)
                        .tiling(vk_tiling)
                        .initial_layout(vk::ImageLayout::UNDEFINED)
                        .usage(vk_usage)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .sharing_mode(vk::SharingMode::EXCLUSIVE);

                    let location = gpu_allocator::MemoryLocation::GpuOnly;
                    let (image, allocation) = context.create_image(image_info, location);

                    // Create image view
                    // Determine aspect mask based on format (depth/stencil vs color)
                    let aspect_mask = if is_depth_or_stencil_format(*format) {
                        vk::ImageAspectFlags::DEPTH
                    } else {
                        vk::ImageAspectFlags::COLOR
                    };
                    let image_view = context.create_image_view(image, vk_format, aspect_mask);

                    resources.insert(
                        *resource_id,
                        CompiledResource::Image {
                            image: image.into(),
                            image_view: image_view.into(),
                            allocation,
                            extent: *extent,
                            format: *format,
                            layout: crate::render_graph::types::ImageLayout::Undefined,
                        },
                    );
                }
            }
        }

        Ok(resources)
    }

    /// Create framebuffers for each render pass.
    fn create_framebuffers(
        groups: &[RenderPassGroup],
        resources: &HashMap<ResourceId, CompiledResource>,
        _context: &Rc<VulkanContext>,
    ) -> Result<Vec<vk::Framebuffer>, RenderGraphError> {
        let mut framebuffers = Vec::new();

        for group in groups.iter() {
            // Check if this pass uses external resources
            let uses_external = group.attachments.iter().any(|resource_id| {
                matches!(
                    resources.get(resource_id),
                    Some(CompiledResource::ExternalImage { .. })
                        | Some(CompiledResource::ExternalBuffer { .. })
                )
            });

            // Skip framebuffer creation for passes with external resources
            // They will be created later by create_swapchain_framebuffers()
            // For dynamic rendering, we don't need traditional framebuffers
            if uses_external {
                framebuffers.push(vk::Framebuffer::null());
                continue;
            }

            // For non-external resources (internal render targets), we also skip framebuffer creation
            // when using Dynamic Rendering. Dynamic Rendering renders directly to image views
            // without needing a framebuffer object.
            // Framebuffers are only needed for legacy render pass compatibility.
            framebuffers.push(vk::Framebuffer::null());
        }

        Ok(framebuffers)
    }

    /// Compile passes with execution info.
    fn compile_passes(
        passes: &mut [Pass],
        framebuffers: &[vk::Framebuffer],
        resources: &HashMap<ResourceId, CompiledResource>,
    ) -> Result<Vec<CompiledPass>, RenderGraphError> {
        let mut compiled_passes = Vec::new();

        for (i, pass) in passes.iter_mut().enumerate() {
            // Collect clear values
            let clear_values: Vec<ClearValue> = pass
                .usages()
                .iter()
                .filter_map(|usage| usage.clear_value)
                .collect();

            // Get extent from resources or pass
            let extent = if let Some(extent) = pass.extent() {
                extent
            } else if let Some(resource_id) = pass.outputs().first() {
                // Try to get extent from output resources
                if let Some(CompiledResource::Image { extent, .. }) = resources.get(resource_id) {
                    Extent2D::new(extent.width, extent.height)
                } else if let Some(CompiledResource::ExternalImage { extent, .. }) =
                    resources.get(resource_id)
                {
                    (*extent)
                } else {
                    return Err(RenderGraphError::CompilationError(format!(
                        "Cannot determine extent for resource {:?} - no explicit extent set",
                        resource_id
                    )));
                }
            } else {
                return Err(RenderGraphError::CompilationError(
                    "No output resources in pass and no explicit extent set".into(),
                ));
            };

            // Get framebuffer (null for external resources using dynamic rendering)
            let vk_framebuffer_raw = framebuffers
                .get(i)
                .copied()
                .unwrap_or(vk::Framebuffer::null());

            // Wrap in our wrapper type
            let vk_framebuffers = vec![VkFramebuffer::new(vk_framebuffer_raw)];

            // Get execute name from the pass
            let execute_name = pass.take_execute_name();
            let execute = PassExecute::new(execute_name);

            // Get pre-execute name from the pass (for custom barriers before begin_rendering)
            let pre_execute = pass.take_pre_execute_name().map(PassExecute::new);

            // Extract color and depth attachments for dynamic rendering
            // Store as (image_view, resource_id) tuples for easy layout transitions
            let mut color_attachments: Vec<(VkImageView, ResourceId)> = Vec::new();
            let mut depth_attachment: Option<(VkImageView, ResourceId)> = None;
            let mut transfer_src_resources: Vec<ResourceId> = Vec::new();
            let mut transfer_dst_resources: Vec<ResourceId> = Vec::new();

            for output_resource_id in pass.outputs() {
                // Check the usage to determine if this is a render attachment or transfer resource
                let usage_info = pass
                    .usages()
                    .iter()
                    .find(|u| u.resource_id == *output_resource_id);

                let layout = usage_info.map(|u| u.layout).unwrap_or(ImageLayout::Undefined);

                let is_render_attachment = matches!(
                    layout,
                    ImageLayout::ColorAttachmentOptimal | ImageLayout::DepthStencilAttachmentOptimal
                );

                let is_transfer_src = layout == ImageLayout::TransferSrcOptimal;
                let is_transfer_dst = layout == ImageLayout::TransferDstOptimal;

                debug!(
                    "compile pass '{}': output {:?}, layout={:?}, is_render={}, is_transfer_src={}, is_transfer_dst={}",
                    pass.name(),
                    output_resource_id,
                    layout,
                    is_render_attachment,
                    is_transfer_src,
                    is_transfer_dst
                );

                if is_transfer_src {
                    transfer_src_resources.push(*output_resource_id);
                    continue;
                }
                if is_transfer_dst {
                    transfer_dst_resources.push(*output_resource_id);
                    continue;
                }

                // Skip other non-render resources
                if !is_render_attachment {
                    continue;
                }

                if let Some(resource) = resources.get(output_resource_id) {
                    match resource {
                        CompiledResource::ExternalImage {
                            image_view, format, ..
                        } => {
                            if is_depth_or_stencil_format(*format) {
                                depth_attachment = Some((*image_view, *output_resource_id));
                            } else {
                                color_attachments.push((*image_view, *output_resource_id));
                            }
                        }
                        CompiledResource::Image {
                            image_view, format, ..
                        } => {
                            if is_depth_or_stencil_format(*format) {
                                depth_attachment = Some((*image_view, *output_resource_id));
                            } else {
                                color_attachments.push((*image_view, *output_resource_id));
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Also check inputs for transfer reads
            for input_resource_id in pass.inputs() {
                let usage_info = pass
                    .usages()
                    .iter()
                    .find(|u| u.resource_id == *input_resource_id);

                if let Some(usage) = usage_info {
                    if usage.layout == ImageLayout::TransferSrcOptimal
                        && !transfer_src_resources.contains(input_resource_id) {
                            transfer_src_resources.push(*input_resource_id);
                        }
                }
            }

            let compiled = CompiledPass {
                name: pass.name().to_string(),
                vk_framebuffers,
                extent,
                clear_values,
                category: pass.category(),
                execute,
                pre_execute,
                color_attachments,
                depth_attachment,
                transfer_src_resources,
                transfer_dst_resources,
            };

            compiled_passes.push(compiled);
        }

        Ok(compiled_passes)
    }

    /// Set the draw list cell that will be used during execution.
    ///
    /// This is called during setup_render_graph to store the cell that
    /// closures will capture.
    pub fn set_draw_list_cell(&mut self, cell: Rc<RefCell<Option<DrawList>>>) {
        self.draw_list_cell = Some(cell);
    }

    /// Set the draw list for this frame.
    ///
    /// This should be called before execute() to provide the draw calls
    /// that will be processed during render graph execution.
    pub fn set_draw_list(&mut self, draw_list: DrawList) {
        if let Some(cell) = &self.draw_list_cell {
            *cell.borrow_mut() = Some(draw_list);
        }
    }

    /// Update viewport texture attachments for double-buffering support.
    ///
    /// This should be called before execute() to update the viewport color and depth
    /// attachments to use the correct textures for the current frame index.
    /// This prevents race conditions when frames overlap (frames in flight > 1).
    pub fn update_viewport_attachments(
        &mut self,
        color_image_view: VkImageView,
        depth_image_view: VkImageView,
        color_image: VkImage,
        depth_image: VkImage,
    ) {
        // Update color_attachments for viewport passes (sky_pass, geometry_pass, particle_pass)
        for pass in &mut self.passes {
            // Skip ui_pass and present_pass (they use output texture or swapchain transfer, not viewport)
            if pass.category == PassCategory::Ui || pass.category == PassCategory::Present {
                continue;
            }

            // For viewport passes, update the first color attachment's image view
            if !pass.color_attachments.is_empty() {
                pass.color_attachments[0].0 = color_image_view;
            }

            // Update depth attachment
            if let Some((ref mut depth_view, _)) = pass.depth_attachment {
                *depth_view = depth_image_view;
            }
        }

        // Update the resources map so ctx.get_image() returns the correct viewport texture
        // This is crucial for copy_pass which uses get_image() to get the viewport texture
        let mut resources = self.resources.borrow_mut();

        // Update viewport color resource
        if let Some(color_id) = self.viewport_color_resource_id {
            if let Some(resource) = resources.get_mut(&color_id) {
                if let CompiledResource::ExternalImage {
                    image, image_view, ..
                } = resource
                {
                    *image = color_image;
                    *image_view = color_image_view;
                }
            }
        }

        // Update viewport depth resource
        if let Some(depth_id) = self.viewport_depth_resource_id {
            if let Some(resource) = resources.get_mut(&depth_id) {
                if let CompiledResource::ExternalImage {
                    image, image_view, ..
                } = resource
                {
                    *image = depth_image;
                    *image_view = depth_image_view;
                }
            }
        }
    }

    /// Execute the compiled render graph.
    /// Executes all passes in order using the provided command buffer.
    /// The ExecutionRegistry (owned by this graph) provides the closure logic.
    /// The image_index selects which framebuffer to use for each pass (for swapchain images).
    ///
    /// Uses Dynamic Rendering (VK_KHR_dynamic_rendering) when attachments are available,
    /// otherwise falls back to traditional render passes.
    pub fn execute(
        &mut self,
        command_buffer: &mut CommandBuffer,
        image_index: usize,
        swapchain_images: &[VkImage],
        depth_image: VkImage,
    ) -> Result<(), RenderGraphError> {
        let pass_count = self.passes.len();
        debug!("execute: starting {} passes", pass_count);
        for i in 0..pass_count {
            // Check if we have dynamic rendering attachments
            let has_color_attachments = !self.passes[i].color_attachments.is_empty();
            let has_depth_attachment = self.passes[i].depth_attachment.is_some();
            let has_render_attachments = has_color_attachments || has_depth_attachment;
            let is_last_pass = i == pass_count - 1;

            debug!(
                "execute: pass {} ({}), has_color={}, has_depth={}, is_transfer={}",
                i,
                self.passes[i].name,
                has_color_attachments,
                has_depth_attachment,
                !has_render_attachments
            );

            if has_render_attachments {
                // Use Dynamic Rendering path (Vulkan 1.3)
                debug!("execute: calling execute_pass_dynamic for pass {}", i);
                self.execute_pass_dynamic(
                    command_buffer,
                    i,
                    image_index,
                    swapchain_images,
                    depth_image,
                    is_last_pass,
                )?;
                debug!("execute: execute_pass_dynamic for pass {} complete", i);
            } else {
                // Transfer/compute pass - no render pass needed
                debug!("execute: calling execute_pass_transfer for pass {}", i);
                self.execute_pass_transfer(command_buffer, i, is_last_pass)?;
                debug!("execute: execute_pass_transfer for pass {} complete", i);
            }
        }
        Ok(())
    }

    /// Execute the render graph without swapchain dependencies.
    ///
    /// This is used for offscreen rendering (e.g., model preview) where
    /// the graph doesn't need to interact with the swapchain.
    ///
    /// Uses image_index 0 for all attachments (single set of attachments).
    pub fn execute_no_swapchain(
        &mut self,
        command_buffer: &mut CommandBuffer,
    ) -> Result<(), RenderGraphError> {
        let pass_count = self.passes.len();
        debug!("execute_no_swapchain: starting {} passes", pass_count);

        // Use image_index 0 for all passes (offscreen rendering has single attachment set)
        let image_index = 0;
        let empty_swapchain: [VkImage; 0] = [];
        // Null depth image - preview graphs handle their own depth barriers in pre_execute
        let no_depth = VkImage::new(vk::Image::null());

        for i in 0..pass_count {
            let has_color_attachments = !self.passes[i].color_attachments.is_empty();
            let has_depth_attachment = self.passes[i].depth_attachment.is_some();
            let has_render_attachments = has_color_attachments || has_depth_attachment;
            let is_last_pass = i == pass_count - 1;

            debug!(
                "execute_no_swapchain: pass {} ({}), has_color={}, has_depth={}",
                i,
                self.passes[i].name,
                has_color_attachments,
                has_depth_attachment,
            );

            if has_render_attachments {
                debug!("execute_no_swapchain: calling execute_pass_dynamic for pass {}", i);
                self.execute_pass_dynamic(
                    command_buffer,
                    i,
                    image_index,
                    &empty_swapchain,
                    no_depth,
                    is_last_pass,
                )?;
                debug!("execute_no_swapchain: execute_pass_dynamic for pass {} complete", i);
            } else {
                debug!("execute_no_swapchain: calling execute_pass_transfer for pass {}", i);
                self.execute_pass_transfer(command_buffer, i, is_last_pass)?;
                debug!("execute_no_swapchain: execute_pass_transfer for pass {} complete", i);
            }
        }
        Ok(())
    }

    /// Execute a transfer-only pass (no render pass, just command recording).
    /// Used for vkCmdBlitImage, vkCmdCopyImage, compute dispatches, etc.
    fn execute_pass_transfer(
        &mut self,
        command_buffer: &mut CommandBuffer,
        pass_index: usize,
        _is_last_pass: bool,
    ) -> Result<(), RenderGraphError> {
        let pass = &self.passes[pass_index];

        // Transition transfer source images to TRANSFER_SRC_OPTIMAL
        for resource_id in &pass.transfer_src_resources {
            if let Some(image) = self.get_image(*resource_id) {
                let barrier = ImageMemoryBarrier2::new(image)
                    .src_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
                    .src_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
                    .dst_stage(PipelineStage2Flags::TRANSFER)
                    .dst_access(AccessFlags2::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .subresource_range(COLOR_SUBRESOURCE_RANGE);

                DependencyInfo::new()
                    .add_image_barrier(barrier)
                    .build(|dep_info| unsafe {
                        self.context
                            .device
                            .cmd_pipeline_barrier2(command_buffer.vk_command_buffer(), dep_info);
                    });
            }
        }

        // Transition transfer destination images to TRANSFER_DST_OPTIMAL
        for resource_id in &pass.transfer_dst_resources {
            if let Some(image) = self.get_image(*resource_id) {
                let barrier = ImageMemoryBarrier2::new(image)
                    .src_stage(PipelineStage2Flags::BOTTOM_OF_PIPE)
                    .src_access(AccessFlags2::NONE)
                    .dst_stage(PipelineStage2Flags::TRANSFER)
                    .dst_access(AccessFlags2::TRANSFER_WRITE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .subresource_range(COLOR_SUBRESOURCE_RANGE);

                DependencyInfo::new()
                    .add_image_barrier(barrier)
                    .build(|dep_info| unsafe {
                        self.context
                            .device
                            .cmd_pipeline_barrier2(command_buffer.vk_command_buffer(), dep_info);
                    });
            }
        }

        // Create execution context with optional renderer context
        let ctx = if let Some(ref rc) = self.renderer_context {
            Rc::new(PassExecutionContext::with_renderer_context(
                command_buffer.clone(),
                Rc::clone(&self.resources),
                pass.extent,
                Rc::clone(rc),
            ))
        } else {
            Rc::new(PassExecutionContext::new_dynamic(
                command_buffer.clone(),
                Rc::clone(&self.resources),
                pass.extent,
            ))
        };

        // Execute the pass closure directly (no begin_rendering/end_rendering)
        pass.execute(ctx, &mut self.registry);

        // After blit: transition transfer destinations to final layout
        for resource_id in &pass.transfer_dst_resources {
            if let Some(image) = self.get_image(*resource_id) {
                // Check if this is the swapchain - transition to PRESENT_SRC
                let is_swapchain = self.swapchain_resource_id == Some(*resource_id);

                let (new_layout, dst_stage, dst_access) = if is_swapchain {
                    (vk::ImageLayout::PRESENT_SRC_KHR, PipelineStage2Flags::BOTTOM_OF_PIPE, AccessFlags2::NONE)
                } else {
                    // For other textures, transition back to COLOR_ATTACHMENT for next frame
                    (vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT, AccessFlags2::COLOR_ATTACHMENT_WRITE)
                };

                let barrier = ImageMemoryBarrier2::new(image)
                    .src_stage(PipelineStage2Flags::TRANSFER)
                    .src_access(AccessFlags2::TRANSFER_WRITE)
                    .dst_stage(dst_stage)
                    .dst_access(dst_access)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(new_layout)
                    .subresource_range(COLOR_SUBRESOURCE_RANGE);

                DependencyInfo::new()
                    .add_image_barrier(barrier)
                    .build(|dep_info| unsafe {
                        self.context
                            .device
                            .cmd_pipeline_barrier2(command_buffer.vk_command_buffer(), dep_info);
                    });
            }
        }

        // Transition transfer sources back to COLOR_ATTACHMENT_OPTIMAL for next frame
        for resource_id in &pass.transfer_src_resources {
            if let Some(image) = self.get_image(*resource_id) {
                let barrier = ImageMemoryBarrier2::new(image)
                    .src_stage(PipelineStage2Flags::TRANSFER)
                    .src_access(AccessFlags2::TRANSFER_READ)
                    .dst_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
                    .dst_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .subresource_range(COLOR_SUBRESOURCE_RANGE);

                DependencyInfo::new()
                    .add_image_barrier(barrier)
                    .build(|dep_info| unsafe {
                        self.context
                            .device
                            .cmd_pipeline_barrier2(command_buffer.vk_command_buffer(), dep_info);
                    });
            }
        }

        Ok(())
    }

    /// Execute a pass using dynamic rendering (Vulkan 1.3).
    fn execute_pass_dynamic(
        &mut self,
        command_buffer: &mut CommandBuffer,
        pass_index: usize,
        _image_index: usize,
        _swapchain_images: &[VkImage],
        _depth_image: VkImage,
        _is_last_pass: bool,
    ) -> Result<(), RenderGraphError> {
        let pass = &self.passes[pass_index];

        // Build rendering info
        let mut rendering_info = RenderingInfo::new()
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: pass.extent.into(),
            })
            .layer_count(1);

        // Add color attachments with clear values
        let color_clears: Vec<_> = pass
            .clear_values
            .iter()
            .filter(|cv| matches!(cv, ClearValue::Color(_)))
            .collect();

        for (i, (image_view, _resource_id)) in pass.color_attachments.iter().enumerate() {
            let mut attachment = RenderingAttachmentInfo::new(*image_view)
                .layout(ImageLayout::ColorAttachmentOptimal);

            // Use filtered color clears, indexed by color attachment position
            if let Some(&cv) = color_clears.get(i) {
                attachment = attachment.clear(*cv);
            }

            rendering_info = rendering_info.add_color_attachment(attachment);
        }

        // Add depth attachment with clear value
        if let Some((depth_view, _resource_id)) = pass.depth_attachment {
            // Find depth clear value
            let depth_clear = pass
                .clear_values
                .iter()
                .find(|cv| matches!(cv, ClearValue::DepthStencil(_)));

            let mut attachment = RenderingAttachmentInfo::new(depth_view)
                .layout(ImageLayout::DepthStencilAttachmentOptimal);

            if let Some(&cv) = depth_clear {
                attachment = attachment.clear(cv);
            }

            rendering_info = rendering_info.depth_attachment(attachment);
        }

        // Transition ALL color attachment images to COLOR_ATTACHMENT_OPTIMAL before rendering
        // The render graph automatically handles layout transitions for all render targets.
        for (_image_view, resource_id) in &pass.color_attachments {
            if let Some(image) = self.get_image(*resource_id) {
                // Determine old layout based on pass index
                // First pass: UNDEFINED (discard previous content)
                // Subsequent passes: COLOR_ATTACHMENT_OPTIMAL (preserve content)
                let old_layout = if pass_index == 0 {
                    vk::ImageLayout::UNDEFINED
                } else {
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                };

                let barrier = ImageMemoryBarrier2::new(image)
                    .src_stage(PipelineStage2Flags::BOTTOM_OF_PIPE)
                    .src_access(AccessFlags2::NONE)
                    .dst_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
                    .dst_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
                    .old_layout(old_layout)
                    .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .subresource_range(COLOR_SUBRESOURCE_RANGE);

                DependencyInfo::new()
                    .add_image_barrier(barrier)
                    .build(|dep_info| unsafe {
                        self.context
                            .device
                            .cmd_pipeline_barrier2(command_buffer.vk_command_buffer(), dep_info);
                    });
            }
        }

        // Transition depth image to DEPTH_STENCIL_ATTACHMENT_OPTIMAL before rendering
        if let Some((_depth_view, depth_resource_id)) = pass.depth_attachment {
            if let Some(depth_image) = self.get_image(depth_resource_id) {
                let old_layout = if pass_index == 0 {
                    vk::ImageLayout::UNDEFINED
                } else {
                    vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
                };

                let barrier = ImageMemoryBarrier2::new(depth_image)
                    .src_stage(PipelineStage2Flags::LATE_FRAGMENT_TESTS)
                    .src_access(AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
                    .dst_stage(PipelineStage2Flags::EARLY_FRAGMENT_TESTS)
                    .dst_access(AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ.union(AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE))
                    .old_layout(old_layout)
                    .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                    .subresource_range(DEPTH_SUBRESOURCE_RANGE);

                DependencyInfo::new()
                    .add_image_barrier(barrier)
                    .build(|dep_info| unsafe {
                        self.context
                            .device
                            .cmd_pipeline_barrier2(command_buffer.vk_command_buffer(), dep_info);
                    });
            }
        }

        // Create execution context early so pre_execute can use it
        let ctx = if let Some(ref rc) = self.renderer_context {
            Rc::new(PassExecutionContext::with_renderer_context(
                (*command_buffer).clone(),
                self.resources.clone(),
                pass.extent,
                Rc::clone(rc),
            ))
        } else {
            Rc::new(PassExecutionContext::new_dynamic(
                (*command_buffer).clone(),
                self.resources.clone(),
                pass.extent,
            ))
        };

        // Execute pre-rendering callback (for custom barriers BEFORE begin_rendering)
        // This is needed because pipeline barriers with image transitions cannot be called
        // inside a render pass (VUID-vkCmdPipelineBarrier2-None-09553)
        pass.pre_execute(ctx.clone(), &mut self.registry);

        // Begin dynamic rendering
        command_buffer.begin_rendering(rendering_info);

        // Set viewport and scissor for this pass
        let viewport = Viewport {
            x: 0.0,
            y: pass.extent.height as f32,
            width: pass.extent.width as f32,
            height: -(pass.extent.height as f32),
            min_depth: 0.0,
            max_depth: 1.0,
        };

        let scissor = Rect2D {
            offset: Offset2D { x: 0, y: 0 },
            extent: pass.extent,
        };

        command_buffer.set_viewport(&[viewport]);
        command_buffer.set_scissor(&[scissor]);

        // Execute pass-specific commands (ctx was created before begin_rendering for pre_execute)
        pass.execute(ctx, &mut self.registry);

        // End dynamic rendering
        command_buffer.end_rendering();

        // Note: Swapchain transition to PRESENT_SRC is handled separately after all passes complete
        // The present_pass uses transfer (blit), not color attachment, so swapchain is never
        // in COLOR_ATTACHMENT_OPTIMAL layout

        Ok(())
    }

    /// Execute a pass using legacy render passes.
    ///
    /// NOTE: This method now uses dynamic rendering internally to avoid deprecated APIs.
    fn execute_pass_legacy(
        &mut self,
        command_buffer: &mut CommandBuffer,
        pass_index: usize,
        _image_index: usize,
    ) -> Result<(), RenderGraphError> {
        let pass = &self.passes[pass_index];

        // Use dynamic rendering (Vulkan 1.3) instead of legacy render pass
        // Build rendering info from framebuffer attachments if available
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: pass.extent.into(),
        };

        let mut rendering_info = RenderingInfo::new().render_area(render_area).layer_count(1);

        // Add color attachments with clear values
        let color_clears: Vec<_> = pass
            .clear_values
            .iter()
            .filter(|cv| matches!(cv, ClearValue::Color(_)))
            .collect();

        for (i, (image_view, _resource_id)) in pass.color_attachments.iter().enumerate() {
            let mut attachment = RenderingAttachmentInfo::new(*image_view)
                .layout(ImageLayout::ColorAttachmentOptimal);

            if let Some(&cv) = color_clears.get(i) {
                attachment = attachment.clear(*cv);
            }

            rendering_info = rendering_info.add_color_attachment(attachment);
        }

        // Add depth attachment if available
        if let Some((depth_view, _resource_id)) = pass.depth_attachment {
            let depth_clear = pass
                .clear_values
                .iter()
                .find(|cv| matches!(cv, ClearValue::DepthStencil(_)));

            let mut attachment = RenderingAttachmentInfo::new(depth_view)
                .layout(ImageLayout::DepthStencilAttachmentOptimal);

            if let Some(&cv) = depth_clear {
                attachment = attachment.clear(cv);
            }

            rendering_info = rendering_info.depth_attachment(attachment);
        }

        // Begin dynamic rendering
        command_buffer.begin_rendering(rendering_info);

        // Set viewport and scissor for this pass
        let viewport = Viewport {
            x: 0.0,
            y: 0.0,
            width: pass.extent.width as f32,
            height: pass.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        let scissor = Rect2D {
            offset: Offset2D { x: 0, y: 0 },
            extent: pass.extent,
        };

        command_buffer.set_viewport(&[viewport]);
        command_buffer.set_scissor(&[scissor]);

        // Create execution context with optional renderer context
        let ctx = if let Some(ref rc) = self.renderer_context {
            Rc::new(PassExecutionContext::with_renderer_context(
                (*command_buffer).clone(),
                self.resources.clone(),
                pass.extent,
                Rc::clone(rc),
            ))
        } else {
            Rc::new(PassExecutionContext::new_dynamic(
                (*command_buffer).clone(),
                self.resources.clone(),
                pass.extent,
            ))
        };

        // Execute pass-specific commands using ExecutionRegistry
        pass.execute(ctx, &mut self.registry);

        // End dynamic rendering
        command_buffer.end_rendering();

        Ok(())
    }
}

impl CompiledPass {
    /// Get the name of this pass.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Execute this pass using the provided ExecutionRegistry.
    pub fn execute(&self, ctx: Rc<PassExecutionContext>, registry: &mut ExecutionRegistry) {
        let name = self.execute.pass_name();
        if !name.is_empty() {
            registry.execute(name, ctx);
        }
    }

    /// Execute pre-rendering callback before begin_rendering().
    /// This is for custom pipeline barriers that must happen outside a render pass.
    pub fn pre_execute(&self, ctx: Rc<PassExecutionContext>, registry: &mut ExecutionRegistry) {
        if let Some(ref pre_exec) = self.pre_execute {
            let name = pre_exec.pass_name();
            if !name.is_empty() {
                registry.execute(name, ctx);
            }
        }
    }
}

impl Drop for CompiledRenderGraph {
    fn drop(&mut self) {
        unsafe {
            // Destroy all framebuffers from all passes
            for pass in &self.passes {
                for framebuffer in &pass.vk_framebuffers {
                    self.context
                        .device
                        .destroy_framebuffer(framebuffer.vk(), None);
                }
            }
            // Clean up resources
            // Since we're in Drop and all PassExecutionContexts should be gone,
            // we try to extract the HashMap safely. If we're not the sole owner,
            // we skip cleanup to avoid double-free.
            if let Ok(resources) = Rc::try_unwrap(std::mem::replace(
                &mut self.resources,
                Rc::new(RefCell::new(HashMap::new())),
            )) {
                // We're the sole owner, safe to free resources
                for (_, resource) in resources.into_inner() {
                    match resource {
                        CompiledResource::Buffer {
                            buffer, allocation, ..
                        } => {
                            self.context.free_buffer(buffer.vk(), allocation);
                        }
                        CompiledResource::Image {
                            image,
                            image_view,
                            allocation,
                            ..
                        } => {
                            self.context.device.destroy_image_view(image_view.vk(), None);
                            self.context.free_image(image.vk(), allocation);
                        }
                        CompiledResource::ExternalBuffer { .. }
                        | CompiledResource::ExternalImage { .. } => {
                            // Don't destroy external resources
                        }
                    }
                }
            }
            // If we couldn't unwrap the Rc, other owners still exist
            // Don't free resources to avoid double-free
        }
    }
}

/// Helper function to check if a format is depth or stencil (wrapper type).
fn is_depth_or_stencil_format(format: crate::render_graph::types::ImageFormat) -> bool {
    matches!(
        format,
        crate::render_graph::types::ImageFormat::D16Unorm
            | crate::render_graph::types::ImageFormat::D32Sfloat
            | crate::render_graph::types::ImageFormat::D32SfloatS8Uint
            | crate::render_graph::types::ImageFormat::D24UnormS8Uint
    )
}

/// Helper function to check if a format is depth or stencil (raw vk type for internal use).
fn is_depth_or_stencil(format: vk::Format) -> bool {
    matches!(
        format,
        vk::Format::D16_UNORM
            | vk::Format::X8_D24_UNORM_PACK32
            | vk::Format::D32_SFLOAT
            | vk::Format::S8_UINT
            | vk::Format::D16_UNORM_S8_UINT
            | vk::Format::D24_UNORM_S8_UINT
            | vk::Format::D32_SFLOAT_S8_UINT
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pass::Attachment;
    use crate::render_graph::types::{
        Extent3D, ImageFormat, ImageLayout, ImageTiling, ImageUsage, SampleCount,
    };
    use crate::RenderGraphBuilder;

    #[test]
    fn test_render_pass_group_creation() {
        let group = RenderPassGroup {
            pass_indices: vec![0],
            attachments: vec![ResourceId(0)],
            subpasses: vec![SubpassDescriptor {
                pass_index: 0,
                input_attachments: Vec::new(),
                color_attachments: vec![(0, ResourceId(0))],
                depth_stencil: None,
                // resolve_attachments: Vec::new(), // TODO: Not yet implemented
                vk_input_refs: Vec::new(),
                vk_color_refs: Vec::new(),
                vk_depth_ref: None,
            }],
        };

        assert_eq!(group.pass_indices.len(), 1);
        assert_eq!(group.attachments.len(), 1);
        assert_eq!(group.subpasses.len(), 1);
    }

    #[test]
    fn test_end_to_end_render_graph_flow() {
        // This test demonstrates the complete render graph flow from
        // builder creation to compilation, showcasing the full API ergonomics.

        // Step 1: Create render graph builder (owns ExecutionRegistry internally)
        let mut builder = RenderGraphBuilder::new();

        // Step 2: Add resources (test resource creation and storage)
        let _depth_target = builder.add_resource(
            "depth_target",
            ResourceKind::Image {
                extent: Extent3D::new(1920, 1080, 1),
                format: ImageFormat::D32Sfloat,
                usage: vec![ImageUsage::DepthStencilAttachment],
                samples: SampleCount::Sample1,
                tiling: ImageTiling::Optimal,
                initial_layout: ImageLayout::Undefined,
                final_layout: ImageLayout::DepthStencilAttachmentOptimal,
            },
        );

        let _geometry_color = builder.add_resource(
            "geometry_color",
            ResourceKind::Image {
                extent: Extent3D::new(1920, 1080, 1),
                format: ImageFormat::B8G8R8A8Srgb,
                usage: vec![ImageUsage::ColorAttachment, ImageUsage::Sampled],
                samples: SampleCount::Sample1,
                tiling: ImageTiling::Optimal,
                initial_layout: ImageLayout::Undefined,
                final_layout: ImageLayout::ShaderReadOnlyOptimal,
            },
        );

        // Step 3: Add passes with hardcoded resource IDs to avoid capture issues
        // Note: In real usage, you would capture ResourceId by value (Copy trait)
        builder.add_pass("geometry_pass", |pass| {
            pass.write(Attachment::Color(ResourceId(1)))
                .write(Attachment::DepthStencil(ResourceId(0)))
                .clear_color(ResourceId(1), [0.1, 0.2, 0.3, 1.0])
                .clear_depth_stencil(ResourceId(0), 1.0, 0)
                .execute("geometry_pass", |ctx| {
                    // In a real scenario, this would record actual Vulkan commands
                    // For testing, we verify the closure signature is correct
                    let _ = ctx.command_buffer;
                    let _ = ctx.resources;
                    let _ = ctx.framebuffer;
                    let _ = ctx.extent;
                });
        });

        builder.add_pass("lighting_pass", |pass| {
            pass.read(ResourceId(1))
                .write(Attachment::Color(ResourceId(1)))
                .extent(1920, 1080)
                .execute("lighting_pass", |ctx| {
                    // Test that we can access resources
                    let _ = ctx.get_image(ResourceId(1));
                });
        });

        // Step 4: Verify builder state
        assert_eq!(builder.graph().passes.len(), 2);
        assert_eq!(builder.graph().resources.len(), 2);
        assert_eq!(builder.registry().len(), 2); // Two execution closures registered!

        // Step 5: Verify pass structures
        let geometry_pass = &builder.graph().passes[0];
        assert_eq!(geometry_pass.name(), "geometry_pass");
        assert_eq!(geometry_pass.outputs().len(), 2); // color + depth

        let lighting_pass = &builder.graph().passes[1];
        assert_eq!(lighting_pass.name(), "lighting_pass");
        assert_eq!(lighting_pass.inputs().len(), 1); // reads color
        assert_eq!(lighting_pass.outputs().len(), 1); // writes color

        // Step 6: Verify resource lifetimes
        assert!(geometry_pass.usages().iter().any(|u| {
            u.resource_id == ResourceId(1)
                && u.load_op == crate::render_graph::types::AttachmentLoadOp::Clear
        }));

        // The builder flow is complete and validated!
        // (Actual compilation requires VulkanContext, which isn't available in unit tests)
    }

    #[test]
    fn test_execution_registry_integration() {
        // This test verifies that ExecutionRegistry properly manages closures.

        let mut registry = ExecutionRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        // Register a closure
        registry.register("test_pass".to_string(), |_ctx| {
            // Execution logic would go here
        });

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        // Verify closure is stored and can be executed
        // (We can't actually execute without real Vulkan objects)
        assert!(registry.len() == 1);
    }
}
