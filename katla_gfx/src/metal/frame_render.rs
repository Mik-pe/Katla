//! Native encoders consume resolved graph attachments and pass-local commands.

use super::attachments::ResolvedMetalAttachments;
use super::execution_plan::{MetalExecutionPlan, MetalPassRecord};
use super::metal_renderer::MetalRenderer;
use super::render_encoder::MetalRenderEncoder;
use super::texture::{MetalTexture, MetalTextureView};
use crate::backend::command::{GpuCommandBuffer, GpuRenderEncoder, IndexType, ShaderStages};
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
        if record.kind == PassKind::Ui
            && pending
                .get(&record.pass_index)
                .is_some_and(|data| data.ui_draw_lists.len() > 1)
        {
            return Err(RendererError::InvalidOperation(format!(
                "Metal UI pass '{}' received {} UI draw lists; submit one composed list per PassId",
                record.name,
                pending[&record.pass_index].ui_draw_lists.len()
            )));
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
                validate_builtin_attachments(record, &attachments)?;
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
                if record.kind == PassKind::Fullscreen { self.fullscreen_input(record, graph, slot)?; }
                if record.kind == PassKind::Geometry {
                    if record.color_attachments[0].load_op == crate::render_pass::LoadOp::Clear && self.sky_pipeline.is_some()
                        && (record.color_attachments[0].format != crate::texture::ImageFormat::R16G16B16A16Sfloat || record.depth_attachment.is_none()) {
                        return Err(RendererError::InvalidOperation(format!("Geometry pass '{}' has attachments incompatible with its sky pipeline", record.name)));
                    }
                    if let Some(data) = pending.get(&record.pass_index) {
                        for draw in data.prepared().iter() {
                            if let Some(material) = self.materials.get(draw.material) {
                                let key = crate::renderer::pipeline_variant::PipelineVariantKey::resolve(&material.descriptor, record.color_attachments[0].format);
                                if key.depth_format() != record.depth_attachment.map(|attachment| attachment.format) {
                                    return Err(RendererError::InvalidOperation(format!("Geometry pass '{}' has depth incompatible with its material", record.name)));
                                }
                            }
                        }
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
            self.preflight_graphics_record(&cmd_buffer.resources, record, data, graph, slot)?;
        }
        cmd_buffer.begin();
        if let Some(queries) = &self.timestamp_queries {
            let generation = self.frame_slots[slot].generation;
            self.frame_slots[slot].timestamps.begin(
                &cmd_buffer.inner,
                queries.labels(),
                generation,
            );
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
            let mut blit = cmd_buffer.begin_blit_pass_with_label("texture_upload");
            blit.inner
                .barrierAfterQueueStages_beforeStages_visibilityOptions(
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
            blit.end_encoding();
        }
        log::debug!("Metal execution plan: {}", plan.trace().join(" -> "));
        let mut picking_target = None;
        let mut execution_trace = crate::render_graph::ResourceExecutionTrace::new();
        for (position, (record, attachments)) in plan.passes().iter().zip(resolved).enumerate() {
            let data = pending.remove(&record.pass_index).unwrap_or_default();
            if record.pass_type != crate::render_graph::PassType::Graphics {
                self.encode_compute_record(&mut cmd_buffer, record, graph, slot)?;
                if trace_enabled {
                    execution_trace.push(crate::render_graph::ResourceExecutionTraceEntry {
                        pass_index: record.pass_index,
                        name: record.name.clone(),
                        pass_type: record.pass_type,
                        encode_position: position,
                        outcome: crate::render_graph::EmittedPassOutcome::Encoded,
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
            // Picking observes the attachment selected by this pass, including its frame slot.
            if record.kind == PassKind::ObjectId {
                picking_target = Some(attachments.info.color_attachments[0].view.clone());
            }
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
            let mut encoder = cmd_buffer.begin_render_pass(attachments.info);
            encoder
                .inner
                .setLabel(Some(&objc2_foundation::NSString::from_str(&record.name)));
            super::sync::apply_compiled_boundary(
                encoder.inner.as_ref(),
                graph,
                record.pass_index,
                objc2_metal::MTLStages::Vertex | objc2_metal::MTLStages::Fragment,
            );
            encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
            encoder.set_scissor(0, 0, width, height);
            match record.kind {
                PassKind::Shadow => {
                    self.encode_shadow_record(&mut encoder, record, &data, width)?
                }
                PassKind::DepthPrepass => {
                    self.encode_depth_prepass_record(&mut encoder, record, &data, width, height)?
                }
                PassKind::Geometry => {
                    self.encode_geometry_record(&mut encoder, record, &data, graph, slot)?
                }
                PassKind::ObjectId => {
                    self.encode_object_id_record(&mut encoder, record, &data, width, height)?
                }
                PassKind::Outline => {
                    self.encode_outline_record(&mut encoder, record, &data, width, height)?
                }
                PassKind::Fullscreen => {
                    self.encode_fullscreen_record(&mut encoder, record, graph, slot)?
                }
                PassKind::Ui => self.encode_ui_record(
                    &mut encoder,
                    record,
                    data.ui_draw_lists.first(),
                    width,
                    height,
                )?,
                PassKind::Particles => {
                    self.encode_particle_record(&mut encoder, record, width, height)?
                }
                PassKind::StencilIndicator | PassKind::Compositing => {
                    unreachable!("unsupported Metal handler")
                }
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
            let terminal = cmd_buffer.begin_compute_pass();
            super::sync::apply_final_image_boundary(terminal.inner.as_ref(), graph);
            terminal.end_encoding();
        }
        cmd_buffer.resources.check()?;
        self.frame_slots[slot].timestamps.end(&cmd_buffer.inner);
        cmd_buffer.end();
        graph.record_buffer_execution(self);
        self.pending_frame = Some(super::frame_lifecycle::MetalPendingFrame {
            command: cmd_buffer,
            defined_outputs,
            picking_target,
        });
        Ok(execution_trace)
    }

    fn encode_geometry_record(
        &self,
        encoder: &mut MetalRenderEncoder,
        record: &MetalPassRecord,
        data: &PassExecutionData,
        graph: &FrameGraph<Self>,
        slot: usize,
    ) -> Result<(), RendererError> {
        Self::bind_common_resources(self, encoder);
        for access in &record.image_accesses {
            if access.usage == crate::render_graph::ResourceAccessUsage::Sampled
                && let Some(texture) = graph.transient_texture_by_id(access.resource, slot)
                && matches!(texture.format, crate::texture::ImageFormat::D32Sfloat)
            {
                encoder.bind_native_texture(
                    &texture.view.inner,
                    1,
                    crate::backend::command::ShaderStages::FRAGMENT,
                );

                encoder.use_texture(
                    &texture.view.inner,
                    objc2_metal::MTLResourceUsage::Read,
                    objc2_metal::MTLRenderStages::Fragment,
                );
            }
        }
        if record.color_attachments[0].load_op == crate::render_pass::LoadOp::Clear
            && let Some(ref pipeline) = self.sky_pipeline
        {
            if let Some(ref buffer) = self.dummy_vertex_buffer {
                encoder.bind_vertex_buffer(buffer, 0, 10);
            }
            encoder.bind_graphics_pipeline(pipeline);
            encoder.draw(3, 1, 0, 0);
        }
        Self::draw_objects(
            self,
            encoder,
            record.color_attachments[0].format,
            data.prepared(),
        );
        Ok(())
    }

    fn encode_shadow_record(
        &self,
        encoder: &mut MetalRenderEncoder,
        _record: &MetalPassRecord,
        data: &PassExecutionData,
        resolution: u32,
    ) -> Result<(), RendererError> {
        if data.prepared().is_empty() {
            return Ok(());
        }
        let pipeline = self
            .shadow
            .pipeline()
            .ok_or_else(|| RendererError::InvalidOperation("Shadow pipeline missing".into()))?;
        super::shadow::render_cascades(
            encoder,
            pipeline,
            self.shadow.pipeline_skinned(),
            Some(&self.skeletons[self.frame_index()]),
            resolution,
            self.current_frame_uniform_buffer()
                .ok_or_else(|| RendererError::InvalidOperation("Frame uniforms missing".into()))?,
            self.current_object_storage_buffer()
                .ok_or_else(|| RendererError::InvalidOperation("Object storage missing".into()))?,
            self.shadow_cascade_encode_buffers[self.frame_index()]
                .as_ref()
                .ok_or_else(|| RendererError::InvalidOperation("Cascade data missing".into()))?,
            self.shadow.cascade_count(),
            &self.meshes,
            &self.materials,
            data.prepared(),
        );
        Ok(())
    }

    fn encode_depth_prepass_record(
        &self,
        encoder: &mut MetalRenderEncoder,
        _record: &MetalPassRecord,
        data: &PassExecutionData,
        width: u32,
        height: u32,
    ) -> Result<(), RendererError> {
        if data.prepared().is_empty() {
            return Ok(());
        }
        let pipeline = self
            .depth_prepass
            .pipeline()
            .ok_or_else(|| RendererError::InvalidOperation("Depth pipeline missing".into()))?;
        super::depth_prepass::render_depth_prepass(
            encoder,
            pipeline,
            self.depth_prepass.pipeline_skinned(),
            self.depth_prepass.pipeline_billboard(),
            width,
            height,
            self.current_frame_uniform_buffer()
                .ok_or_else(|| RendererError::InvalidOperation("Frame uniforms missing".into()))?,
            self.current_object_storage_buffer()
                .ok_or_else(|| RendererError::InvalidOperation("Object storage missing".into()))?,
            &self.meshes,
            &self.materials,
            data.prepared(),
            &self.skeletons[self.frame_index()],
            self.bindless_manager.snapshot(),
            self.shared_sampler.as_ref(),
        );
        Ok(())
    }

    fn encode_object_id_record(
        &self,
        encoder: &mut MetalRenderEncoder,
        _record: &MetalPassRecord,
        data: &PassExecutionData,
        width: u32,
        height: u32,
    ) -> Result<(), RendererError> {
        if data.prepared().is_empty() {
            return Ok(());
        }
        let pipeline = self
            .picking
            .pipeline()
            .ok_or_else(|| RendererError::InvalidOperation("Picking pipeline missing".into()))?;
        super::picking::render_object_id_pass(
            encoder,
            pipeline,
            self.picking.pipeline_skinned(),
            width,
            height,
            self.current_frame_uniform_buffer()
                .ok_or_else(|| RendererError::InvalidOperation("Frame uniforms missing".into()))?,
            self.current_object_storage_buffer()
                .ok_or_else(|| RendererError::InvalidOperation("Object storage missing".into()))?,
            &self.meshes,
            &self.materials,
            data.prepared(),
            &self.skeletons[self.frame_index()],
        );
        Ok(())
    }

    fn encode_outline_record(
        &self,
        encoder: &mut MetalRenderEncoder,
        _record: &MetalPassRecord,
        data: &PassExecutionData,
        width: u32,
        height: u32,
    ) -> Result<(), RendererError> {
        if data.prepared().is_empty() {
            return Ok(());
        }
        let frame = self
            .current_frame_uniform_buffer()
            .ok_or_else(|| RendererError::InvalidOperation("Frame uniforms missing".into()))?;
        let objects = self
            .current_object_storage_buffer()
            .ok_or_else(|| RendererError::InvalidOperation("Object storage missing".into()))?;
        if let Some(pipeline) = self.outline.stencil_mark_pipeline() {
            super::outline::render_stencil_mark(
                encoder,
                pipeline,
                self.outline.stencil_mark_skinned_pipeline(),
                width,
                height,
                frame,
                objects,
                &self.meshes,
                &self.materials,
                data.prepared(),
                &self.skeletons[self.frame_index()],
            );
        }
        if let Some(pipeline) = self.outline.outline_draw_pipeline() {
            super::outline::render_outline(
                encoder,
                pipeline,
                self.outline.outline_draw_skinned_pipeline(),
                width,
                height,
                frame,
                objects,
                &self.meshes,
                &self.materials,
                data.prepared(),
                &self.skeletons[self.frame_index()],
            );
        }
        Ok(())
    }

    fn encode_particle_record(
        &self,
        encoder: &mut MetalRenderEncoder,
        _record: &MetalPassRecord,
        width: u32,
        height: u32,
    ) -> Result<(), RendererError> {
        if let Some(system) = &self.particle_system
            && let Some(pipeline) = system.render_pipeline()
        {
            super::particle::render_particles(
                encoder,
                pipeline,
                width,
                height,
                self.current_frame_uniform_buffer().ok_or_else(|| {
                    RendererError::InvalidOperation("Frame uniforms missing".into())
                })?,
                system,
                self.frame_index,
            );
        }
        Ok(())
    }

    fn fullscreen_input(
        &self,
        record: &MetalPassRecord,
        graph: &FrameGraph<Self>,
        slot: usize,
    ) -> Result<u32, RendererError> {
        let inputs = record
            .image_accesses
            .iter()
            .filter(|access| access.usage == crate::render_graph::ResourceAccessUsage::Sampled)
            .collect::<Vec<_>>();
        if inputs.len() != 1 {
            return Err(RendererError::InvalidOperation(format!(
                "Fullscreen pass '{}' requires exactly one declared sampled input",
                record.name
            )));
        }
        let resource = inputs[0].resource;
        let sampling_slot = if let Some(handle) = graph.imported_images.get(&resource) {
            self.textures
                .get(*handle)
                .and_then(|entry| entry.bindless_slot)
        } else {
            graph
                .transient_texture_by_id(resource, slot)
                .and_then(|texture| texture.bindless_slot)
        };
        sampling_slot
            .filter(|_| self.tonemap_pipeline.is_some())
            .ok_or_else(|| {
                RendererError::InvalidOperation(format!(
                    "Fullscreen pass '{}' has an unresolved sampled input or pipeline",
                    record.name
                ))
            })
    }

    fn encode_fullscreen_record(
        &self,
        encoder: &mut MetalRenderEncoder,
        record: &MetalPassRecord,
        graph: &FrameGraph<Self>,
        slot: usize,
    ) -> Result<(), RendererError> {
        let tonemap_pipeline = self
            .tonemap_pipeline
            .as_ref()
            .ok_or_else(|| RendererError::InvalidOperation("Tonemap pipeline missing".into()))?;
        let hdr_slot = self.fullscreen_input(record, graph, slot)?;
        let mut uniforms = self.frame_uniforms.clone();
        if let Some(params) = record.tonemap_params {
            uniforms.tonemap = [
                params.exposure,
                params.gamma,
                params.mode as u32 as f32,
                hdr_slot as f32,
            ];
        } else {
            uniforms.tonemap[3] = hdr_slot as f32;
        }
        if let Some(snapshot) = self.bindless_manager.snapshot() {
            encoder.bind_bindless(snapshot);
        }
        if let Some(ref sampler) = self.shared_sampler {
            encoder.bind_native_sampler(
                &sampler.inner,
                0,
                crate::backend::command::ShaderStages::FRAGMENT,
            );
        }
        if let Some(ref dummy_vertex_buffer) = self.dummy_vertex_buffer {
            encoder.bind_vertex_buffer(dummy_vertex_buffer, 0, 10);
        }
        encoder.set_push_constants(
            unsafe {
                // FrameUniforms is repr(C) and consists of initialized scalar arrays.
                std::slice::from_raw_parts(
                    &uniforms as *const crate::renderer::types::FrameUniforms as *const u8,
                    std::mem::size_of_val(&uniforms),
                )
            },
            0,
            ShaderStages::VERTEX_FRAGMENT,
        );
        encoder.bind_graphics_pipeline(tonemap_pipeline);
        encoder.draw(3, 1, 0, 0);

        Ok(())
    }

    fn encode_ui_record(
        &mut self,
        encoder: &mut MetalRenderEncoder,
        record: &MetalPassRecord,
        draw_list: Option<&UIDrawList>,
        width: u32,
        height: u32,
    ) -> Result<(), RendererError> {
        let Some(draw_list) = draw_list.filter(|list| !list.is_empty()) else {
            return Ok(());
        };
        let material = record
            .material
            .ok_or_else(|| RendererError::InvalidOperation("UI pass has no material".into()))?;
        let pipeline = self.material_pipeline(material, record.color_attachments[0].format)?;
        encoder.bind_graphics_pipeline(&pipeline);
        if let Some(snapshot) = self.bindless_manager.snapshot() {
            encoder.bind_bindless(snapshot);
        }
        if let Some(ref sampler) = self.shared_sampler {
            encoder.bind_native_sampler(
                &sampler.inner,
                0,
                crate::backend::command::ShaderStages::FRAGMENT,
            );
        }
        if let Some(vertex_buffer) = self.ui_renderers[self.frame_index()].vertex_buffer() {
            encoder.bind_vertex_buffer(vertex_buffer, 0, 10);
        }
        if let Some(index_buffer) = self.ui_renderers[self.frame_index()].index_buffer() {
            encoder.bind_index_buffer(index_buffer, 0, IndexType::Uint32);
        }
        self.ui_renderers[self.frame_index()]
            .render_ui_commands(encoder, draw_list, &pipeline, width, height);
        Ok(())
    }
}

fn validate_builtin_attachments(
    record: &MetalPassRecord,
    attachments: &ResolvedMetalAttachments,
) -> Result<(), RendererError> {
    use crate::texture::ImageFormat;
    let formats = record
        .color_attachments
        .iter()
        .map(|attachment| attachment.format)
        .collect::<Vec<_>>();
    let depth = record.depth_attachment.map(|attachment| attachment.format);
    let ds = ImageFormat::D32SfloatS8Uint;
    let valid = match record.kind {
        PassKind::Shadow => {
            formats.is_empty()
                && depth == Some(ImageFormat::D32Sfloat)
                && attachments.width == attachments.height
        }
        PassKind::DepthPrepass => formats.is_empty() && depth == Some(ds),
        PassKind::ObjectId => formats == [ImageFormat::R32Uint] && depth == Some(ds),
        PassKind::Particles | PassKind::Outline => {
            formats == [ImageFormat::R16G16B16A16Sfloat] && depth == Some(ds)
        }
        PassKind::Geometry => formats.len() == 1 && (depth.is_none() || depth == Some(ds)),
        PassKind::Fullscreen => formats == [ImageFormat::B8G8R8A8Srgb] && depth.is_none(),
        PassKind::Ui => formats == [ImageFormat::B8G8R8A8Srgb] && depth.is_none(),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(RendererError::InvalidOperation(format!(
            "Metal pass '{}' has attachments incompatible with its built-in encoder ({:?})",
            record.name, record.kind
        )))
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
        use crate::render_graph::{
            BuiltinComputeKernel, ComputeCommand, ComputeDispatchSize, ComputeKernel,
            RenderGraphBackend,
        };
        for (index, operation) in record.commands.iter().enumerate() {
            match operation {
                ComputeCommand::Dispatch(dispatch) => {
                    let descriptor = dispatch.kernel.descriptor_ref();
                    let pipeline = self.compute_pipelines.get(descriptor).ok_or_else(|| {
                        RendererError::InvalidOperation(format!(
                            "Compute pipeline for '{}' was not prepared before encoding",
                            record.name
                        ))
                    })?;
                    let builtin = match &dispatch.kernel {
                        ComputeKernel::Builtin(kernel) => Some(*kernel),
                        _ => None,
                    };
                    let groups = match dispatch.size {
                        ComputeDispatchSize::Direct(groups) => groups,
                        ComputeDispatchSize::Frame => match builtin {
                            Some(BuiltinComputeKernel::AnimationPose) => [
                                graph
                                    .frame_parameters()
                                    .animation_skeleton_count
                                    .div_ceil(64),
                                1,
                                1,
                            ],
                            Some(BuiltinComputeKernel::LightCulling) => self
                                .light_culling
                                .as_ref()
                                .map(|light| light.dispatch_size())
                                .unwrap_or([0, 0, 0]),
                            Some(BuiltinComputeKernel::ParticleEmit) => {
                                [graph.frame_parameters().particle_emit_workgroup_count, 1, 1]
                            }
                            Some(BuiltinComputeKernel::ParticleSimulate) => [
                                graph.frame_parameters().particle_simulate_workgroup_count,
                                1,
                                1,
                            ],
                            Some(BuiltinComputeKernel::ParticleDrawCommand) => [
                                u32::from(
                                    graph.frame_parameters().particle_simulate_workgroup_count > 0,
                                ),
                                1,
                                1,
                            ],
                            None => {
                                return Err(RendererError::InvalidOperation(
                                    "Custom compute dispatch requires explicit workgroups".into(),
                                ));
                            }
                        },
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
                    }
                    super::sync::apply_compiled_boundary(
                        encoder.inner.as_ref(),
                        graph,
                        record.pass_index,
                        objc2_metal::MTLStages::Dispatch,
                    );
                    encoder.bind_compute_pipeline(pipeline);
                    for binding in &dispatch.bindings {
                        let Some(buffer) = graph.buffer_by_id(self, binding.resource, slot) else {
                            if builtin.is_some() && graph.is_builtin_buffer(binding.resource) {
                                continue;
                            }
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
                        if graph.is_builtin_buffer(*resource) {
                            continue;
                        }
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
                    }
                    super::sync::apply_compiled_boundary(
                        encoder.inner.as_ref(),
                        graph,
                        record.pass_index,
                        objc2_metal::MTLStages::Blit,
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
                        if graph.is_builtin_buffer(*source) || graph.is_builtin_buffer(*destination)
                        {
                            continue;
                        }
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
                    }
                    super::sync::apply_compiled_boundary(
                        encoder.inner.as_ref(),
                        graph,
                        record.pass_index,
                        objc2_metal::MTLStages::Blit,
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
