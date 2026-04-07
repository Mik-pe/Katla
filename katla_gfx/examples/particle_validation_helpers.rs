//! Helper functions for particle validation — GPU compute execution and render path exercise.

use ash::vk;
use gpu_allocator::vulkan::{AllocationCreateDesc, AllocationScheme};
use katla_gfx::VulkanContext;
use katla_gfx::particles::{
    GlobalParticleSystem, PARTICLE_EMIT_WORKGROUP_SIZE, PARTICLE_SIMULATE_WORKGROUP_SIZE,
};
use katla_gfx::renderer::AssetRegistry;
use std::path::PathBuf;

/// GPU resources needed for render pass validation.
pub struct RenderValidationResources {
    /// 1x1 color attachment image (B8G8R8A8_SRGB).
    color_image: vk::Image,
    color_allocation: Option<gpu_allocator::vulkan::Allocation>,
    /// Color attachment image view.
    color_image_view: vk::ImageView,
    /// 1x1 depth attachment image (D32_SFLOAT_S8_UINT).
    depth_image: vk::Image,
    depth_allocation: Option<gpu_allocator::vulkan::Allocation>,
    /// Depth attachment image view.
    depth_image_view: vk::ImageView,
    /// Dummy FrameUniforms storage buffer (256 bytes).
    frame_uniforms_buffer: vk::Buffer,
    frame_uniforms_allocation: Option<gpu_allocator::vulkan::Allocation>,
    /// Descriptor set for Set 1 (2 STORAGE_BUFFER bindings).
    storage_descriptor_set: vk::DescriptorSet,
    /// Descriptor pool for Set 1.
    storage_descriptor_pool: vk::DescriptorPool,
    /// Descriptor set layout for Set 1.
    storage_descriptor_layout: vk::DescriptorSetLayout,
}

impl RenderValidationResources {
    /// Create all GPU resources needed for render pass validation (1x1 offscreen images, dummy FrameUniforms).
    pub fn new(context: &VulkanContext) -> Result<Self, String> {
        let device = &context.device;

        // 1x1 color attachment (B8G8R8A8_SRGB)
        let color_image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::B8G8R8A8_SRGB)
            .extent(vk::Extent3D {
                width: 1,
                height: 1,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let (color_image, color_allocation) =
            context.create_image(color_image_info, gpu_allocator::MemoryLocation::GpuOnly);

        // Transition color image to COLOR_ATTACHMENT_OPTIMAL
        let cmd = context.begin_single_time_commands();
        let color_barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image(color_image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let dependency_info = vk::DependencyInfo::default()
            .image_memory_barriers(std::slice::from_ref(&color_barrier));
        unsafe {
            device.cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dependency_info);
        }
        context.end_single_time_commands(cmd);

        let color_image_view = unsafe {
            device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(color_image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::B8G8R8A8_SRGB)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )
                .map_err(|e| format!("Failed to create color image view: {:?}", e))?
        };

        // 1x1 depth attachment (D32_SFLOAT_S8_UINT)
        let depth_image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::D32_SFLOAT_S8_UINT)
            .extent(vk::Extent3D {
                width: 1,
                height: 1,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let (depth_image, depth_allocation) =
            context.create_image(depth_image_info, gpu_allocator::MemoryLocation::GpuOnly);

        // Transition depth image to DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        let cmd = context.begin_single_time_commands();
        let depth_barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS)
            .dst_access_mask(vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .image(depth_image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let dependency_info = vk::DependencyInfo::default()
            .image_memory_barriers(std::slice::from_ref(&depth_barrier));
        unsafe {
            device.cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dependency_info);
        }
        context.end_single_time_commands(cmd);

        let depth_image_view = unsafe {
            device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(depth_image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::D32_SFLOAT_S8_UINT)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::DEPTH
                                | vk::ImageAspectFlags::STENCIL,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )
                .map_err(|e| format!("Failed to create depth image view: {:?}", e))?
        };

        // Dummy FrameUniforms buffer (256 bytes)
        const FRAME_UNIFORMS_SIZE: u64 = 256;
        let buffer_info = vk::BufferCreateInfo::default()
            .size(FRAME_UNIFORMS_SIZE)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let frame_uniforms_buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .map_err(|e| format!("Failed to create frame uniforms buffer: {:?}", e))?
        };

