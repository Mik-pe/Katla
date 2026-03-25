use super::*;

impl GlobalParticleSystem {
    /// Issue buffer memory barriers that must execute BEFORE entering a dynamic
    /// rendering instance for particle drawing.
    ///
    /// These barriers synchronize compute shader writes (simulate pass) with
    /// graphics pipeline reads (indirect draw + vertex input). Calling them
    /// inside `cmd_begin_rendering`/`cmd_end_rendering` violates the Vulkan
    /// spec unless `VK_KHR_dynamic_rendering_local_read` is enabled.
    pub fn pre_render_barriers(&self, command_buffer: vk::CommandBuffer, frame_index: usize) {
        if self.estimated_max_alive == 0 {
            return;
        }

        let fi = frame_index % 2;

        let indirect_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(vk::PipelineStageFlags2::DRAW_INDIRECT)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags2::INDIRECT_COMMAND_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.buffer.indirect_draw_buffer(fi))
            .offset(0)
            .size(16);

        let particle_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(vk::PipelineStageFlags2::VERTEX_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags2::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.buffer.particle_buffer())
            .offset(0)
            .size(vk::WHOLE_SIZE);

        let barriers = [indirect_barrier, particle_barrier];
        let dep_info = vk::DependencyInfo::default().buffer_memory_barriers(&barriers);

        unsafe {
            self.context
                .device
                .cmd_pipeline_barrier2(command_buffer, &dep_info);
        }
    }

    /// Draw particles using indirect drawing.
    ///
    /// Must be called inside a dynamic rendering instance (after `cmd_begin_rendering`).
    /// Call [`Self::pre_render_barriers()`] before entering the render pass to ensure
    /// compute-to-graphics synchronization.
    pub fn render(
        &mut self,
        command_buffer: vk::CommandBuffer,
        _render_pass: vk::RenderPass,
        pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        storage_descriptor_set: vk::DescriptorSet,
        frame_index: usize,
    ) -> Result<(), String> {
        let device = &self.context.device;

        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
        }

        self.update_render_descriptor_binding(frame_index)?;

        if let Some(descriptor_set) = self.render_descriptor_sets[frame_index % 2] {
            unsafe {
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    layout,
                    0,
                    std::slice::from_ref(&descriptor_set),
                    &[],
                );
            }
        } else {
            return Err("Particle render descriptor set not allocated".to_string());
        }

        unsafe {
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                1,
                std::slice::from_ref(&storage_descriptor_set),
                &[],
            );
        }

        if self.estimated_max_alive > 0 {
            unsafe {
                device.cmd_draw_indirect(
                    command_buffer,
                    self.buffer.indirect_draw_buffer(frame_index),
                    0,
                    1,
                    16,
                );
            }
        }

        Ok(())
    }
}
