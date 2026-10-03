//! Native encoders consume resolved graph attachments and pass-local commands.

use super::attachments::ResolvedMetalAttachments;
use super::execution_plan::{MetalExecutionPlan, MetalPassRecord};
use super::graphics_packet::GraphicsFrame;
use super::metal_renderer::MetalRenderer;
use super::render_encoder::MetalRenderEncoder;
use super::texture::{MetalTexture, MetalTextureView};
use crate::backend::command::{GpuCommandBuffer, GpuRenderEncoder, IndexType};
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::render_graph::{FrameGraph, PassExecutionData, PassKind};
use crate::renderer::gpu_renderer::GpuRenderer;
use crate::renderer::types::{DrawList, UIDrawList};
use objc2_metal::{MTL4CommandBuffer, MTL4CommandEncoder, MTLTexture};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

fn native_buffer_scope(buffer: &super::buffer::MetalGraphBuffer) -> u64 {
    buffer
        .desc
        .size
        .min(buffer.buffer.size().saturating_sub(buffer.offset))
}

fn validate_copy_scope(
    source_size: u64,
    destination_size: u64,
    source_offset: u64,
    destination_offset: u64,
    size: u64,
) -> Result<(), RendererError> {
    if source_offset
        .checked_add(size)
        .is_none_or(|end| end > source_size)
        || destination_offset
            .checked_add(size)
            .is_none_or(|end| end > destination_size)
    {
        return Err(RendererError::InvalidOperation(
            "Copy buffer command exceeds its native source or destination range".into(),
        ));
    }
    Ok(())
}

fn validate_frame_submissions(
    plan: &MetalExecutionPlan,
    pending: &HashMap<usize, PassExecutionData>,
) -> Result<(), RendererError> {
    let scheduled = plan
        .passes()
        .iter()
        .map(|record| record.pass_index)
        .collect::<HashSet<_>>();
    if let Some(index) = pending.keys().find(|index| !scheduled.contains(index)) {
        return Err(RendererError::InvalidOperation(format!(
            "Metal received submissions for pass index {index}, which is absent from the compiled execution plan"
        )));
    }
    for record in plan.passes() {
        if let Some(data) = pending.get(&record.pass_index) {
            data.validate(&record.name, record.pass_type, record.kind)?;
        }
    }
    Ok(())
}