        let requirements = unsafe { device.get_buffer_memory_requirements(frame_uniforms_buffer) };

        let frame_uniforms_allocation = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "validation_frame_uniforms",
                requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate frame uniforms memory: {}", e))?;

        unsafe {
            device
                .bind_buffer_memory(
                    frame_uniforms_buffer,
                    frame_uniforms_allocation.memory(),
                    frame_uniforms_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind frame uniforms buffer: {:?}", e))?;
        }

        // Fill with identity view/proj matrices
        unsafe {
            let ptr = frame_uniforms_allocation.mapped_ptr().unwrap().as_ptr() as *mut f32;

            // Identity view matrix (column-major)
            let identity: [f32; 16] = [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ];

            // View matrix (offset 0)
            std::ptr::copy_nonoverlapping(identity.as_ptr(), ptr, 16);
            // Proj matrix (offset 64 bytes)
            std::ptr::copy_nonoverlapping(identity.as_ptr(), ptr.add(16), 16);
            // Inv view-proj matrix (offset 128 bytes)
            std::ptr::copy_nonoverlapping(identity.as_ptr(), ptr.add(32), 16);

            context.flush_mapped_memory(&frame_uniforms_allocation, 0, FRAME_UNIFORMS_SIZE);
        }

        // --- Create descriptor set layout for Set 1 (2 STORAGE_BUFFER bindings) ---
        let storage_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];

        let storage_descriptor_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&storage_bindings),
                    None,
                )
                .map_err(|e| format!("Failed to create storage descriptor layout: {:?}", e))?
        };

        // --- Create descriptor pool and allocate descriptor set ---
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(2)];

        let storage_descriptor_pool = unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .pool_sizes(&pool_sizes)
                        .max_sets(1),
                    None,
                )
                .map_err(|e| format!("Failed to create storage descriptor pool: {:?}", e))?
        };

        let descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(storage_descriptor_pool)
                        .set_layouts(std::slice::from_ref(&storage_descriptor_layout)),
                )
                .map_err(|e| format!("Failed to allocate storage descriptor set: {:?}", e))?
        };
        let storage_descriptor_set = descriptor_sets[0];

        // Binding 0: FrameUniforms (offset 0, 256 bytes)
        let binding0_info = [vk::DescriptorBufferInfo::default()
            .buffer(frame_uniforms_buffer)
            .offset(0)
            .range(FRAME_UNIFORMS_SIZE)];

        // Binding 1: Same buffer at offset 0 (dummy, not actually read by particle shader)
        let binding1_info = [vk::DescriptorBufferInfo::default()
            .buffer(frame_uniforms_buffer)
            .offset(0)
            .range(FRAME_UNIFORMS_SIZE)];

        let descriptor_writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(storage_descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&binding0_info),
            vk::WriteDescriptorSet::default()
                .dst_set(storage_descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&binding1_info),
        ];

        unsafe {
            device.update_descriptor_sets(&descriptor_writes, &[]);
        }

        log::info!("Render validation resources created successfully");
        Ok(Self {
            color_image,
            color_allocation: Some(color_allocation),
            color_image_view,
            depth_image,
            depth_allocation: Some(depth_allocation),
            depth_image_view,
            frame_uniforms_buffer,
            frame_uniforms_allocation: Some(frame_uniforms_allocation),
            storage_descriptor_set,
            storage_descriptor_pool,
            storage_descriptor_layout,
        })
    }

    /// Clean up all GPU resources.
    pub fn destroy(&mut self, context: &VulkanContext) {
        let device = &context.device;

        unsafe {
            device.destroy_descriptor_pool(self.storage_descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.storage_descriptor_layout, None);
            device.destroy_image_view(self.color_image_view, None);
            device.destroy_image_view(self.depth_image_view, None);
            device.destroy_buffer(self.frame_uniforms_buffer, None);
        }

        if let Some(alloc) = self.color_allocation.take() {
            context.allocator.free(alloc, "particle validation color");
        }
        if let Some(alloc) = self.depth_allocation.take() {
            context.allocator.free(alloc, "particle validation depth");
        }
        if let Some(alloc) = self.frame_uniforms_allocation.take() {
            context
                .allocator
                .free(alloc, "particle validation frame uniforms");
        }

        unsafe {
            device.destroy_image(self.color_image, None);
            device.destroy_image(self.depth_image, None);
        }

        log::info!("Render validation resources destroyed");
    }
}

