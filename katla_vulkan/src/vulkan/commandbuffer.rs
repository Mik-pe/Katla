use ash::{vk, Device};

use super::{vertexbuffer::IndexType, CommandPool};
use crate::render_graph::types::RenderingInfo;
use crate::sync::DependencyInfo;

#[derive(Clone)]
pub struct CommandBuffer {
    device: Device,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
}

impl CommandBuffer {
    pub fn new(device: &Device, command_pool: &CommandPool) -> Self {
        let create_info = vk::CommandBufferAllocateInfo::default()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(command_pool.vk_command_pool())
            .command_buffer_count(1);
        let command_buffer: vk::CommandBuffer = unsafe {
            device
                .allocate_command_buffers(&create_info)
                .expect("Failed to allocate Vulkan command buffer - check device memory")
        }[0];

        Self {
            device: device.clone(),
            command_pool: command_pool.vk_command_pool(),
            command_buffer,
        }
    }

    pub fn vk_command_buffer(&self) -> vk::CommandBuffer {
        self.command_buffer
    }

    pub fn begin_single_time_command(&self) {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(self.command_buffer, &begin_info)
                .expect("Failed to begin command buffer - command buffer may be in invalid state");
        }
    }

    pub fn end_single_time_command(&self) {
        unsafe {
            self.device
                .end_command_buffer(self.command_buffer)
                .expect("Failed to end command buffer - command buffer may not be in recording state");
        }
    }

    pub fn begin_command(&self, flags: vk::CommandBufferUsageFlags) {
        let begin_info = vk::CommandBufferBeginInfo::default().flags(flags);
        unsafe {
            self.device
                .begin_command_buffer(self.command_buffer, &begin_info)
                .expect("Failed to begin command buffer - command buffer may be in invalid state");
        }
    }

    pub fn end_command(&self) {
        unsafe {
            self.device
                .end_command_buffer(self.command_buffer)
                .expect("Failed to end command buffer - command buffer may not be in recording state");
        }
    }

    pub fn begin_render_pass(
        &self,
        framebuffer: vk::Framebuffer,
        render_pass: vk::RenderPass,
        render_area: vk::Rect2D,
        clear_values: &[vk::ClearValue],
    ) {
        let begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(render_pass)
            .framebuffer(framebuffer)
            .render_area(render_area)
            .clear_values(clear_values);

        unsafe {
            self.device.cmd_begin_render_pass(
                self.command_buffer,
                &begin_info,
                vk::SubpassContents::INLINE,
            );
            self.device
                .cmd_set_scissor(self.command_buffer, 0, &[render_area]);

            self.device.cmd_set_viewport(
                self.command_buffer,
                0,
                &[vk::Viewport::default()
                    .x(render_area.offset.x as f32)
                    .y(render_area.offset.y as f32 + render_area.extent.height as f32)
                    .width(render_area.extent.width as f32)
                    .height(-(render_area.extent.height as f32))
                    .min_depth(0.0)
                    .max_depth(1.0)],
            )
        }
    }

    pub fn end_render_pass(&self) {
        unsafe {
            self.device.cmd_end_render_pass(self.command_buffer);
        }
    }

    pub fn bind_pipeline(
        &self,
        pipeline: vk::Pipeline,
        pipeline_bind_point: vk::PipelineBindPoint,
    ) {
        unsafe {
            self.device
                .cmd_bind_pipeline(self.command_buffer, pipeline_bind_point, pipeline);
        }
    }

    pub fn bind_descriptor_sets(
        &self,
        pipeline_bind_point: vk::PipelineBindPoint,
        pipeline_layout: vk::PipelineLayout,
        descriptor_sets: &[vk::DescriptorSet],
    ) {
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                pipeline_bind_point,
                pipeline_layout,
                0,
                descriptor_sets,
                &[],
            );
        }
    }

    pub fn bind_index_buffer(&self, buffer: vk::Buffer, offset: u64, index_type: IndexType) {
        let vk_index_type: vk::IndexType = index_type.into();
        unsafe {
            self.device
                .cmd_bind_index_buffer(self.command_buffer, buffer, offset, vk_index_type)
        }
    }

    pub fn bind_vertex_buffers(&self, first_binding: u32, buffers: &[vk::Buffer], offsets: &[u64]) {
        unsafe {
            self.device.cmd_bind_vertex_buffers(
                self.command_buffer,
                first_binding,
                buffers,
                offsets,
            )
        }
    }

    pub fn draw_indexed(
        &self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        unsafe {
            self.device.cmd_draw_indexed(
                self.command_buffer,
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            )
        }
    }

    pub fn draw_array(
        &self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        unsafe {
            self.device.cmd_draw(
                self.command_buffer,
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            )
        }
    }

    pub fn pipeline_barrier(
        &self,
        src_stage_mask: vk::PipelineStageFlags,
        dst_stage_mask: vk::PipelineStageFlags,
        dependency_flags: vk::DependencyFlags,
        memory_barriers: &[vk::MemoryBarrier],
        buffer_memory_barriers: &[vk::BufferMemoryBarrier],
        image_memory_barriers: &[vk::ImageMemoryBarrier],
    ) {
        unsafe {
            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                src_stage_mask,
                dst_stage_mask,
                dependency_flags,
                memory_barriers,
                buffer_memory_barriers,
                image_memory_barriers,
            );
        }
    }

    /// Vulkan 1.3: Pipeline barrier 2 command using modern synchronization.
    ///
    /// This method uses `vkCmdPipelineBarrier2` which provides more flexible
    /// and expressive synchronization compared to the legacy `vkCmdPipelineBarrier`.
    ///
    /// # Arguments
    /// * `dependency_info` - Dependency info containing all barriers
    ///
    /// # Example
    /// ```no_run
    /// use katla_vulkan::sync::{ImageMemoryBarrier2, PipelineStage2Flags, AccessFlags2, DependencyInfo};
    /// # use ash::vk;
    ///
    /// # let command_buffer: katla_vulkan::CommandBuffer = unsafe { std::mem::zeroed() };
    /// # let image: vk::Image = unsafe { std::mem::zeroed() };
    /// let barrier = ImageMemoryBarrier2::new(image)
    ///     .src_stage(PipelineStage2Flags::TRANSFER)
    ///     .dst_stage(PipelineStage2Flags::FRAGMENT_SHADER)
    ///     .src_access(AccessFlags2::TRANSFER_WRITE)
    ///     .dst_access(AccessFlags2::SHADER_READ)
    ///     .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
    ///     .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
    ///     .subresource_range(vk::ImageSubresourceRange {
    ///         aspect_mask: vk::ImageAspectFlags::COLOR,
    ///         base_mip_level: 0,
    ///         level_count: 1,
    ///         base_array_layer: 0,
    ///         layer_count: 1,
    ///     });
    ///
    /// let dep_info = DependencyInfo::new()
    ///     .add_image_barrier(barrier);
    ///
    /// command_buffer.pipeline_barrier2(dep_info);
    /// ```
    pub fn pipeline_barrier2(&self, dependency_info: DependencyInfo) {
        dependency_info.build(|dep_info| unsafe {
            self.device
                .cmd_pipeline_barrier2(self.command_buffer, dep_info);
        });
    }

    /// Vulkan 1.3: Begin dynamic rendering.
    ///
    /// This method begins a dynamic rendering pass using VK_KHR_dynamic_rendering,
    /// which replaces traditional render passes and framebuffers.
    ///
    /// # Arguments
    /// * `rendering_info` - Rendering info describing attachments and render area
    ///
    /// # Example
    /// ```no_run
    /// # use ash::vk;
    /// # use katla_vulkan::render_graph::types::{RenderingAttachmentInfo, RenderingInfo, ClearValue, Extent2D};
    /// # use katla_vulkan::CommandBuffer;
    /// # let command_buffer: CommandBuffer = unsafe { std::mem::zeroed() };
    /// # let color_image_view: vk::ImageView = unsafe { std::mem::zeroed() };
    /// # let depth_image_view: vk::ImageView = unsafe { std::mem::zeroed() };
    /// let color_attachment = RenderingAttachmentInfo::new(color_image_view)
    ///     .clear(ClearValue::color(0.1, 0.2, 0.3, 1.0));
    ///
    /// let depth_attachment = RenderingAttachmentInfo::new(depth_image_view)
    ///     .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
    ///     .clear(ClearValue::depth(1.0, 0));
    ///
    /// let rendering_info = RenderingInfo::new()
    ///     .add_color_attachment(color_attachment)
    ///     .depth_attachment(depth_attachment)
    ///     .render_area(vk::Rect2D {
    ///         offset: vk::Offset2D { x: 0, y: 0 },
    ///         extent: vk::Extent2D { width: 1920, height: 1080 },
    ///     });
    ///
    /// command_buffer.begin_rendering(rendering_info);
    /// // ... record draw commands ...
    /// command_buffer.end_rendering();
    /// ```
    pub fn begin_rendering(&self, rendering_info: RenderingInfo) {
        rendering_info.build(|rendering_info| unsafe {
            self.device
                .cmd_begin_rendering(self.command_buffer, rendering_info);
        });
    }

    /// Vulkan 1.3: End dynamic rendering.
    ///
    /// Ends a dynamic rendering pass begun with `begin_rendering`.
    pub fn end_rendering(&self) {
        unsafe {
            self.device.cmd_end_rendering(self.command_buffer);
        }
    }

    /// Set the viewport for this command buffer.
    ///
    /// # Arguments
    /// * `viewports` - Slice of viewport structures to set
    pub fn set_viewport(&self, viewports: &[vk::Viewport]) {
        unsafe {
            self.device
                .cmd_set_viewport(self.command_buffer, 0, viewports);
        }
    }

    /// Set the scissor rectangle for this command buffer.
    ///
    /// # Arguments
    /// * `scissors` - Slice of scissor rectangles to set
    pub fn set_scissor(&self, scissors: &[vk::Rect2D]) {
        unsafe {
            self.device
                .cmd_set_scissor(self.command_buffer, 0, scissors);
        }
    }

    pub fn return_to_pool(&self) {
        unsafe {
            self.device
                .free_command_buffers(self.command_pool, &[self.command_buffer]);
        }
    }
}
