use ash::{vk, Device};

use super::CommandPool;

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
        let command_buffer: vk::CommandBuffer =
            unsafe { device.allocate_command_buffers(&create_info).unwrap()[0] };

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
                .unwrap();
        }
    }

    pub fn end_single_time_command(&self) {
        unsafe {
            self.device.end_command_buffer(self.command_buffer).unwrap();
        }
    }

    pub fn begin_command(&self, flags: vk::CommandBufferUsageFlags) {
        let begin_info = vk::CommandBufferBeginInfo::default().flags(flags);
        unsafe {
            self.device
                .begin_command_buffer(self.command_buffer, &begin_info)
                .unwrap();
        }
    }

    pub fn end_command(&self) {
        unsafe {
            self.device.end_command_buffer(self.command_buffer).unwrap();
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
            //TODO: alternatives to binding?
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

    pub fn bind_index_buffer(&self, buffer: vk::Buffer, offset: u64, index_type: vk::IndexType) {
        unsafe {
            self.device
                .cmd_bind_index_buffer(self.command_buffer, buffer, offset, index_type)
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

    pub fn return_to_pool(&self) {
        unsafe {
            self.device
                .free_command_buffers(self.command_pool, &[self.command_buffer]);
        }
    }
}