/// Record a render dispatch using dynamic rendering with 1x1 color + depth attachments.

pub fn record_render_dispatch(
    context: &VulkanContext,
    particle_system: &mut GlobalParticleSystem,
    asset_registry: &AssetRegistry,
    cmd: vk::CommandBuffer,
    render_resources: &RenderValidationResources,
    frame_index: usize,
) -> Result<(), String> {
    let device = &context.device;

    let pipeline_handle = particle_system
        .render_pipeline_handle()
        .ok_or("Render pipeline not created")?;

    let pipeline_asset = asset_registry
        .get_pipeline(pipeline_handle)
        .ok_or("Render pipeline not found in registry")?;

    let vk_pipeline = pipeline_asset.vk_pipeline();
    let vk_layout = pipeline_asset.vk_layout();

    // Pipeline barrier: COMPUTE_SHADER -> VERTEX_SHADER (for particle buffer reads)
    let compute_to_vertex_barrier = vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::VERTEX_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(particle_system.particle_buffer())
        .offset(0)
        .size(vk::WHOLE_SIZE);

    // Pipeline barrier: COMPUTE_SHADER -> DRAW_INDIRECT (for indirect draw command buffer)
    let compute_to_indirect_barrier = vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::DRAW_INDIRECT)
        .dst_access_mask(vk::AccessFlags2::INDIRECT_COMMAND_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(particle_system.indirect_draw_buffer(frame_index))
        .offset(0)
        .size(16);

    let barriers = [compute_to_vertex_barrier, compute_to_indirect_barrier];

    let dependency_info = vk::DependencyInfo::default().buffer_memory_barriers(&barriers);

    unsafe {
        device.cmd_pipeline_barrier2(cmd, &dependency_info);
    }

    // Begin dynamic rendering with color + depth attachments
    let color_attachment = vk::RenderingAttachmentInfo::default()
        .image_view(render_resources.color_image_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue { float32: [0.0; 4] },
        });

    let depth_attachment = vk::RenderingAttachmentInfo::default()
        .image_view(render_resources.depth_image_view)
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .clear_value(vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 0.0,
                stencil: 0,
            },
        });

    let rendering_info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: 1,
                height: 1,
            },
        })
        .layer_count(1)
        .color_attachments(std::slice::from_ref(&color_attachment))
        .depth_attachment(&depth_attachment);

    unsafe {
        device.cmd_begin_rendering(cmd, &rendering_info);
    }

    // Set 1x1 viewport and scissor
    let viewport = vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let scissor = vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent: vk::Extent2D {
            width: 1,
            height: 1,
        },
    };

    unsafe {
        device.cmd_set_viewport(cmd, 0, std::slice::from_ref(&viewport));
        device.cmd_set_scissor(cmd, 0, std::slice::from_ref(&scissor));
    }

    // Call particle system render (binds pipeline, descriptor sets, and issues vkCmdDraw)
    particle_system.render(
        cmd,
        vk::RenderPass::null(),
        vk_pipeline,
        vk_layout,
        render_resources.storage_descriptor_set,
        frame_index,
    )?;

    // End dynamic rendering
    unsafe {
        device.cmd_end_rendering(cmd);
    }

    log::debug!(
        "Render dispatch recorded: {} alive particles ({} vertices)",
        particle_system.alive_count(),
        particle_system.alive_count() * 6
    );
    Ok(())
}

