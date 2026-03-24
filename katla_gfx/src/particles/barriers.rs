use super::*;

use crate::sync::{
    AccessFlags2, BufferMemoryBarrier2, DependencyInfo, PipelineStage2Flags, VkBuffer,
};

impl GlobalParticleSystem {
    pub fn emit_to_simulate_barrier(
        &self,
        command_buffer: vk::CommandBuffer,
        frame_index: usize,
    ) -> Result<(), String> {
        let particle_buffer = self.buffer.particle_buffer();
        let counters_buffer = self.buffer.counters_buffer(frame_index);
        let device = &self.context.device;

        let total_buffer_size = self.buffer.layout().total_size;

        let counters_size = std::mem::size_of::<buffer::ParticleCounters>() as u64;

        let particle_barrier = BufferMemoryBarrier2 {
            src_stage_mask: PipelineStage2Flags::COMPUTE_SHADER,
            dst_stage_mask: PipelineStage2Flags::COMPUTE_SHADER,
            src_access_mask: AccessFlags2::SHADER_WRITE,
            dst_access_mask: AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer: VkBuffer::new(particle_buffer),
            offset: 0,
            size: total_buffer_size,
        };

        let counters_barrier = BufferMemoryBarrier2 {
            src_stage_mask: PipelineStage2Flags::COMPUTE_SHADER,
            dst_stage_mask: PipelineStage2Flags::COMPUTE_SHADER,
            src_access_mask: AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE,
            dst_access_mask: AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer: VkBuffer::new(counters_buffer),
            offset: 0,
            size: counters_size,
        };

        let dep_info = DependencyInfo::new()
            .add_buffer_barrier2(particle_barrier)
            .add_buffer_barrier2(counters_barrier);

        dep_info.build(|dep_info| unsafe {
            device.cmd_pipeline_barrier2(command_buffer, dep_info);
        });

        Ok(())
    }
}
