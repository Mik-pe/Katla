use ash::vk;

use super::*;
use crate::error::RendererError;
use crate::renderer::registry::AssetRegistry;

impl GlobalParticleSystem {
    pub fn update(
        &mut self,
        delta_time: f32,
        frame_index: u32,
    ) -> Result<(u32, u32), RendererError> {
        self.frame_count += 1;

        self.upload_emitter_configs(frame_index as usize)?;

        self.recompute_estimated_max_alive();

        let total_emit_count = self.calculate_emit_count(delta_time);

        let total_burst_count: u32 = self
            .emitter_pool
            .emitter_states
            .iter()
            .map(|state| state.burst_count)
            .sum();

        let total_this_frame = total_emit_count + total_burst_count;

        log::debug!(
            "Particle emit: rate={} burst={} total={}",
            total_emit_count,
            total_burst_count,
            total_this_frame
        );

        self.update_frame_data(delta_time, total_emit_count, total_burst_count, frame_index)?;

        for state in &mut self.emitter_pool.emitter_states {
            state.burst_count = 0;
        }

        let emit_count = total_emit_count + total_burst_count;

        #[cfg(debug_assertions)]
        {
            let validation_errors = validate_all_emitters(&self.emitter_pool.emitters);
            if !validation_errors.is_empty() {
                for error in &validation_errors {
                    log::warn!("Emitter validation error: {}", error);
                }
            }
        }

        if total_this_frame > 0 {
            self.total_emitted += total_this_frame as u64;
        }

        Ok((self.estimated_max_alive, emit_count))
    }