/// Load particle compute shaders and create pipelines (including render pipeline).
pub fn load_and_create_pipelines(
    context: &VulkanContext,
    particle_system: &mut GlobalParticleSystem,
    asset_registry: &mut AssetRegistry,
    shader_dir: &PathBuf,
) -> Result<(), String> {
    use katla_gfx::ShaderCache;
    use katla_gfx::sync::VkShaderModule;

    let mut shader_cache = ShaderCache::new(context.device.clone());

    // Load emit shader
    let emit_shader_path = shader_dir.join("particles/particle_emit.wgsl");
    log::info!("Loading emit shader from: {:?}", emit_shader_path);

    let emit_shader = shader_cache
        .load_shader(&emit_shader_path, vk::ShaderStageFlags::COMPUTE)
        .map_err(|e| format!("Failed to load emit shader: {}", e))?;

    let emit_shader_wrapper = VkShaderModule(emit_shader);
    particle_system
        .create_emit_pipeline(asset_registry, emit_shader_wrapper)
        .map_err(|e| format!("Failed to create emit pipeline: {}", e))?;

    log::info!("Emit pipeline created successfully");

    // Load simulate shader
    let simulate_shader_path = shader_dir.join("particles/particle_simulate.wgsl");
    log::info!("Loading simulate shader from: {:?}", simulate_shader_path);

    let simulate_shader = shader_cache
        .load_shader(&simulate_shader_path, vk::ShaderStageFlags::COMPUTE)
        .map_err(|e| format!("Failed to load simulate shader: {}", e))?;

    let simulate_shader_wrapper = VkShaderModule(simulate_shader);
    particle_system
        .create_simulate_pipeline(asset_registry, simulate_shader_wrapper)
        .map_err(|e| format!("Failed to create simulate pipeline: {}", e))?;

    log::info!("Simulate pipeline created successfully");

    // Load draw command shader
    let draw_cmd_shader_path = shader_dir.join("particles/particle_draw_command.wgsl");
    log::info!(
        "Loading draw command shader from: {:?}",
        draw_cmd_shader_path
    );

    let draw_cmd_shader = shader_cache
        .load_shader(&draw_cmd_shader_path, vk::ShaderStageFlags::COMPUTE)
        .map_err(|e| format!("Failed to load draw command shader: {}", e))?;

    particle_system
        .create_draw_command_pipeline(asset_registry, VkShaderModule(draw_cmd_shader))
        .map_err(|e| format!("Failed to create draw command pipeline: {}", e))?;

    log::info!("Draw command pipeline created successfully");

    // Load render shaders (vertex + fragment from same WGSL file)
    let render_shader_path = shader_dir.join("particles/particle_render.wgsl");
    log::info!("Loading render shader from: {:?}", render_shader_path);

    let vertex_shader = shader_cache
        .load_shader(&render_shader_path, vk::ShaderStageFlags::VERTEX)
        .map_err(|e| format!("Failed to load particle vertex shader: {}", e))?;

    let fragment_shader = shader_cache
        .load_shader(&render_shader_path, vk::ShaderStageFlags::FRAGMENT)
        .map_err(|e| format!("Failed to load particle fragment shader: {}", e))?;

    particle_system
        .create_render_pipeline(
            asset_registry,
            VkShaderModule(vertex_shader),
            VkShaderModule(fragment_shader),
        )
        .map_err(|e| format!("Failed to create render pipeline: {}", e))?;

    log::info!("Render pipeline created successfully");

    Ok(())
}

/// Find the shader directory by checking common locations.
pub fn find_shader_directory() -> PathBuf {
    let possible_paths = vec![
        PathBuf::from("resources/shaders"),
        PathBuf::from("../resources/shaders"),
        PathBuf::from("../../resources/shaders"),
        PathBuf::from("../../../resources/shaders"),
    ];

    for path in possible_paths {
        if path.exists() {
            log::info!("Found shader directory at: {:?}", path);
            return path;
        }
    }

    // Default to resources/shaders even if it doesn't exist
    // The error will be clearer when shader loading fails
    log::warn!("Could not find shader directory, defaulting to resources/shaders");
    PathBuf::from("resources/shaders")
}