impl MetalRenderer {
    pub(crate) fn render_frame(
        &mut self,
        frame: &crate::renderer::frame_scope::FrameToken,
        plan: &MetalExecutionPlan,
        mut pending: HashMap<usize, PassExecutionData>,
        graph: &FrameGraph<Self>,
        trace_enabled: bool,
    ) -> Result<crate::render_graph::ResourceExecutionTrace, RendererError> {
        validate_frame_submissions(plan, &pending)?;
        let slot = self.frame_index();
        if self.pending_frame.is_some() || self.frame_slots[slot].submission.is_some() {
            return Err(RendererError::InvalidOperation(
                "The acquired frame has already been submitted".into(),
            ));
        }
        let texture = self
            .current_drawable_texture
            .as_ref()
            .cloned()
            .ok_or_else(|| RendererError::InvalidOperation("No drawable texture".into()))?;
        let drawable = MetalTextureView::new(
            texture.clone(),
            MetalTexture::new(texture, crate::texture::ImageFormat::B8G8R8A8Srgb),
        );
        let slot = <Self as crate::render_graph::RenderGraphBackend>::current_frame(self);
        let mut defined_outputs = self.defined_output_contents.clone();
        let resolved = plan
            .passes()
            .iter()
            .map(|record| {
                if record.pass_type != crate::render_graph::PassType::Graphics { return Ok(None); }
                let attachments =
                    ResolvedMetalAttachments::resolve(record, graph, &drawable, slot, self)?;

                for (contract, native) in record.color_attachments.iter().zip(&attachments.info.color_attachments) {
                    if graph.imported_images.contains_key(&contract.resource) || graph.resource_name(contract.resource) == Some("backbuffer") {
                        let identity = (native.view.inner.gpuResourceID().to_raw(), 1);
                        if contract.load_op == crate::render_pass::LoadOp::Load && !defined_outputs.contains(&identity) {
                            return Err(RendererError::InvalidOperation(format!("Pass '{}' loads an output image before its contents are defined", record.name)));
                        }
                        if contract.store_op == crate::render_pass::StoreOp::Store { defined_outputs.insert(identity); } else { defined_outputs.remove(&identity); }
                    }
                }
                if let (Some(contract), Some(native)) = (&record.depth_attachment, &attachments.info.depth_attachment)
                    && graph.imported_images.contains_key(&contract.resource) {
                        let resource_id = native.view.inner.gpuResourceID().to_raw();
                        let mut aspects = vec![(2, contract.load_op, contract.store_op)];
                        if matches!(contract.format, crate::texture::ImageFormat::D32SfloatS8Uint | crate::texture::ImageFormat::D24UnormS8Uint) {
                            aspects.push((4, contract.stencil_ops.load, contract.stencil_ops.store));
                        }
                        for (aspect, load, store) in aspects {
                            let identity = (resource_id, aspect);
                            if load == crate::render_pass::LoadOp::Load && !defined_outputs.contains(&identity) { return Err(RendererError::InvalidOperation(format!("Pass '{}' loads imported depth/stencil contents before they are defined", record.name))); }
                            if store == crate::render_pass::StoreOp::Store { defined_outputs.insert(identity); } else { defined_outputs.remove(&identity); }
                        }
                }
                Ok(Some(attachments))
            })
            .collect::<Result<Vec<_>, RendererError>>()?;
        let mut uploaded: Vec<*const DrawList> = Vec::new();
        for data in pending.values() {
            for list in &data.draw_lists {
                let identity = Rc::as_ptr(list);
                if !uploaded.contains(&identity) {
                    uploaded.push(identity);
                    GpuRenderer::execute_draw_calls(self, frame, list)?;
                }
            }
        }
        let label = format!(
            "frame_slot.{slot}.frame.{}",
            self.frame_slots[slot].generation
        );
        let mut cmd_buffer = self
            .context
            .create_command_buffer_for_allocator(self.frame_slots[slot].allocator.clone(), &label);
        cmd_buffer
            .resources
            .set_submission_identity(slot, self.frame_slots[slot].generation);
        if trace_enabled {
            cmd_buffer.resources.enable_capture();
        }
        let default_data = PassExecutionData::default();
        let mut ui_uploads = HashMap::new();
        for record in plan
            .passes()
            .iter()
            .filter(|record| record.pass_type == crate::render_graph::PassType::Graphics)
        {
            let data = pending.get(&record.pass_index).unwrap_or(&default_data);
            if record.kind == PassKind::Ui
                && let Some(list) = data.ui_draw_lists.first().filter(|list| !list.is_empty())
            {
                self.ui_renderers[slot].upload_draw_list(&self.context, list)?;
                ui_uploads.insert(record.pass_index, self.ui_renderers[slot].clone());
            }
            self.prepare_packet_samplers(record)?;
            self.preflight_graphics_record(&cmd_buffer.resources, record, data, graph, slot)?;
        }
        cmd_buffer.begin();
        if let Some(queries) = &self.timestamp_queries
            && !queries.labels().is_empty()
        {
            if self.frame_slots[slot].timestamps.is_none() {
                self.frame_slots[slot].timestamps = Some(
                    super::timestamp_queries::MetalTimestampSlot::new(&self.context.device, slot)?,
                );
            }
            let generation = self.frame_slots[slot].generation;
            if let Some(timestamps) = &mut self.frame_slots[slot].timestamps {
                timestamps.begin(&cmd_buffer.inner, queries.labels(), generation);
            }
        }
        let persistent = self.persistent_buffers.snapshot();
        cmd_buffer.inner.useResidencySet(persistent.native());
        cmd_buffer.resources.retain_persistent_residency(persistent);
        cmd_buffer
            .inner
            .setLabel(Some(&objc2_foundation::NSString::from_str(&format!(
                "frame_slot.{slot}.frame.{}",
                self.frame_slots[slot].generation
            ))));
        if self.texture_uploads.has_pending() {
            use crate::backend::command::GpuBlitEncoder;
            cmd_buffer
                .resources
                .capture_context(None, "texture_upload", Vec::new());
            let mut blit = cmd_buffer.begin_blit_pass_with_label("texture_upload");
            blit.inner
                .barrierAfterQueueStages_beforeStages_visibilityOptions(
                    objc2_metal::MTLStages::All,
                    objc2_metal::MTLStages::Blit,
                    objc2_metal::MTL4VisibilityOptions::Device,
                );
            super::sync::capture_native_boundary(
                &cmd_buffer.resources,
                "texture_upload.acquire",
                super::sync::NativeBoundary::UploadAcquire,
                objc2_metal::MTLStages::All,
                objc2_metal::MTLStages::Blit,
                objc2_metal::MTL4VisibilityOptions::Device,
            );
            self.texture_uploads
                .encode_into(&mut blit, self.frame_slots[slot].generation);
            blit.inner
                .barrierAfterStages_beforeQueueStages_visibilityOptions(
                    objc2_metal::MTLStages::Blit,
                    objc2_metal::MTLStages::All,
                    objc2_metal::MTL4VisibilityOptions::Device,
                );
            super::sync::capture_native_boundary(
                &cmd_buffer.resources,
                "texture_upload.release",
                super::sync::NativeBoundary::UploadRelease,
                objc2_metal::MTLStages::Blit,
                objc2_metal::MTLStages::All,
                objc2_metal::MTL4VisibilityOptions::Device,
            );
            blit.end_encoding();
        }
        log::debug!("Metal execution plan: {}", plan.trace().join(" -> "));
        let mut execution_trace = crate::render_graph::ResourceExecutionTrace::new();
        for (position, (record, attachments)) in plan.passes().iter().zip(resolved).enumerate() {
            let data = pending.remove(&record.pass_index).unwrap_or_default();
            if record.pass_type != crate::render_graph::PassType::Graphics {
                let has_work = record.commands.iter().any(|command| match command { crate::render_graph::ComputeCommand::Dispatch(dispatch) => !matches!(dispatch.size, crate::render_graph::ComputeDispatchSize::Direct(groups) if groups.contains(&0)), crate::render_graph::ComputeCommand::FillBuffer {range,..} => range.size != 0, crate::render_graph::ComputeCommand::CopyBuffer {size,..} => *size != 0 });
                self.encode_compute_record(&mut cmd_buffer, record, graph, slot)?;
                if !has_work {
                    super::sync::capture_skipped_boundary(
                        graph,
                        record.pass_index,
                        &cmd_buffer.resources,
                    );
                }
                if trace_enabled {
                    execution_trace.push(crate::render_graph::ResourceExecutionTraceEntry {
                        pass_index: record.pass_index,
                        name: record.name.clone(),
                        pass_type: record.pass_type,
                        encode_position: position,
                        outcome: if has_work {
                            crate::render_graph::EmittedPassOutcome::Encoded
                        } else {
                            crate::render_graph::EmittedPassOutcome::SkippedNoWork
                        },
                        draw_calls: 0,
                        instances: 0,
                        color_attachment_ops: Vec::new(),
                        depth_attachment_ops: None,
                        color_targets: Vec::new(),
                        depth_target: None,
                    });
                }
                continue;
            }
            if let Some(upload) = ui_uploads.remove(&record.pass_index) {
                self.ui_renderers[slot] = upload;
            }
            let attachments = attachments.expect("graphics attachments were resolved");
            let counts = data.prepared_counts();
            let width = attachments.width;
            let height = attachments.height;
            let color_attachment_ops = trace_enabled.then(|| {
                attachments
                    .info
                    .color_attachments
                    .iter()
                    .map(|attachment| crate::render_pass::AttachmentOps {
                        load: attachment.load_op,
                        store: attachment.store_op,
                        clear_value: attachment.clear_value,
                    })
                    .collect::<Vec<_>>()
            });
            let depth_attachment_ops = trace_enabled
                .then(|| {
                    attachments
                        .info
                        .depth_attachment
                        .as_ref()
                        .map(|attachment| crate::render_pass::DepthStencilAttachmentOps {
                            depth: crate::render_pass::AttachmentOps {
                                load: attachment.load_op,
                                store: attachment.store_op,
                                clear_value: attachment.clear_value,
                            },
                            stencil: attachment.stencil_ops,
                        })
                })
                .flatten();
            let resources = if trace_enabled {
                record
                    .color_attachments
                    .iter()
                    .map(|attachment| attachment.resource.0)
                    .chain(
                        record
                            .depth_attachment
                            .iter()
                            .map(|attachment| attachment.resource.0),
                    )
                    .collect()
            } else {
                Vec::new()
            };
            cmd_buffer
                .resources
                .capture_context(Some(record.pass_index), &record.name, resources);
            let mut encoder = cmd_buffer.begin_render_pass(attachments.info);
            encoder
                .inner
                .setLabel(Some(&objc2_foundation::NSString::from_str(&record.name)));
            super::sync::apply_compiled_boundary(
                encoder.inner.as_ref(),
                graph,
                record.pass_index,
                objc2_metal::MTLStages::Vertex | objc2_metal::MTLStages::Fragment,
                &cmd_buffer.resources,
            );
            encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
            encoder.set_scissor(0, 0, width, height);
            let frame = GraphicsFrame {
                graph,
                slot,
                size: crate::size::Size2D::new(width, height),
            };
            if record.kind == PassKind::Ui {
                self.encode_ui_record(&mut encoder, record, data.ui_draw_lists.first(), frame)?;
            } else {
                self.encode_graphics_packet(&mut encoder, record, &data, frame)?;
            }
            encoder.end_encoding();
            if trace_enabled {
                execution_trace.push(crate::render_graph::ResourceExecutionTraceEntry {
                    pass_index: record.pass_index,
                    name: record.name.clone(),
                    pass_type: crate::render_graph::PassType::Graphics,
                    encode_position: position,
                    outcome: crate::render_graph::EmittedPassOutcome::Encoded,
                    draw_calls: counts.draw_calls,
                    instances: counts.instances,
                    color_attachment_ops: color_attachment_ops.unwrap_or_default(),
                    depth_attachment_ops,
                    color_targets: attachments.color_targets,
                    depth_target: attachments.depth_target,
                });
            }
        }
        if !graph.final_image_sync_ops().is_empty() {
            use crate::backend::command::GpuComputeEncoder;
            cmd_buffer
                .resources
                .capture_context(None, "final_output", Vec::new());
            let terminal = cmd_buffer.begin_compute_pass();
            super::sync::apply_final_image_boundary(
                terminal.inner.as_ref(),
                graph,
                &cmd_buffer.resources,
            );
            terminal.end_encoding();
        }
        cmd_buffer.resources.check()?;
        if let Some(timestamps) = &self.frame_slots[slot].timestamps {
            timestamps.end(&cmd_buffer.inner);
        }
        cmd_buffer.end();
        graph.record_buffer_execution(self);
        let mut exported_images = HashMap::new();
        for resource in &graph.exported_resources {
            let view = if graph.resource_name(*resource) == Some("backbuffer") {
                Some(drawable.clone())
            } else if let Some(handle) = graph.imported_images.get(resource) {
                self.textures.get(*handle).map(|entry| entry._view.clone())
            } else {
                graph
                    .transient_texture_by_id(*resource, slot)
                    .map(|texture| texture.view.clone())
            };
            if let Some(view) = view {
                exported_images.insert(*resource, view);
            }
        }
        let imported_buffers = plan
            .passes()
            .iter()
            .flat_map(|record| &record.buffer_accesses)
            .filter_map(|access| graph.imported_buffer_handle(access.resource))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if trace_enabled {
            execution_trace.backend = cmd_buffer.resources.capture();
            execution_trace.backend.frame =
                Some(crate::render_graph::capture::CapturedSubmission {
                    frame_slot: slot,
                    generation: self.frame_slots[slot].generation,
                    command_allocator: slot,
                    feedback_identity: format!(
                        "slot.{slot}.generation.{}",
                        self.frame_slots[slot].generation
                    ),
                    feedback: crate::render_graph::capture::CapturedFeedback::Pending,
                });
        }
        self.pending_frame = Some(super::frame_lifecycle::MetalPendingFrame {
            command: cmd_buffer,
            defined_outputs,
            exported_images,
            imported_buffers,
        });
        Ok(execution_trace)
    }

