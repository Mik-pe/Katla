//! Native encoders consume resolved graph attachments and pass-local commands.

use super::attachments::ResolvedMetalAttachments;
use super::execution_plan::{MetalExecutionPlan, MetalPassRecord};
use super::metal_renderer::MetalRenderer;
use super::render_encoder::MetalRenderEncoder;
use super::texture::{MetalTexture, MetalTextureView};
use crate::backend::command::{GpuCommandBuffer, GpuRenderEncoder, IndexType, ShaderStages};
use crate::error::RendererError;
use crate::render_graph::{FrameGraph, PassExecutionData, PassKind};
use crate::renderer::gpu_renderer::GpuRenderer;
use crate::renderer::types::{DrawList, UIDrawList};
use objc2_metal::{MTLCommandBuffer, MTLCommandEncoder, MTLRenderCommandEncoder};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

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
        let texture = self
            .current_drawable_texture
            .take()
            .ok_or_else(|| RendererError::InvalidOperation("No drawable texture".into()))?;
        let drawable = MetalTextureView::new(
            texture.clone(),
            MetalTexture::new(texture, crate::texture::ImageFormat::B8G8R8A8Srgb),
        );
        let slot = <Self as crate::render_graph::RenderGraphBackend>::current_frame(self);
        let resolved = plan
            .passes()
            .iter()
            .map(|record| {
                let attachments =
                    ResolvedMetalAttachments::resolve(record, graph, &drawable, slot)?;
                validate_builtin_attachments(record, &attachments)?;
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
                Ok(attachments)
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
        let mut cmd_buffer = self
            .context
            .create_command_buffer_with_diagnostics(self.gpu_diagnostics_mode);
        cmd_buffer.begin();
        cmd_buffer
            .inner
            .setLabel(Some(&objc2_foundation::NSString::from_str(&format!(
                "render_graph_frame.{}",
                self.frame_index
            ))));
        if self.texture_uploads.has_pending() {
            use crate::backend::command::GpuBlitEncoder;
            let mut blit = cmd_buffer.begin_blit_pass_with_label("texture_upload");
            self.texture_uploads.encode_into(&mut blit);
            blit.end_encoding();
        }
        log::debug!("Metal execution plan: {}", plan.trace().join(" -> "));
        let mut execution_trace = crate::render_graph::ResourceExecutionTrace::new();
        for (position, (record, attachments)) in plan.passes().iter().zip(resolved).enumerate() {
            let data = pending.remove(&record.pass_index).unwrap_or_default();
            let counts = data.prepared_counts();
            let width = attachments.width;
            let height = attachments.height;
            // Picking observes the attachment selected by this pass, including its frame slot.
            if record.kind == PassKind::ObjectId {
                self.picking
                    .set_render_target(attachments.info.color_attachments[0].view.clone());
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
        cmd_buffer.end();
        self.context.surface.present(&cmd_buffer.inner);
        self.last_command_buffer = Some(cmd_buffer.inner.clone());
        cmd_buffer.submit(&self.context);
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
                unsafe {
                    encoder
                        .inner
                        .setFragmentTexture_atIndex(Some(&texture.view.inner), 1);
                }
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
            Some(&self.skeletons),
            resolution,
            self.current_frame_uniform_buffer()
                .ok_or_else(|| RendererError::InvalidOperation("Frame uniforms missing".into()))?,
            self.current_object_storage_buffer()
                .ok_or_else(|| RendererError::InvalidOperation("Object storage missing".into()))?,
            self.shadow_cascade_encode_buffer
                .as_ref()
                .ok_or_else(|| RendererError::InvalidOperation("Cascade data missing".into()))?,
            self.buffer_sizes_buffer.as_ref(),
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
            &self.skeletons,
            self.bindless_manager.argument_buffer(),
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
            &self.skeletons,
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
                &self.skeletons,
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
                &self.skeletons,
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

    fn fullscreen_input<'a>(
        &self,
        record: &MetalPassRecord,
        graph: &'a FrameGraph<Self>,
        slot: usize,
    ) -> Result<&'a super::metal_transient_texture::MetalTransientTexture, RendererError> {
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
        let input = graph
            .transient_texture_by_id(inputs[0].resource, slot)
            .ok_or_else(|| {
                RendererError::InvalidOperation(format!(
                    "Fullscreen pass '{}' has an unresolved sampled input",
                    record.name
                ))
            })?;
        if input.bindless_slot.is_none() || self.tonemap_pipeline.is_none() {
            return Err(RendererError::InvalidOperation(format!(
                "Fullscreen pass '{}' has no sampling slot or tonemap pipeline",
                record.name
            )));
        }
        Ok(input)
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
        let input = self.fullscreen_input(record, graph, slot)?;
        let hdr_slot = input.bindless_slot.ok_or_else(|| {
            RendererError::InvalidOperation(
                "Fullscreen input is not registered for sampling".into(),
            )
        })?;
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
        if let Some(argument_buffer) = self.bindless_manager.argument_buffer() {
            unsafe {
                encoder
                    .inner
                    .setVertexBuffer_offset_atIndex(Some(argument_buffer), 0, 9);
                encoder
                    .inner
                    .setFragmentBuffer_offset_atIndex(Some(argument_buffer), 0, 9);
            }
            encoder.use_buffer(
                argument_buffer,
                objc2_metal::MTLResourceUsage::Read,
                objc2_metal::MTLRenderStages::Fragment,
            );
        }
        if let Some(ref sampler) = self.shared_sampler {
            unsafe {
                encoder
                    .inner
                    .setFragmentSamplerState_atIndex(Some(&sampler.inner), 0);
            }
        }
        if let Some(ref buffer_sizes) = self.buffer_sizes_buffer {
            encoder.bind_storage_buffer(buffer_sizes, 0, 8, ShaderStages::VERTEX_FRAGMENT);
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
        encoder.use_texture(
            &input.view.inner,
            objc2_metal::MTLResourceUsage::Read,
            objc2_metal::MTLRenderStages::Fragment,
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
        self.ui_renderer
            .upload_draw_list(&self.context, draw_list)?;
        let material = record
            .material
            .ok_or_else(|| RendererError::InvalidOperation("UI pass has no material".into()))?;
        let pipeline = self.material_pipeline(material, record.color_attachments[0].format)?;
        encoder.bind_graphics_pipeline(&pipeline);
        if let Some(argument_buffer) = self.bindless_manager.argument_buffer() {
            unsafe {
                encoder
                    .inner
                    .setVertexBuffer_offset_atIndex(Some(argument_buffer), 0, 9);
                encoder
                    .inner
                    .setFragmentBuffer_offset_atIndex(Some(argument_buffer), 0, 9);
            }
            encoder.use_buffer(
                argument_buffer,
                objc2_metal::MTLResourceUsage::Read,
                objc2_metal::MTLRenderStages::Vertex | objc2_metal::MTLRenderStages::Fragment,
            );
            for texture in self.bindless_manager.registered_textures() {
                encoder.use_texture(
                    texture,
                    objc2_metal::MTLResourceUsage::Read,
                    objc2_metal::MTLRenderStages::Vertex | objc2_metal::MTLRenderStages::Fragment,
                );
            }
        }
        if let Some(ref sampler) = self.shared_sampler {
            unsafe {
                encoder
                    .inner
                    .setFragmentSamplerState_atIndex(Some(&sampler.inner), 0);
            }
        }
        if let Some(vertex_buffer) = self.ui_renderer.vertex_buffer() {
            encoder.bind_vertex_buffer(vertex_buffer, 0, 10);
        }
        if let Some(index_buffer) = self.ui_renderer.index_buffer() {
            encoder.bind_index_buffer(index_buffer, 0, IndexType::Uint32);
        }
        if let Some(ref buffer_sizes) = self.buffer_sizes_buffer {
            encoder.bind_storage_buffer(buffer_sizes, 0, 8, ShaderStages::VERTEX_FRAGMENT);
        }
        self.ui_renderer
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
