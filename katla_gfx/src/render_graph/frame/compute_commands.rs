use super::Frame;
use crate::render_graph::{
    BufferUsage, ComputeCommand, ComputeDispatch, ComputeDispatchSize, PassDesc, RenderGraphError,
};
use crate::renderer::VulkanRenderer;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

fn command_scope(command: &ComputeCommand) -> (vk::PipelineStageFlags2, vk::AccessFlags2) {
    match command {
        ComputeCommand::Dispatch(dispatch) => {
            let mut stages = vk::PipelineStageFlags2::COMPUTE_SHADER;
            let mut accesses = vk::AccessFlags2::SHADER_READ
                | vk::AccessFlags2::SHADER_WRITE
                | vk::AccessFlags2::UNIFORM_READ;
            if !dispatch.constants.is_empty() {
                stages |= vk::PipelineStageFlags2::ALL_TRANSFER;
                accesses |= vk::AccessFlags2::TRANSFER_WRITE;
            }
            if matches!(dispatch.size, ComputeDispatchSize::Indirect { .. }) {
                stages |= vk::PipelineStageFlags2::DRAW_INDIRECT;
                accesses |= vk::AccessFlags2::INDIRECT_COMMAND_READ;
            }
            (stages, accesses)
        }
        ComputeCommand::FillBuffer { .. } => (
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::TRANSFER_WRITE,
        ),
        ComputeCommand::CopyBuffer { .. } => (
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::TRANSFER_READ | vk::AccessFlags2::TRANSFER_WRITE,
        ),
    }
}

fn command_barrier(
    previous: &ComputeCommand,
    command: &ComputeCommand,
) -> vk::MemoryBarrier2<'static> {
    let (source_stages, source_accesses) = command_scope(previous);
    let (destination_stages, destination_accesses) = command_scope(command);
    vk::MemoryBarrier2::default()
        .src_stage_mask(source_stages)
        .src_access_mask(source_accesses)
        .dst_stage_mask(destination_stages)
        .dst_access_mask(destination_accesses)
}