    pub(super) fn upload_emitter_configs(&self, frame_index: usize) -> Result<(), RendererError> {
        let fi = frame_index % 2;
        if let Some((_buffer, allocation)) = &self.buffers.emitter_configs[fi] {
            if let Some(mapped) = allocation.mapped_ptr() {
                let dst = mapped.as_ptr() as *mut EmitterConfig;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.emitter_pool.emitters.as_ptr(),
                        dst,
                        self.emitter_pool.emitters.len(),
                    );
                }
                let _ = self.context.flush_mapped_memory(
                    allocation,
                    0,
                    (self.emitter_pool.emitters.len() * std::mem::size_of::<EmitterConfig>())
                        as u64,
                );
            } else {
                log::warn!("Emitter configs buffer is not mapped for CPU access");
                return Err(RendererError::InvalidOperation(
                    "Emitter configs buffer mapping failed".into(),
                ));
            }
        } else {
            log::warn!("Emitter configs buffer not initialized");
            return Err(RendererError::InvalidOperation(
                "Emitter configs buffer not created".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn update_frame_data(
        &self,
        delta_time: f32,
        emit_count: u32,
        burst_count: u32,
        frame_index: u32,
    ) -> Result<(), RendererError> {
        let fi = (frame_index as usize) % 2;
        if let Some((_buffer, allocation)) = &self.buffers.frame_data[fi] {
            if let Some(mapped) = allocation.mapped_ptr() {
                let active_emitter_count = self
                    .emitter_pool
                    .emitters
                    .iter()
                    .zip(self.emitter_pool.emitter_states.iter())
                    .filter(|(e, s)| e.emit_rate > 0.0 || s.burst_count > 0)
                    .count() as u32;

                let total_simulate_count = self.estimated_max_alive + emit_count + burst_count;

                let frame_data = FrameData {
                    delta_time,
                    total_emit_count: emit_count + burst_count,
                    emitter_count: active_emitter_count,
                    random_seed: self.frame_count,
                    total_simulate_count,
                    burst_count,
                    frame_index,
                    _pad: 0,
                };

                log::debug!(
                    "FrameData {}: dt={:.6} emit={} burst={} max_alive={} sim={} emitters={}",
                    frame_index,
                    frame_data.delta_time,
                    frame_data.total_emit_count,
                    frame_data.burst_count,
                    self.estimated_max_alive,
                    frame_data.total_simulate_count,
                    frame_data.emitter_count
                );

                unsafe {
                    std::ptr::copy_nonoverlapping(
                        &frame_data as *const FrameData as *const u8,
                        mapped.as_ptr() as *mut u8,
                        std::mem::size_of::<FrameData>(),
                    );
                }
                let _ = self.context.flush_mapped_memory(
                    allocation,
                    0,
                    std::mem::size_of::<FrameData>() as u64,
                );
            } else {
                log::warn!("Frame data buffer is not mapped for CPU access");
                return Err(RendererError::InvalidOperation(
                    "Frame data buffer mapping failed".into(),
                ));
            }
        } else {
            log::warn!("Frame data buffer not initialized");
            return Err(RendererError::InvalidOperation(
                "Frame data buffer not created".into(),
            ));
        }
        Ok(())
    }

    pub fn reset_simulate_counters(
        &self,
        command_buffer: vk::CommandBuffer,
        emit_ran: bool,
        frame_index: usize,
    ) {
        let device = &self.context.device;
        let counters_buffer = self.buffer.counters_buffer(frame_index);

        let zero_bytes = 0u32.to_le_bytes();
        unsafe {
            device.cmd_update_buffer(command_buffer, counters_buffer, 12, &zero_bytes);
        }

        if !emit_ran {
            unsafe {
                device.cmd_update_buffer(command_buffer, counters_buffer, 0, &zero_bytes);
            }
            let prev_fi = (frame_index + 1) % 2;
            let prev_counters = self.buffer.counters_buffer(prev_fi);
            let copy_regions = [
                vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 8,
                    size: 4,
                },
                vk::BufferCopy {
                    src_offset: 4,
                    dst_offset: 4,
                    size: 4,
                },
            ];
            unsafe {
                device.cmd_copy_buffer(
                    command_buffer,
                    prev_counters,
                    counters_buffer,
                    &copy_regions,
                );
            }
        }

        let counters_barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(counters_buffer)
            .offset(0)
            .size(std::mem::size_of::<buffer::ParticleCounters>() as u64);

        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                std::slice::from_ref(&counters_barrier),
                &[],
            );
        }
    }

    pub fn record_emit_dispatch(
        &self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        emit_workgroups: u32,
        frame_index: usize,
    ) -> Result<(), RendererError> {
        let pipeline = self.pipelines.emit.ok_or("Emit pipeline not created")?;

        let compute_pipeline = asset_registry
            .get_pipeline(pipeline)
            .ok_or("Failed to get emit pipeline from registry")?;

        let vk_pipeline = compute_pipeline.vk_pipeline();
        let vk_layout = compute_pipeline.vk_layout();

        let device = &self.context.device;

        let prev_fi = (frame_index + 1) % 2;
        let counters_buffer = self.buffer.counters_buffer(frame_index);
        let prev_counters_buffer = self.buffer.counters_buffer(prev_fi);

        let copy_regions = [
            vk::BufferCopy {
                src_offset: 0,
                dst_offset: 8,
                size: 4,
            },
            vk::BufferCopy {
                src_offset: 4,
                dst_offset: 4,
                size: 4,
            },
        ];
        unsafe {
            device.cmd_copy_buffer(
                command_buffer,
                prev_counters_buffer,
                counters_buffer,
                &copy_regions,
            );
        }

        let zero_bytes = 0u32.to_le_bytes();
        unsafe {
            device.cmd_update_buffer(command_buffer, counters_buffer, 0, &zero_bytes);
        }

        let fill_barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(counters_buffer)
            .offset(0)
            .size(std::mem::size_of::<ParticleCounters>() as u64);

        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                std::slice::from_ref(&fill_barrier),
                &[],
            );
        }

        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        if let Some(descriptor_set) = self.descriptors.compute_sets[frame_index % 2] {
            if descriptor_set != vk::DescriptorSet::null() {
                log::debug!(
                    "Emit dispatch: Set 0 descriptor={:?}, particle_buffer={:?}",
                    descriptor_set,
                    self.buffer.particle_buffer(),
                );
                unsafe {
                    device.cmd_bind_descriptor_sets(
                        command_buffer,
                        vk::PipelineBindPoint::COMPUTE,
                        vk_layout,
                        0,
                        std::slice::from_ref(&descriptor_set),
                        &[],
                    );
                }
            } else {
                return Err(RendererError::InvalidOperation(
                    "Emit compute descriptor set is null".into(),
                ));
            }
        } else {
            return Err(RendererError::InvalidOperation(
                "Compute descriptor set not allocated".into(),
            ));
        }

        let fi = frame_index % 2;
        if let Some((frame_buffer, _)) = &self.buffers.frame_data[fi]
            && let Some((emitter_buffer, _)) = &self.buffers.emitter_configs[fi]
        {
            let frame_data_size = std::mem::size_of::<FrameData>() as u64;
            let emitter_size =
                (MAX_EMITTERS as usize * std::mem::size_of::<EmitterConfig>()) as u64;

            let frame_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*frame_buffer)
                .offset(0)
                .range(frame_data_size)];

            let emitter_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*emitter_buffer)
                .offset(0)
                .range(emitter_size)];

            let push_descriptor_writes = [
                vk::WriteDescriptorSet::default()
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(&frame_buffer_info),
                vk::WriteDescriptorSet::default()
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(&emitter_buffer_info),
            ];

            unsafe {
                let push_descriptor = self
                    .context
                    .push_descriptor_khr
                    .as_ref()
                    .ok_or("Push descriptor extension not available")?;

                push_descriptor.cmd_push_descriptor_set(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    vk_layout,
                    1,
                    &push_descriptor_writes,
                );
            }
        }

        unsafe {
            device.cmd_dispatch(command_buffer, emit_workgroups, 1, 1);
        }

        self.emit_to_simulate_barrier(command_buffer, frame_index)?;

        Ok(())
    }

    pub fn record_simulate_dispatch(
        &self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        simulate_workgroups: u32,
        frame_index: usize,
    ) -> Result<(), RendererError> {
        let device = &self.context.device;

        let pipeline = self
            .pipelines
            .simulate
            .ok_or("Simulate pipeline not created")?;

        let compute_pipeline = asset_registry
            .get_pipeline(pipeline)
            .ok_or("Failed to get simulate pipeline from registry")?;

        let vk_pipeline = compute_pipeline.vk_pipeline();
        let vk_layout = compute_pipeline.vk_layout();

        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        if let Some(descriptor_set) = self.descriptors.compute_sets[frame_index % 2] {
            if descriptor_set != vk::DescriptorSet::null() {
                unsafe {
                    device.cmd_bind_descriptor_sets(
                        command_buffer,
                        vk::PipelineBindPoint::COMPUTE,
                        vk_layout,
                        0,
                        std::slice::from_ref(&descriptor_set),
                        &[],
                    );
                }
            } else {
                return Err(RendererError::InvalidOperation(
                    "Simulate compute descriptor set is null".into(),
                ));
            }
        } else {
            return Err(RendererError::InvalidOperation(
                "Compute descriptor set not allocated".into(),
            ));
        }

        let fi = frame_index % 2;
        if let Some((frame_buffer, _)) = &self.buffers.frame_data[fi] {
            let frame_data_size = std::mem::size_of::<FrameData>() as u64;
            let emitter_size =
                (MAX_EMITTERS as usize * std::mem::size_of::<EmitterConfig>()) as u64;

            let frame_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*frame_buffer)
                .offset(0)
                .range(frame_data_size)];

            let emitter_buffer_info =
                if let Some((emitter_buf, _)) = &self.buffers.emitter_configs[fi] {
                    Some([vk::DescriptorBufferInfo::default()
                        .buffer(*emitter_buf)
                        .offset(0)
                        .range(emitter_size)])
                } else {
                    None
                };

            let mut push_descriptor_writes = vec![
                vk::WriteDescriptorSet::default()
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(&frame_buffer_info),
            ];

            if let Some(info) = &emitter_buffer_info {
                push_descriptor_writes.push(
                    vk::WriteDescriptorSet::default()
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(1)
                        .buffer_info(info),
                );
            }

            unsafe {
                let push_descriptor = self
                    .context
                    .push_descriptor_khr
                    .as_ref()
                    .ok_or("Push descriptor extension not available")?;

                push_descriptor.cmd_push_descriptor_set(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    vk_layout,
                    1,
                    &push_descriptor_writes,
                );
            }
        }

        unsafe {
            device.cmd_dispatch(command_buffer, simulate_workgroups, 1, 1);
        }

        Ok(())
    }

    /// Record a 1-workgroup dispatch that writes the indirect draw command.
    ///
    /// This must be called AFTER `record_simulate_dispatch`. Uses push
    /// descriptors so bindings are recorded inline in the command buffer.
    /// A compute-to-compute barrier ensures the simulate's alive_count
    /// writes are visible before reading, and the draw command write is
    /// visible to subsequent compute reads (e.g. validation shader).
    pub fn record_draw_command_dispatch(
        &self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        frame_index: usize,
    ) -> Result<(), RendererError> {
        let device = &self.context.device;
        let fi = frame_index % 2;

        let pipeline = self
            .pipelines
            .draw_command
            .ok_or("Draw command pipeline not created")?;

        let compute_pipeline = asset_registry
            .get_pipeline(pipeline)
            .ok_or("Failed to get draw command pipeline from registry")?;

        let vk_pipeline = compute_pipeline.vk_pipeline();
        let vk_layout = compute_pipeline.vk_layout();

        // Barrier: simulate wrote counters (alive_count), draw command reads them
        let counters_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags2::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.buffer.counters_buffer(fi))
            .offset(0)
            .size(std::mem::size_of::<buffer::ParticleCounters>() as u64);

        // Barrier: draw command will write indirect draw buffer
        let indirect_draw_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.buffer.indirect_draw_buffer(fi))
            .offset(0)
            .size(16);

        let barriers = [counters_barrier, indirect_draw_barrier];
        let dep_info = vk::DependencyInfo::default().buffer_memory_barriers(&barriers);

        unsafe {
            device.cmd_pipeline_barrier2(command_buffer, &dep_info);
        }

        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        // Push descriptors inline in the command buffer
        let counters_size = std::mem::size_of::<buffer::ParticleCounters>() as u64;
        let counters_buffer_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.counters_buffer(fi),
            offset: 0,
            range: counters_size,
        }];
        let draw_buffer_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.indirect_draw_buffer(fi),
            offset: 0,
            range: 16,
        }];

        let push_descriptor_writes = [
            vk::WriteDescriptorSet::default()
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&counters_buffer_info),
            vk::WriteDescriptorSet::default()
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&draw_buffer_info),
        ];

        let push_ext = self
            .context
            .push_descriptor_khr
            .as_ref()
            .ok_or("Push descriptor extension not available")?;

        unsafe {
            push_ext.cmd_push_descriptor_set(
                command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                vk_layout,
                0,
                &push_descriptor_writes,
            );
        }

        unsafe {
            device.cmd_dispatch(command_buffer, 1, 1, 1);
        }

        // Barrier: make draw command write visible to subsequent compute reads
        // (e.g. validation shader). The render pass handles its own
        // COMPUTE→DRAW_INDIRECT transition.
        let draw_read_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags2::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.buffer.indirect_draw_buffer(fi))
            .offset(0)
            .size(16);

        let particle_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags2::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.buffer.particle_buffer())
            .offset(0)
            .size(self.buffer.layout().total_size);

        let post_barriers = [draw_read_barrier, particle_barrier];
        let post_dep_info = vk::DependencyInfo::default().buffer_memory_barriers(&post_barriers);

        unsafe {
            device.cmd_pipeline_barrier2(command_buffer, &post_dep_info);
        }

        Ok(())
    }
}
