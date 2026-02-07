use ash::vk;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::render_graph::pass::{ExecutionRegistry, Pass, PassExecute, PassExecutionContext};
use crate::render_graph::resource::{CompiledResource, ResourceId, ResourceKind, ResourceLifetime};
use crate::render_graph::types::{ClearValue, Extent2D};
use crate::rendering::DrawList;
use crate::sync::{VkFramebuffer, VkImage, VkImageView, VkRenderPass};
use crate::vulkan::RenderPass;
use crate::CommandBuffer;
use crate::RenderGraphError;
use crate::VulkanContext;

/// CompiledRenderGraph represents a fully compiled render graph with all Vulkan objects created.
/// This is the result of the compilation process and can be executed each frame.
pub struct CompiledRenderGraph {
    pub context: Rc<VulkanContext>,
    pub passes: Vec<CompiledPass>,
    pub resources: Rc<HashMap<ResourceId, CompiledResource>>,
    vk_render_passes: Vec<vk::RenderPass>,
    framebuffers: Vec<vk::Framebuffer>,
    pub registry: ExecutionRegistry<'static>,
    /// Cell for storing the draw list that will be processed during execution.
    /// This is set each frame before calling execute().
    draw_list_cell: Option<Rc<RefCell<Option<DrawList>>>>,
}

/// CompiledPass represents a single compiled pass with all necessary Vulkan objects.
/// The execute field now contains the pass name for looking up the closure in the ExecutionRegistry.
/// Multiple framebuffers are supported (e.g., one per swapchain image).
pub struct CompiledPass {
    pub name: String,
    pub vk_render_pass: VkRenderPass,
    /// The render pass to use for rendering (may differ from the compilation render pass)
    pub active_render_pass: VkRenderPass,
    /// Multiple framebuffers - one per swapchain image variant
    pub vk_framebuffers: Vec<VkFramebuffer>,
    pub extent: Extent2D,
    pub clear_values: Vec<ClearValue>,
    execute: PassExecute,
    pub pipeline_barriers_before: Vec<vk::MemoryBarrier<'static>>,
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
    #[allow(dead_code)]
    resolve_attachments: Vec<(u32, ResourceId)>,
    // Store Vulkan attachment references to ensure they live long enough
    vk_input_refs: Vec<vk::AttachmentReference>,
    vk_color_refs: Vec<vk::AttachmentReference>,
    vk_depth_ref: Option<vk::AttachmentReference>,
}

impl CompiledRenderGraph {
    /// Create multiple framebuffers for passes that use external images.
    /// This is useful for swapchain rendering where you need one framebuffer per swapchain image.
    /// Returns an error if the graph has already been compiled with framebuffers.
    pub fn create_swapchain_framebuffers(
        &mut self,
        swapchain_images: &[(VkImage, VkImageView, Extent2D, vk::Format)],
        immediate_render_pass: VkRenderPass,
    ) -> Result<(), RenderGraphError> {
        // Find the depth image view before the loop
        let mut depth_image_view: Option<vk::ImageView> = None;
        for (_, resource) in self.resources.iter() {
            if let CompiledResource::ExternalImage {
                format, image_view, ..
            } = resource
            {
                if is_depth_or_stencil(*format) {
                    depth_image_view = Some(*image_view);
                    break;
                }
            }
        }

        let depth_view = depth_image_view.ok_or_else(|| {
            RenderGraphError::CompilationError("No depth image view found".into())
        })?;

        // For each swapchain image, we need to create new framebuffers for each pass
        // that uses external images
        for (image_index, (_vk_image, image_view, extent, _format)) in
            swapchain_images.iter().enumerate()
        {
            // Recreate framebuffers for all passes with this swapchain image
            for pass_idx in 0..self.passes.len() {
                let framebuffer = self.create_framebuffer_for_pass(
                    pass_idx,
                    image_view.vk(),
                    depth_view,
                    (*extent).into(),
                    immediate_render_pass.vk(),
                )?;
                if image_index == 0 {
                    // First framebuffer - replace the null placeholder
                    self.framebuffers[pass_idx] = framebuffer;
                    self.passes[pass_idx].vk_framebuffers = vec![VkFramebuffer::new(framebuffer)];
                    // Set the active render pass to the immediate-mode render pass
                    self.passes[pass_idx].active_render_pass = immediate_render_pass;
                } else {
                    // Additional framebuffers - append to the list
                    self.passes[pass_idx].vk_framebuffers.push(VkFramebuffer::new(framebuffer));
                }
            }
        }

        Ok(())
    }

