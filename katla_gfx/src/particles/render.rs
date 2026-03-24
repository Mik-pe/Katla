use super::*;

impl GlobalParticleSystem {
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

        if let Some(descriptor_set) = self.render_descriptor_set {
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