    fn encode_ui_record(
        &mut self,
        encoder: &mut MetalRenderEncoder,
        record: &MetalPassRecord,
        draw_list: Option<&UIDrawList>,
        frame: GraphicsFrame<'_>,
    ) -> Result<(), RendererError> {
        let crate::size::Size2D { width, height } = frame.size;
        let Some(draw_list) = draw_list.filter(|list| !list.is_empty()) else {
            return Ok(());
        };
        let material = record
            .material
            .ok_or_else(|| RendererError::InvalidOperation("UI pass has no material".into()))?;
        let pipeline = self.material_pipeline(material, record.color_attachments[0].format)?;
        encoder.bind_graphics_pipeline(&pipeline);
        self.bind_packet(encoder, &pipeline, &record.bindings, None, frame, None)?;
        if let Some(vertex_buffer) = self.ui_renderers[frame.slot].vertex_buffer() {
            encoder.bind_vertex_buffer(vertex_buffer, 0, 10);
        }
        if let Some(index_buffer) = self.ui_renderers[frame.slot].index_buffer() {
            encoder.bind_index_buffer(index_buffer, 0, IndexType::Uint32);
        }
        self.ui_renderers[frame.slot]
            .render_ui_commands(encoder, draw_list, &pipeline, width, height);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::validate_frame_submissions;
    use crate::metal::execution_plan::MetalExecutionPlan;
    use crate::render_graph::{PassExecutionData, PassKind};
    use std::collections::HashMap;
    #[test]
    fn test_submission_to_unknown_pass_index_is_rejected() {
        let plan = MetalExecutionPlan::for_test(&[PassKind::Ui]);
        let mut pending = HashMap::new();
        pending.insert(7usize, PassExecutionData::default());

        let err =
            validate_frame_submissions(&plan, &pending).expect_err("unknown pass index must fail");
        let message = err.to_string();
        assert!(
            message.contains("pass index 7"),
            "error must name the offending index: {message}"
        );
        assert!(
            message.contains("absent from the compiled execution plan"),
            "error must explain the contract: {message}"
        );
    }

    #[test]
    fn test_ui_pass_rejects_multiple_draw_lists() {
        let plan = MetalExecutionPlan::for_test(&[PassKind::Ui]);
        let mut pending = HashMap::new();
        pending.insert(
            0usize,
            PassExecutionData {
                ui_draw_lists: vec![
                    crate::renderer::types::UIDrawList::default(),
                    crate::renderer::types::UIDrawList::default(),
                ],
                ..Default::default()
            },
        );

        let err =
            validate_frame_submissions(&plan, &pending).expect_err("two UI draw lists must fail");
        let message = err.to_string();
        assert!(
            message.contains("received 2 UI draw lists"),
            "error must name the count: {message}"
        );
        assert!(
            message.contains("'pass_0'") && message.contains("UI pass"),
            "error must name the failing pass: {message}"
        );
    }

    #[test]
    fn test_single_ui_draw_list_is_accepted() {
        let plan = MetalExecutionPlan::for_test(&[PassKind::Ui]);
        let mut pending = HashMap::new();
        pending.insert(
            0usize,
            PassExecutionData {
                ui_draw_lists: vec![crate::renderer::types::UIDrawList::default()],
                ..Default::default()
            },
        );

        validate_frame_submissions(&plan, &pending)
            .expect("one composed UI draw list is the contract");
    }
}

impl MetalRenderer {
    fn encode_compute_record(
        &self,
        command: &mut super::command_buffer::MetalCommandBuffer,
        record: &MetalPassRecord,
        graph: &FrameGraph<Self>,
        slot: usize,
    ) -> Result<(), RendererError> {
        use crate::backend::command::{GpuBlitEncoder, GpuComputeEncoder};
        use crate::render_graph::{ComputeCommand, ComputeDispatchSize, RenderGraphBackend};
        for (index, operation) in record.commands.iter().enumerate() {
            match operation {
                ComputeCommand::Dispatch(dispatch) => {
                    let descriptor = &dispatch.pipeline;
                    let pipeline = self.compute_pipelines.get(descriptor).ok_or_else(|| {
                        RendererError::InvalidOperation(format!(
                            "Compute pipeline for '{}' was not prepared before encoding",
                            record.name
                        ))
                    })?;
                    let groups = match dispatch.size {
                        ComputeDispatchSize::Direct(groups) => groups,
                        ComputeDispatchSize::Indirect { .. } => [1, 1, 1],
                    };
                    if groups.contains(&0) {
                        continue;
                    }
                    for required in &pipeline.table_layout.bindings {
                        let binding = dispatch
                            .bindings
                            .iter()
                            .find(|binding| {
                                binding.group == required.group
                                    && binding.binding == required.binding
                            })
                            .ok_or_else(|| {
                                RendererError::InvalidOperation(format!(
                                    "Compute pass '{}' has no binding for {}:{}",
                                    record.name, required.group, required.binding
                                ))
                            })?;
                        let buffer = graph
                            .buffer_by_id(self, binding.resource, slot)
                            .ok_or_else(|| {
                                RendererError::InvalidOperation(format!(
                                    "Compute pass '{}' cannot resolve buffer {}",
                                    record.name, binding.resource.0
                                ))
                            })?;
                        let bound_size = binding
                            .range
                            .size
                            .min(native_buffer_scope(&buffer).saturating_sub(binding.range.offset));
                        if bound_size < required.minimum_buffer_bytes {
                            return Err(RendererError::InvalidOperation(format!(
                                "Compute buffer binding {}:{} is smaller than its reflected native type",
                                required.group, required.binding
                            )));
                        }
                        if binding.range.offset >= native_buffer_scope(&buffer)
                            || (binding.range.size != u64::MAX
                                && binding.range.size
                                    > native_buffer_scope(&buffer) - binding.range.offset)
                        {
                            return Err(RendererError::InvalidOperation(format!(
                                "Compute pass '{}' has a binding outside its native buffer range",
                                record.name
                            )));
                        }
                    }
                    if let ComputeDispatchSize::Indirect { resource, offset } = dispatch.size {
                        let buffer = graph.buffer_by_id(self, resource, slot).ok_or_else(|| {
                            RendererError::InvalidOperation(
                                "Unresolved indirect compute command".into(),
                            )
                        })?;
                        if !offset.is_multiple_of(4)
                            || offset
                                .checked_add(12)
                                .is_none_or(|end| end > native_buffer_scope(&buffer))
                        {
                            return Err(RendererError::InvalidOperation(
                                "Indirect compute command is outside its native buffer range"
                                    .into(),
                            ));
                        }
                    }
                    command.resources.capture_context(
                        Some(record.pass_index),
                        &format!("{}.dispatch.{index}", record.name),
                        Vec::new(),
                    );
                    let mut encoder = command.begin_compute_pass();
                    encoder
                        .inner
                        .setLabel(Some(&objc2_foundation::NSString::from_str(&format!(
                            "{}.slot.{slot}.dispatch.{index}",
                            record.name
                        ))));
                    if index > 0 {
                        let before = match &record.commands[index - 1] {
                            ComputeCommand::Dispatch(_) => objc2_metal::MTLStages::Dispatch,
                            _ => objc2_metal::MTLStages::Blit,
                        };
                        let after = match operation {
                            ComputeCommand::Dispatch(_) => objc2_metal::MTLStages::Dispatch,
                            _ => objc2_metal::MTLStages::Blit,
                        };
                        encoder
                            .inner
                            .barrierAfterQueueStages_beforeStages_visibilityOptions(
                                before,
                                after,
                                objc2_metal::MTL4VisibilityOptions::Device,
                            );
                        super::sync::capture_native_boundary(
                            &command.resources,
                            "inter_command",
                            super::sync::NativeBoundary::BetweenCommands {
                                previous_compute: matches!(
                                    &record.commands[index - 1],
                                    ComputeCommand::Dispatch(_)
                                ),
                                current_compute: matches!(operation, ComputeCommand::Dispatch(_)),
                            },
                            before,
                            after,
                            objc2_metal::MTL4VisibilityOptions::Device,
                        );
                    }
                    super::sync::apply_compiled_boundary(
                        encoder.inner.as_ref(),
                        graph,
                        record.pass_index,
                        objc2_metal::MTLStages::Dispatch,
                        &command.resources,
                    );
                    encoder.bind_compute_pipeline(pipeline);
                    for binding in &dispatch.bindings {
                        let Some(buffer) = graph.buffer_by_id(self, binding.resource, slot) else {
                            return Err(RendererError::InvalidOperation(format!(
                                "Compute pass '{}' cannot resolve buffer {}",
                                record.name, binding.resource.0
                            )));
                        };
                        let Some(native) = pipeline.table_layout.bindings.iter().find(|native| {
                            native.group == binding.group && native.binding == binding.binding
                        }) else {
                            continue;
                        };
                        let offset = <Self as RenderGraphBackend>::buffer_offset(&buffer)
                            + binding.range.offset;
                        let uniform = pipeline
                            .uniform_bindings
                            .contains(&(binding.group, binding.binding));
                        if uniform && !dispatch.constants.is_empty() {
                            encoder.set_push_constants(&dispatch.constants, native.index as u32);
                        } else {
                            command.resources.observe_resource(binding.resource.0);
                            encoder.bind_storage_buffer_range(
                                &buffer.buffer,
                                offset,
                                binding.range.size.min(
                                    native_buffer_scope(&buffer)
                                        .saturating_sub(binding.range.offset),
                                ),
                                native.index as u32,
                            );
                        }
                    }
                    match dispatch.size {
                        ComputeDispatchSize::Indirect { resource, offset } => {
                            let buffer =
                                graph.buffer_by_id(self, resource, slot).ok_or_else(|| {
                                    RendererError::InvalidOperation(
                                        "Unresolved indirect compute command".into(),
                                    )
                                })?;
                            command.resources.observe_resource(resource.0);
                            encoder.dispatch_indirect(
                                &buffer.buffer,
                                <Self as RenderGraphBackend>::buffer_offset(&buffer) + offset,
                            );
                        }
                        _ => encoder.dispatch(groups[0], groups[1], groups[2]),
                    }
                    encoder.end_encoding();
                }
                ComputeCommand::FillBuffer {
                    resource,
                    range,
                    value,
                } => {
                    let Some(buffer) = graph.buffer_by_id(self, *resource, slot) else {
                        return Err(RendererError::InvalidOperation(
                            "Unresolved fill buffer".into(),
                        ));
                    };
                    if range.offset >= native_buffer_scope(&buffer) {
                        return Err(RendererError::InvalidOperation(
                            "Fill buffer offset exceeds native range".into(),
                        ));
                    }
                    let available = native_buffer_scope(&buffer) - range.offset;
                    if range.size != u64::MAX && range.size > available {
                        return Err(RendererError::InvalidOperation(
                            "Fill buffer command exceeds its native range".into(),
                        ));
                    }
                    let fill_size = range.size.min(available);
                    command.resources.capture_context(
                        Some(record.pass_index),
                        &format!("{}.transfer.{index}", record.name),
                        match operation {
                            ComputeCommand::FillBuffer { resource, .. } => vec![resource.0],
                            ComputeCommand::CopyBuffer {
                                source,
                                destination,
                                ..
                            } => vec![source.0, destination.0],
                            _ => Vec::new(),
                        },
                    );
                    let mut encoder = command.begin_blit_pass();
                    encoder
                        .inner
                        .setLabel(Some(&objc2_foundation::NSString::from_str(&format!(
                            "{}.slot.{slot}.fill.{index}",
                            record.name
                        ))));
                    if index > 0 {
                        let before = match &record.commands[index - 1] {
                            ComputeCommand::Dispatch(_) => objc2_metal::MTLStages::Dispatch,
                            _ => objc2_metal::MTLStages::Blit,
                        };
                        let after = match operation {
                            ComputeCommand::Dispatch(_) => objc2_metal::MTLStages::Dispatch,
                            _ => objc2_metal::MTLStages::Blit,
                        };
                        encoder
                            .inner
                            .barrierAfterQueueStages_beforeStages_visibilityOptions(
                                before,
                                after,
                                objc2_metal::MTL4VisibilityOptions::Device,
                            );
                        super::sync::capture_native_boundary(
                            &command.resources,
                            "inter_command",
                            super::sync::NativeBoundary::BetweenCommands {
                                previous_compute: matches!(
                                    &record.commands[index - 1],
                                    ComputeCommand::Dispatch(_)
                                ),
                                current_compute: matches!(operation, ComputeCommand::Dispatch(_)),
                            },
                            before,
                            after,
                            objc2_metal::MTL4VisibilityOptions::Device,
                        );
                    }
                    super::sync::apply_compiled_boundary(
                        encoder.inner.as_ref(),
                        graph,
                        record.pass_index,
                        objc2_metal::MTLStages::Blit,
                        &command.resources,
                    );
                    if value
                        .to_le_bytes()
                        .iter()
                        .all(|byte| *byte == value.to_le_bytes()[0])
                    {
                        encoder.fill_buffer(
                            &buffer.buffer,
                            buffer.offset + range.offset,
                            fill_size,
                            *value as u8,
                        );
                    } else {
                        let bytes = value
                            .to_le_bytes()
                            .into_iter()
                            .cycle()
                            .take(fill_size as usize)
                            .collect::<Vec<_>>();
                        let native = command.resources.inline_bytes(&bytes);
                        let staging = super::buffer::MetalBuffer::new(native, fill_size);
                        encoder.copy_buffer_to_buffer(
                            &staging,
                            0,
                            &buffer.buffer,
                            buffer.offset + range.offset,
                            fill_size,
                        );
                    }
                    encoder.end_encoding();
                }
                ComputeCommand::CopyBuffer {
                    source,
                    destination,
                    source_offset,
                    destination_offset,
                    size,
                } => {
                    let source_buffer = graph.buffer_by_id(self, *source, slot);
                    let destination_buffer = graph.buffer_by_id(self, *destination, slot);
                    let (Some(source_buffer), Some(destination_buffer)) =
                        (source_buffer, destination_buffer)
                    else {
                        return Err(RendererError::InvalidOperation(
                            "Unresolved copy buffer".into(),
                        ));
                    };
                    validate_copy_scope(
                        native_buffer_scope(&source_buffer),
                        native_buffer_scope(&destination_buffer),
                        *source_offset,
                        *destination_offset,
                        *size,
                    )?;
                    if source_buffer.buffer.inner == destination_buffer.buffer.inner {
                        let source = crate::render_graph::BufferByteRange::new(
                            source_buffer.offset + source_offset,
                            *size,
                        );
                        let destination = crate::render_graph::BufferByteRange::new(
                            destination_buffer.offset + destination_offset,
                            *size,
                        );
                        if source.intersection(destination).is_some() {
                            return Err(RendererError::InvalidOperation(
                                "Copy buffer command has overlapping native aliases".into(),
                            ));
                        }
                    }
                    command.resources.capture_context(
                        Some(record.pass_index),
                        &format!("{}.transfer.{index}", record.name),
                        match operation {
                            ComputeCommand::FillBuffer { resource, .. } => vec![resource.0],
                            ComputeCommand::CopyBuffer {
                                source,
                                destination,
                                ..
                            } => vec![source.0, destination.0],
                            _ => Vec::new(),
                        },
                    );
                    let mut encoder = command.begin_blit_pass();
                    encoder
                        .inner
                        .setLabel(Some(&objc2_foundation::NSString::from_str(&format!(
                            "{}.slot.{slot}.copy.{index}",
                            record.name
                        ))));
                    if index > 0 {
                        let before = match &record.commands[index - 1] {
                            ComputeCommand::Dispatch(_) => objc2_metal::MTLStages::Dispatch,
                            _ => objc2_metal::MTLStages::Blit,
                        };
                        let after = match operation {
                            ComputeCommand::Dispatch(_) => objc2_metal::MTLStages::Dispatch,
                            _ => objc2_metal::MTLStages::Blit,
                        };
                        encoder
                            .inner
                            .barrierAfterQueueStages_beforeStages_visibilityOptions(
                                before,
                                after,
                                objc2_metal::MTL4VisibilityOptions::Device,
                            );
                        super::sync::capture_native_boundary(
                            &command.resources,
                            "inter_command",
                            super::sync::NativeBoundary::BetweenCommands {
                                previous_compute: matches!(
                                    &record.commands[index - 1],
                                    ComputeCommand::Dispatch(_)
                                ),
                                current_compute: matches!(operation, ComputeCommand::Dispatch(_)),
                            },
                            before,
                            after,
                            objc2_metal::MTL4VisibilityOptions::Device,
                        );
                    }
                    super::sync::apply_compiled_boundary(
                        encoder.inner.as_ref(),
                        graph,
                        record.pass_index,
                        objc2_metal::MTLStages::Blit,
                        &command.resources,
                    );
                    encoder.copy_buffer_to_buffer(
                        &source_buffer.buffer,
                        source_buffer.offset + source_offset,
                        &destination_buffer.buffer,
                        destination_buffer.offset + destination_offset,
                        *size,
                    );
                    encoder.end_encoding();
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod copy_scope_tests {
    use super::validate_copy_scope;
    #[test]
    fn test_native_copy_rejects_declared_scope_overflow() {
        assert!(validate_copy_scope(16, 16, 0, 0, 16).is_ok());
        assert!(validate_copy_scope(16, 16, 8, 0, 12).is_err());
        assert!(validate_copy_scope(16, 16, 0, 8, 12).is_err());
        assert!(validate_copy_scope(16, 16, u64::MAX, 0, 4).is_err());
    }
}