    /// Create a framebuffer for a specific pass with the given swapchain image view.
    fn create_framebuffer_for_pass(
        &self,
        pass_index: usize,
        swapchain_image_view: vk::ImageView,
        depth_image_view: vk::ImageView,
        swapchain_extent: vk::Extent2D,
        immediate_render_pass: vk::RenderPass,
    ) -> Result<vk::Framebuffer, RenderGraphError> {
        let pass = self.passes.get(pass_index).ok_or_else(|| {
            RenderGraphError::CompilationError(format!("Pass {} not found", pass_index))
        })?;

        // For now, we know the attachment order: color (swapchain), then depth
        // In the future, this should be determined from the pass descriptor
        let attachment_views = vec![swapchain_image_view, depth_image_view];

        println!("Creating framebuffer for pass {}: graph_render_pass={:?}, using render_pass={:?}, attachments={:?}, extent={}x{}",
            pass_index, pass.vk_render_pass.vk(), immediate_render_pass, attachment_views, swapchain_extent.width, swapchain_extent.height);

        // Create framebuffer using the immediate-mode render pass
        let framebuffer = self
            .context
            .create_framebuffer(immediate_render_pass, &attachment_views, swapchain_extent)
            .map_err(RenderGraphError::VulkanError)?;

        println!("  Created framebuffer: {:?}", framebuffer);

        Ok(framebuffer)
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

        // Step 3: Generate Vulkan render passes
        let vk_render_passes = Self::generate_render_passes(&pass_structure, &graph, context)?;

        // Step 4: Allocate resources
        let resources = Self::allocate_resources(&graph, &resource_lifetimes, context)?;

        // Step 5: Create framebuffers
        let framebuffers =
            Self::create_framebuffers(&pass_structure, &vk_render_passes, &resources, context)?;
        // For now, use empty barriers as placeholder
        let barriers: Vec<Vec<vk::MemoryBarrier<'static>>> = vec![];

        // Step 7: Compile passes with execution info
        let compiled_passes = Self::compile_passes(
            &mut graph.passes,
            &vk_render_passes,
            &framebuffers,
            &resources,
            &barriers,
        )?;

        Ok(Self {
            context: context.clone(),
            passes: compiled_passes,
            resources: Rc::new(resources),
            vk_render_passes,
            framebuffers,
            registry,
            draw_list_cell: None,
        })
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
                    resolve_attachments: Vec::new(),
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
                    == vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
                    || usage.layout == vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;

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
                        let is_depth = is_depth_or_stencil(*format);
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
        context: &Rc<VulkanContext>,
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
                        // Find the usage for this attachment
                        let mut load_op = vk::AttachmentLoadOp::DONT_CARE;
                        let mut store_op = vk::AttachmentStoreOp::DONT_CARE;

                        // Look at the first pass that uses this resource as an output
                        for pass_idx in &group.pass_indices {
                            if let Some(pass) = graph.passes.get(*pass_idx) {
                                for usage in pass.usages() {
                                    if usage.resource_id == *resource_id {
                                        load_op = usage.load_op;
                                        store_op = usage.store_op;
                                        break;
                                    }
                                }
                            }
                        }

                        let (_final_layout_stencil, stencil_load_op, stencil_store_op) = {
                            if is_depth_or_stencil(*format) {
                                (
                                    *final_layout,
                                    vk::AttachmentLoadOp::DONT_CARE,
                                    vk::AttachmentStoreOp::DONT_CARE,
                                )
                            } else {
                                (vk::ImageLayout::UNDEFINED, load_op, store_op)
                            }
                        };

                        vk::AttachmentDescription::default()
                            .format(*format)
                            .samples(*samples)
                            .load_op(load_op)
                            .store_op(store_op)
                            .stencil_load_op(stencil_load_op)
                            .stencil_store_op(stencil_store_op)
                            .initial_layout(*initial_layout)
                            .final_layout(*final_layout)
                    }
                    ResourceKind::ExternalImage { format, .. } => {
                        // Find the usage for this attachment (same as Image case)
                        let mut load_op = vk::AttachmentLoadOp::DONT_CARE;
                        let mut store_op = vk::AttachmentStoreOp::DONT_CARE;

                        // Look at the first pass that uses this resource as an output
                        for pass_idx in &group.pass_indices {
                            if let Some(pass) = graph.passes.get(*pass_idx) {
                                for usage in pass.usages() {
                                    if usage.resource_id == *resource_id {
                                        load_op = usage.load_op;
                                        store_op = usage.store_op;
                                        break;
                                    }
                                }
                            }
                        }

                        // Determine final layout based on format (swapchain vs depth)
                        let is_depth = is_depth_or_stencil(*format);
                        let final_layout = if is_depth {
                            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
                        } else {
                            vk::ImageLayout::PRESENT_SRC_KHR
                        };

                        println!("ExternalImage attachment: format={:?}, load_op={:?}, store_op={:?}, final_layout={:?}",
                            format, load_op, store_op, final_layout);

                        vk::AttachmentDescription::default()
                            .format(*format)
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

            // Create render pass using RenderPass wrapper
            let render_pass = RenderPass::create_from_config(
                context.device.clone(),
                &attachments,
                &subpasses,
                &dependencies,
            )?;

            render_passes.push(render_pass.get_vk_renderpass());
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
                ResourceKind::ExternalBuffer { vk_buffer } => {
                    resources.insert(
                        *resource_id,
                        CompiledResource::ExternalBuffer { buffer: *vk_buffer },
                    );
                }
                ResourceKind::ExternalImage {
                    vk_image,
                    image_view,
                    format,
                    extent,
                    ..
                } => {
                    resources.insert(
                        *resource_id,
                        CompiledResource::ExternalImage {
                            image: *vk_image,
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
                    let buffer_info = vk::BufferCreateInfo::default()
                        .size(*size)
                        .usage(*usage)
                        .sharing_mode(vk::SharingMode::EXCLUSIVE);

                    let location =
                        if memory_properties.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL) {
                            gpu_allocator::MemoryLocation::GpuOnly
                        } else {
                            gpu_allocator::MemoryLocation::CpuToGpu
                        };

                    let (buffer, allocation) = context.allocate_buffer(&buffer_info, location);

                    resources.insert(
                        *resource_id,
                        CompiledResource::Buffer {
                            buffer,
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
                    let image_info = vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .extent(*extent)
                        .mip_levels(1)
                        .array_layers(1)
                        .format(*format)
                        .tiling(*tiling)
                        .initial_layout(vk::ImageLayout::UNDEFINED)
                        .usage(*usage)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .sharing_mode(vk::SharingMode::EXCLUSIVE);

                    let location = gpu_allocator::MemoryLocation::GpuOnly;
                    let (image, allocation) = context.create_image(image_info, location);

                    // Create image view
                    // Determine aspect mask based on format (depth/stencil vs color)
                    let aspect_mask = if is_depth_or_stencil(*format) {
                        vk::ImageAspectFlags::DEPTH
                    } else {
                        vk::ImageAspectFlags::COLOR
                    };
                    let image_view = context.create_image_view(image, *format, aspect_mask);

                    resources.insert(
                        *resource_id,
                        CompiledResource::Image {
                            image,
                            image_view,
                            allocation,
                            extent: *extent,
                            format: *format,
                            layout: vk::ImageLayout::UNDEFINED,
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
        vk_render_passes: &[vk::RenderPass],
        resources: &HashMap<ResourceId, CompiledResource>,
        context: &Rc<VulkanContext>,
    ) -> Result<Vec<vk::Framebuffer>, RenderGraphError> {
        let mut framebuffers = Vec::new();

        for (i, group) in groups.iter().enumerate() {
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
            if uses_external {
                framebuffers.push(vk::Framebuffer::null());
                continue;
            }

            // Get render pass for this framebuffer
            let render_pass = vk_render_passes.get(i).copied().ok_or_else(|| {
                RenderGraphError::CompilationError(format!(
                    "No render pass found for group at index {}",
                    i
                ))
            })?;

            // Get extent from the first image attachment
            let extent = match group.attachments.first() {
                Some(resource_id) => match resources.get(resource_id) {
                    Some(CompiledResource::Image { extent, .. }) => vk::Extent2D {
                        width: extent.width,
                        height: extent.height,
                    },
                    Some(CompiledResource::ExternalImage { extent, .. }) => *extent,
                    _ => {
                        return Err(RenderGraphError::CompilationError(format!(
                            "No valid extent for resource {:?}",
                            resource_id
                        )));
                    }
                },
                None => {
                    return Err(RenderGraphError::CompilationError(
                        "No attachments in render pass - cannot determine extent".into(),
                    ));
                }
            };

            // Collect image views for framebuffer
            let attachment_views: Vec<vk::ImageView> = group
                .attachments
                .iter()
                .map(|resource_id| match resources.get(resource_id) {
                    Some(CompiledResource::Image { image_view, .. }) => Ok(*image_view),
                    Some(CompiledResource::ExternalImage { image_view, .. }) => Ok(*image_view),
                    _ => Err(RenderGraphError::CompilationError(format!(
                        "Resource {:?} is not an image",
                        resource_id
                    ))),
                })
                .collect::<Result<_, _>>()?;

            // Create framebuffer using VulkanContext wrapper
            #[allow(clippy::redundant_closure)]
            let framebuffer = context
                .create_framebuffer(render_pass, &attachment_views, extent)
                .map_err(|e| RenderGraphError::VulkanError(e))?;

            framebuffers.push(framebuffer);
        }

        Ok(framebuffers)
    }

    /// Calculate memory barriers between passes.
    /// Analyzes resource usage between consecutive passes and creates
    /// appropriate synchronization barriers to ensure correct memory access.
    ///
    /// NOTE: Currently returns empty barriers. For multi-pass graphs with
    /// complex dependencies, proper barrier calculation based on resource
    /// usage should be implemented.
    #[allow(dead_code)]
    fn calculate_barriers(
        graph: &crate::RenderGraph,
        _lifetimes: &HashMap<ResourceId, ResourceLifetime>,
    ) -> Vec<Vec<vk::MemoryBarrier<'static>>> {
        // Placeholder implementation - returns empty barriers
        let _ = (graph, _lifetimes);
        vec![]
    }

    /// Compile passes with execution info and barriers.
    fn compile_passes(
        passes: &mut [Pass],
        vk_render_passes: &[vk::RenderPass],
        framebuffers: &[vk::Framebuffer],
        resources: &HashMap<ResourceId, CompiledResource>,
        barriers: &[Vec<vk::MemoryBarrier<'static>>],
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
                    (*extent).into()
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

            // Get render pass and framebuffer
            let vk_render_pass_raw = vk_render_passes
                .get(i)
                .copied()
                .unwrap_or(vk::RenderPass::null());
            let vk_framebuffer_raw = framebuffers
                .get(i)
                .copied()
                .unwrap_or(vk::Framebuffer::null());

            // Wrap in our wrapper types
            let vk_render_pass = VkRenderPass::new(vk_render_pass_raw);
            let active_render_pass = VkRenderPass::new(vk_render_pass_raw);
            let vk_framebuffers = vec![VkFramebuffer::new(vk_framebuffer_raw)];

            // Get barriers
            let pipeline_barriers_before = barriers.get(i).cloned().unwrap_or_default();

            // Get execute name from the pass
            let execute_name = pass.take_execute_name();
            let execute = PassExecute::new(execute_name);

            let compiled = CompiledPass {
                name: pass.name().to_string(),
                vk_render_pass,
                active_render_pass, // Initially same as vk_render_pass
                vk_framebuffers,
                extent,
                clear_values,
                execute,
                pipeline_barriers_before,
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

    /// Execute the compiled render graph.
    /// Executes all passes in order using the provided command buffer.
    /// The ExecutionRegistry (owned by this graph) provides the closure logic.
    /// The image_index selects which framebuffer to use for each pass (for swapchain images).
    pub fn execute(
        &mut self,
        command_buffer: &mut CommandBuffer,
        image_index: usize,
    ) -> Result<(), RenderGraphError> {
        for pass in &self.passes {
            // Select the correct framebuffer for this image index
            let framebuffer = pass
                .vk_framebuffers
                .get(image_index)
                .or_else(|| pass.vk_framebuffers.first())
                .map(|fb| fb.vk())
                .unwrap_or(vk::Framebuffer::null());

            // Apply pipeline barriers before this pass
            if !pass.pipeline_barriers_before.is_empty() {
                // Determine stage masks - use ALL_COMMANDS as conservative default
                // In the future, we could store stage masks in CompiledPass for better precision
                let src_stage_mask = vk::PipelineStageFlags::ALL_COMMANDS;
                let dst_stage_mask = vk::PipelineStageFlags::ALL_COMMANDS;

                command_buffer.pipeline_barrier(
                    src_stage_mask,
                    dst_stage_mask,
                    vk::DependencyFlags::empty(),
                    &pass.pipeline_barriers_before,
                    &[],
                    &[],
                );
            }

            // Begin render pass
            let render_area = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: pass.extent.into(),
            };
            let clear_values_vk: Vec<vk::ClearValue> = pass.clear_values.iter().map(|cv| (*cv).into()).collect();
            command_buffer.begin_render_pass(
                framebuffer,
                pass.active_render_pass.vk(),
                render_area,
                &clear_values_vk,
            );

            // Create execution context with Rc-wrapped command buffer
            // Clone the CommandBuffer to allow sharing with user closures
            let ctx = Rc::new(PassExecutionContext::new(
                (*command_buffer).clone(),
                self.resources.clone(),
                framebuffer,
                pass.active_render_pass.vk(),
                pass.extent,
            ));

            // Execute pass-specific commands using ExecutionRegistry
            pass.execute(ctx, &mut self.registry);

            // End render pass
            command_buffer.end_render_pass();
        }
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
}

impl Drop for CompiledRenderGraph {
    fn drop(&mut self) {
        unsafe {
            // Destroy all framebuffers from all passes
            for pass in &self.passes {
                for framebuffer in &pass.vk_framebuffers {
                    self.context.device.destroy_framebuffer(framebuffer.vk(), None);
                }
            }
            for render_pass in &self.vk_render_passes {
                self.context.device.destroy_render_pass(*render_pass, None);
            }
            // Clean up resources
            // Since we're in Drop and all PassExecutionContexts should be gone,
            // we try to extract the HashMap safely. If we're not the sole owner,
            // we skip cleanup to avoid double-free.
            if let Ok(resources) = Rc::try_unwrap(std::mem::replace(&mut self.resources, Rc::new(HashMap::new()))) {
                // We're the sole owner, safe to free resources
                for (_, resource) in resources {
                    match resource {
                        CompiledResource::Buffer {
                            buffer, allocation, ..
                        } => {
                            self.context.free_buffer(buffer, allocation);
                        }
                        CompiledResource::Image {
                            image,
                            image_view,
                            allocation,
                            ..
                        } => {
                            self.context.device.destroy_image_view(image_view, None);
                            self.context.free_image(image, allocation);
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

/// Helper function to check if a format is depth or stencil.
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
                resolve_attachments: Vec::new(),
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
                extent: vk::Extent3D {
                    width: 1920,
                    height: 1080,
                    depth: 1,
                },
                format: vk::Format::D32_SFLOAT,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                samples: vk::SampleCountFlags::TYPE_1,
                tiling: vk::ImageTiling::OPTIMAL,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            },
        );

        let _geometry_color = builder.add_resource(
            "geometry_color",
            ResourceKind::Image {
                extent: vk::Extent3D {
                    width: 1920,
                    height: 1080,
                    depth: 1,
                },
                format: vk::Format::B8G8R8A8_SRGB,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                samples: vk::SampleCountFlags::TYPE_1,
                tiling: vk::ImageTiling::OPTIMAL,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
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
                    let _ = ctx.render_pass;
                    let _ = ctx.extent;
                });
        });

        builder.add_pass("lighting_pass", |pass| {
            pass.read(ResourceId(1))
                .write(Attachment::Color(ResourceId(1)))
                .extent(1920, 1080)
                .execute("lighting_pass", |ctx| {
                    // Test that we can access resources
                    let _ = ctx.get_resource(ResourceId(1));
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
            u.resource_id == ResourceId(1) && u.load_op == vk::AttachmentLoadOp::CLEAR
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