impl Frame<'_, VulkanRenderer> {
    fn bind_graph_kernel(
        &mut self,
        dispatch: &ComputeDispatch,
        cmd: vk::CommandBuffer,
    ) -> Result<(), RenderGraphError> {
        let slot = self.current_frame();
        let descriptor = &dispatch.pipeline;
        let pipeline = self
            .renderer
            .graph_compute_pipelines
            .get(descriptor)
            .ok_or_else(|| {
                RenderGraphError::PipelineNotSet(
                    "Compute pipeline was not warmed before encoding".into(),
                )
            })?;
        let interface = &pipeline.interface;
        let mut infos = Vec::with_capacity(interface.bindings.len());
        for reflected in &interface.bindings {
            let binding = dispatch
                .bindings
                .iter()
                .find(|binding| {
                    binding.group == reflected.group && binding.binding == reflected.binding
                })
                .ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(
                        "Missing reflected compute binding".into(),
                    )
                })?;
            let buffer = self
                .graph
                .buffer_by_id(self.renderer, binding.resource, slot)
                .ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(
                        "Compute binding references an unavailable buffer".into(),
                    )
                })?;
            let size = if binding.range.size == u64::MAX {
                buffer.desc.size.checked_sub(binding.range.offset)
            } else {
                Some(binding.range.size)
            }
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(
                    "Compute binding exceeds live allocation".into(),
                )
            })?;
            if size == 0
                || size < reflected.minimum_buffer_bytes
                || binding
                    .range
                    .offset
                    .checked_add(size)
                    .is_none_or(|end| end > buffer.desc.size)
            {
                return Err(RenderGraphError::InvalidConfiguration(
                    "Compute binding exceeds live allocation".into(),
                ));
            }
            let limits = unsafe {
                self.renderer
                    .context
                    .instance
                    .get_physical_device_properties(self.renderer.context.physical_device)
            }
            .limits;
            let alignment = if reflected.usage == BufferUsage::Uniform {
                limits.min_uniform_buffer_offset_alignment
            } else {
                limits.min_storage_buffer_offset_alignment
            }
            .max(1);
            if (buffer.offset + binding.range.offset) % alignment != 0 {
                return Err(RenderGraphError::InvalidConfiguration(
                    "Compute binding offset violates native device alignment".into(),
                ));
            }
            infos.push([vk::DescriptorBufferInfo::default()
                .buffer(buffer.vk_buffer())
                .offset(buffer.offset + binding.range.offset)
                .range(size)]);
        }
        let writes = interface
            .bindings
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                vk::WriteDescriptorSet::default()
                    .dst_binding(index as u32)
                    .descriptor_type(if slot.usage == BufferUsage::Uniform {
                        vk::DescriptorType::UNIFORM_BUFFER
                    } else {
                        vk::DescriptorType::STORAGE_BUFFER
                    })
                    .buffer_info(&infos[index])
            })
            .collect::<Vec<_>>();
        let push = self
            .renderer
            .context
            .push_descriptor_khr
            .as_ref()
            .ok_or_else(|| RenderGraphError::BackendError("Push descriptors unavailable".into()))?;
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                pipeline.pipeline,
            );
            push.cmd_push_descriptor_set(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                pipeline.layout,
                0,
                &writes,
            );
        }
        if !dispatch.constants.is_empty() {
            let index = interface
                .bindings
                .iter()
                .position(|slot| slot.usage == BufferUsage::Uniform)
                .ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(
                        "Inline constants require a uniform binding".into(),
                    )
                })?;
            let binding = infos[index][0];
            if dispatch.constants.len() as u64 > binding.range {
                return Err(RenderGraphError::InvalidConfiguration(
                    "Inline constants exceed uniform allocation".into(),
                ));
            }
            unsafe {
                self.renderer.context.device.cmd_update_buffer(
                    cmd,
                    binding.buffer,
                    binding.offset,
                    &dispatch.constants,
                );
                let barrier = vk::BufferMemoryBarrier2::default()
                    .buffer(binding.buffer)
                    .offset(binding.offset)
                    .size(dispatch.constants.len() as u64)
                    .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                    .dst_access_mask(vk::AccessFlags2::UNIFORM_READ)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
                self.renderer.context.device.cmd_pipeline_barrier2(
                    cmd,
                    &vk::DependencyInfo::default().buffer_memory_barriers(&[barrier]),
                );
            }
        }
        Ok(())
    }

    pub(super) fn execute_compute_commands(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
    ) -> Result<(), RenderGraphError> {
        let slot = self.current_frame();
        let command_buffer = cmd.vk_command_buffer();
        for (index, command) in pass.commands.iter().enumerate() {
            if let Some(previous) = index
                .checked_sub(1)
                .and_then(|index| pass.commands.get(index))
            {
                let barrier = command_barrier(previous, command);
                unsafe {
                    self.renderer.context.device.cmd_pipeline_barrier2(
                        command_buffer,
                        &vk::DependencyInfo::default().memory_barriers(&[barrier]),
                    );
                }
                self.capture_memory_barrier(
                    &barrier,
                    &command_barrier(previous, command),
                    "intra_pass_command_dependency",
                );
            }
            match command {
                ComputeCommand::Dispatch(dispatch) => {
                    let groups = match dispatch.size {
                        ComputeDispatchSize::Direct(groups) => groups,
                        ComputeDispatchSize::Indirect { .. } => [1, 1, 1],
                    };
                    if groups.contains(&0) {
                        continue;
                    }
                    self.bind_graph_kernel(dispatch, command_buffer)?;
                    let layout_identity = self
                        .renderer
                        .graph_compute_pipelines
                        .get(&dispatch.pipeline)
                        .map(|pipeline| format!("compute:{:?}", pipeline.interface));
                    if let Some(identity) = layout_identity {
                        self.capture_binding_set(identity);
                    }
                    let mut resources: Vec<_> = dispatch
                        .bindings
                        .iter()
                        .map(|binding| binding.resource.0)
                        .collect();
                    if let ComputeDispatchSize::Indirect { resource, .. } = dispatch.size {
                        resources.push(resource.0);
                    }
                    self.capture_encoder(
                        pass,
                        crate::render_graph::capture::CapturedEncoderKind::Compute,
                        resources,
                    );
                    unsafe {
                        match dispatch.size {
                            ComputeDispatchSize::Indirect { resource, offset } => {
                                let buffer = self
                                    .graph
                                    .buffer_by_id(self.renderer, resource, slot)
                                    .ok_or_else(|| {
                                        RenderGraphError::InvalidConfiguration(
                                            "Indirect buffer unavailable".into(),
                                        )
                                    })?;
                                if !crate::render_graph::compute::indirect_range_fits(
                                    offset,
                                    buffer.desc.size,
                                ) {
                                    return Err(RenderGraphError::InvalidConfiguration(
                                        "Indirect command exceeds live buffer".into(),
                                    ));
                                }
                                self.renderer.context.device.cmd_dispatch_indirect(
                                    command_buffer,
                                    buffer.vk_buffer(),
                                    buffer.offset + offset,
                                );
                            }
                            _ => self.renderer.context.device.cmd_dispatch(
                                command_buffer,
                                groups[0],
                                groups[1],
                                groups[2],
                            ),
                        }
                    }
                }
                ComputeCommand::FillBuffer {
                    resource,
                    range,
                    value,
                } => {
                    let Some(buffer) = self.graph.buffer_by_id(self.renderer, *resource, slot)
                    else {
                        return Err(RenderGraphError::InvalidConfiguration(
                            "Fill buffer unavailable".into(),
                        ));
                    };
                    let size = if range.size == u64::MAX {
                        buffer.desc.size.checked_sub(range.offset)
                    } else {
                        Some(range.size)
                    }
                    .ok_or_else(|| {
                        RenderGraphError::InvalidConfiguration("Fill exceeds live buffer".into())
                    })?;
                    if size == 0
                        || size % 4 != 0
                        || range
                            .offset
                            .checked_add(size)
                            .is_none_or(|end| end > buffer.desc.size)
                    {
                        return Err(RenderGraphError::InvalidConfiguration(
                            "Fill exceeds live buffer".into(),
                        ));
                    }
                    unsafe {
                        self.renderer.context.device.cmd_fill_buffer(
                            command_buffer,
                            buffer.vk_buffer(),
                            buffer.offset + range.offset,
                            size,
                            *value,
                        );
                    }
                    self.capture_encoder(
                        pass,
                        crate::render_graph::capture::CapturedEncoderKind::Blit,
                        vec![resource.0],
                    );
                }
                ComputeCommand::CopyBuffer {
                    source,
                    destination,
                    source_offset,
                    destination_offset,
                    size,
                } => {
                    let source = self
                        .graph
                        .buffer_by_id(self.renderer, *source, slot)
                        .ok_or_else(|| {
                            RenderGraphError::InvalidConfiguration("Copy source unavailable".into())
                        })?;
                    let destination = self
                        .graph
                        .buffer_by_id(self.renderer, *destination, slot)
                        .ok_or_else(|| {
                            RenderGraphError::InvalidConfiguration(
                                "Copy destination unavailable".into(),
                            )
                        })?;
                    if source_offset
                        .checked_add(*size)
                        .is_none_or(|end| end > source.desc.size)
                        || destination_offset
                            .checked_add(*size)
                            .is_none_or(|end| end > destination.desc.size)
                    {
                        return Err(RenderGraphError::InvalidConfiguration(
                            "Copy exceeds live buffer".into(),
                        ));
                    }
                    if source.vk_buffer() == destination.vk_buffer()
                        && crate::render_graph::BufferByteRange::new(
                            source.offset + source_offset,
                            *size,
                        )
                        .intersection(crate::render_graph::BufferByteRange::new(
                            destination.offset + destination_offset,
                            *size,
                        ))
                        .is_some()
                    {
                        return Err(RenderGraphError::InvalidConfiguration(
                            "Buffer copy aliases overlapping native ranges".into(),
                        ));
                    }
                    let region = vk::BufferCopy::default()
                        .src_offset(source.offset + source_offset)
                        .dst_offset(destination.offset + destination_offset)
                        .size(*size);
                    unsafe {
                        self.renderer.context.device.cmd_copy_buffer(
                            command_buffer,
                            source.vk_buffer(),
                            destination.vk_buffer(),
                            &[region],
                        );
                    }
                    if let ComputeCommand::CopyBuffer {
                        source,
                        destination,
                        ..
                    } = command
                    {
                        self.capture_encoder(
                            pass,
                            crate::render_graph::capture::CapturedEncoderKind::Blit,
                            vec![source.0, destination.0],
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
