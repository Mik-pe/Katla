//! Reflected graphics encoding consumes application-authored pass inputs.

use super::binding_schema::TableBindingKind;
use super::execution_plan::MetalPassRecord;
use super::metal_renderer::MetalRenderer;
use super::pipeline::MetalGraphicsPipeline;
use super::render_encoder::MetalRenderEncoder;
use crate::backend::command::{GpuRenderEncoder, IndexType, ShaderStages};
use crate::error::RendererError;
use crate::handle::MaterialHandle;
use crate::render_graph::{FrameGraph, PassExecutionData};
use crate::renderer::frame_bindings::{PassBindings, PassDraw, PassDrawPhase, PassPipeline};

#[derive(Clone, Copy)]
pub(super) struct GraphicsFrame<'a> {
    pub(super) graph: &'a FrameGraph<MetalRenderer>,
    pub(super) slot: usize,
    pub(super) size: crate::size::Size2D,
}

pub(super) fn phases(record: &MetalPassRecord) -> Vec<PassDrawPhase> {
    if !record.bindings.phases.is_empty() {
        return record.bindings.phases.clone();
    }
    vec![PassDrawPhase {
        samplers: Vec::new(),
        pipelines: record.bindings.pipelines.clone(),
        constants: Vec::new(),
        draw: PassDraw::Submissions,
        viewport: None,
    }]
}

pub(super) fn table_slots(
    pipeline: &MetalGraphicsPipeline,
    group: u32,
    binding: u32,
    kind: TableBindingKind,
    stages: ShaderStages,
) -> Vec<(u32, ShaderStages)> {
    let mut slots = Vec::new();
    for (layout, stage, enabled) in [
        (&pipeline.vertex_layout, ShaderStages::VERTEX, stages.vertex),
        (
            pipeline
                .fragment_layout
                .as_ref()
                .unwrap_or(&pipeline.vertex_layout),
            ShaderStages::FRAGMENT,
            stages.fragment && pipeline.fragment_layout.is_some(),
        ),
    ] {
        if enabled
            && let Some(slot) = layout
                .bindings
                .iter()
                .find(|slot| slot.group == group && slot.binding == binding && slot.kind == kind)
        {
            slots.push((slot.index as u32, stage));
        }
    }
    slots
}

impl MetalRenderer {
    pub(crate) fn prepare_packet_samplers(
        &mut self,
        record: &MetalPassRecord,
    ) -> Result<(), RendererError> {
        for mode in record
            .bindings
            .samplers
            .iter()
            .chain(
                record
                    .bindings
                    .phases
                    .iter()
                    .flat_map(|phase| &phase.samplers),
            )
            .map(|binding| binding.sampling)
        {
            if self
                .packet_samplers
                .iter()
                .any(|(existing, _)| *existing == mode)
            {
                continue;
            }
            self.packet_samplers
                .push((mode, self.context.create_sampler(mode)?));
        }
        Ok(())
    }

    pub(super) fn packet_material(
        &self,
        pipelines: &[PassPipeline],
        draw: &crate::renderer::types::DrawCall,
    ) -> Result<MaterialHandle, RendererError> {
        if pipelines.is_empty() {
            return Ok(draw.material);
        }
        let mesh = self
            .meshes
            .get(draw.mesh)
            .ok_or_else(|| RendererError::StaleHandle {
                resource: "mesh".into(),
                detail: "graphics packet".into(),
            })?;
        pipelines
            .iter()
            .find(|pipeline| pipeline.vertex_layout == mesh.layout)
            .map(|pipeline| pipeline.material)
            .ok_or_else(|| {
                RendererError::InvalidOperation(
                    "No pass pipeline matches the submitted mesh layout".into(),
                )
            })
    }

    pub(super) fn packet_format(record: &MetalPassRecord) -> crate::texture::ImageFormat {
        record
            .color_attachments
            .first()
            .map(|attachment| attachment.format)
            .unwrap_or(crate::texture::ImageFormat::Auto)
    }

    pub(super) fn packet_image_view(
        &self,
        graph: &FrameGraph<Self>,
        binding: &crate::renderer::frame_bindings::ImageBinding,
        slot: usize,
    ) -> Result<super::texture::MetalTextureView, RendererError> {
        use crate::backend::resource::{GpuImage, GpuImageView};
        use crate::render_graph::ImageAspects;
        use objc2_metal::MTLTexture;
        let view = graph
            .imported_images
            .get(&binding.resource)
            .and_then(|handle| self.textures.get(*handle).map(|entry| entry._view.clone()))
            .or_else(|| {
                graph
                    .transient_texture_by_id(binding.resource, slot)
                    .map(|texture| texture.view.clone())
            })
            .ok_or_else(|| {
                RendererError::InvalidOperation("Unresolved graphics packet image".into())
            })?;
        let range = binding.range;
        let allowed = match view.image().format() {
            crate::texture::ImageFormat::D32Sfloat => ImageAspects::DEPTH,
            crate::texture::ImageFormat::D32SfloatS8Uint
            | crate::texture::ImageFormat::D24UnormS8Uint => ImageAspects::DEPTH_STENCIL,
            _ => ImageAspects::COLOR,
        };
        if range.is_empty()
            || !allowed.contains(range.aspects)
            || range.aspects == ImageAspects::STENCIL
        {
            return Err(RendererError::InvalidOperation(
                "Graphics image aspect is unsupported for its native format".into(),
            ));
        }
        let mips = view.inner.mipmapLevelCount() as u32;
        let layers = view.inner.arrayLength() as u32;
        if range.base_mip_level >= mips || range.base_array_layer >= layers {
            return Err(RendererError::InvalidOperation(
                "Graphics image subresource starts outside its native allocation".into(),
            ));
        }
        let count = range.mip_level_count.min(mips - range.base_mip_level);
        let layer_count = range.array_layer_count.min(layers - range.base_array_layer);
        if (range.mip_level_count != u32::MAX && count != range.mip_level_count)
            || (range.array_layer_count != u32::MAX && layer_count != range.array_layer_count)
        {
            return Err(RendererError::InvalidOperation(
                "Graphics image subresource exceeds its native allocation".into(),
            ));
        }
        if range.base_mip_level == 0
            && count == mips
            && range.base_array_layer == 0
            && layer_count == layers
        {
            return Ok(view);
        }
        let inner = unsafe {
            view.inner
                .newTextureViewWithPixelFormat_textureType_levels_slices(
                    view.inner.pixelFormat(),
                    view.inner.textureType(),
                    objc2_foundation::NSRange::new(range.base_mip_level as usize, count as usize),
                    objc2_foundation::NSRange::new(
                        range.base_array_layer as usize,
                        layer_count as usize,
                    ),
                )
        }
        .ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal cannot create the requested sampled image subresource view".into(),
            )
        })?;
        Ok(super::texture::MetalTextureView::new(
            inner,
            view.image().clone(),
        ))
    }

    pub(super) fn encode_graphics_packet(
        &self,
        encoder: &mut MetalRenderEncoder,
        record: &MetalPassRecord,
        data: &PassExecutionData,
        frame: GraphicsFrame<'_>,
    ) -> Result<(), RendererError> {
        let GraphicsFrame { graph, slot, size } = frame;
        let crate::size::Size2D { width, height } = size;
        for phase in phases(record) {
            if let Some(viewport) = phase.viewport {
                encoder.set_viewport(
                    viewport.min[0],
                    viewport.min[1],
                    viewport.width(),
                    viewport.height(),
                    0.,
                    1.,
                );
                encoder.set_scissor(
                    viewport.min[0].max(0.) as u32,
                    viewport.min[1].max(0.) as u32,
                    viewport.width().max(0.) as u32,
                    viewport.height().max(0.) as u32,
                );
            } else {
                encoder.set_viewport(0., 0., width as f32, height as f32, 0., 1.);
                encoder.set_scissor(0, 0, width, height);
            }
            let pipelines = if phase.pipelines.is_empty() {
                &record.bindings.pipelines
            } else {
                &phase.pipelines
            };
            match &phase.draw {
                PassDraw::Submissions | PassDraw::ObjectIndices(_) => {
                    for draw in data.prepared().iter() {
                        if let PassDraw::ObjectIndices(indices) = &phase.draw
                            && !indices.iter().any(|index| {
                                *index >= draw.instance_index
                                    && *index < draw.instance_index + draw.instance_count().max(1)
                            })
                        {
                            continue;
                        }
                        let material = self.packet_material(pipelines, draw)?;
                        let pipeline =
                            self.material_pipeline(material, Self::packet_format(record))?;
                        encoder.bind_graphics_pipeline(&pipeline);
                        self.bind_packet(
                            encoder,
                            &pipeline,
                            &record.bindings,
                            Some(&phase),
                            frame,
                            Some(draw),
                        )?;
                        let mesh = self.meshes.get(draw.mesh).ok_or_else(|| {
                            RendererError::StaleHandle {
                                resource: "mesh".into(),
                                detail: record.name.clone(),
                            }
                        })?;
                        if mesh.index_count == 0 {
                            continue;
                        }
                        encoder.bind_vertex_buffer(&mesh.vertex_buffer, 0, 10);
                        encoder.bind_index_buffer(&mesh.index_buffer, 0, IndexType::Uint32);
                        if let PassDraw::ObjectIndices(indices) = &phase.draw {
                            let mut selected = indices
                                .iter()
                                .copied()
                                .filter(|index| {
                                    *index >= draw.instance_index
                                        && *index
                                            < draw.instance_index + draw.instance_count().max(1)
                                })
                                .collect::<Vec<_>>();
                            selected.sort_unstable();
                            selected.dedup();
                            for index in selected {
                                encoder.draw_indexed(mesh.index_count, 1, 0, 0, index);
                            }
                        } else {
                            encoder.draw_indexed(
                                mesh.index_count,
                                draw.instance_count().max(1),
                                0,
                                0,
                                draw.instance_index,
                            );
                        }
                    }
                }
                PassDraw::Vertices { .. } | PassDraw::Indirect { .. } => {
                    let material = pipelines
                        .first()
                        .map(|pipeline| pipeline.material)
                        .or(record.material)
                        .ok_or_else(|| {
                            RendererError::InvalidOperation(
                                "Generated geometry requires an explicit pipeline".into(),
                            )
                        })?;
                    let pipeline = self.material_pipeline(material, Self::packet_format(record))?;
                    encoder.bind_graphics_pipeline(&pipeline);
                    self.bind_packet(
                        encoder,
                        &pipeline,
                        &record.bindings,
                        Some(&phase),
                        frame,
                        None,
                    )?;
                    match phase.draw {
                        PassDraw::Vertices { count, instances } => {
                            encoder.draw(count, instances, 0, 0)
                        }
                        PassDraw::Indirect { resource, offset } => {
                            let buffer =
                                graph.buffer_by_id(self, resource, slot).ok_or_else(|| {
                                    RendererError::InvalidOperation(
                                        "Unresolved indirect drawing buffer".into(),
                                    )
                                })?;
                            encoder.observe_graph_resource(resource.0);
                            encoder.draw_indirect(&buffer.buffer, buffer.offset + offset);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn bind_packet(
        &self,
        encoder: &mut MetalRenderEncoder,
        pipeline: &MetalGraphicsPipeline,
        packet: &PassBindings,
        phase: Option<&PassDrawPhase>,
        frame: GraphicsFrame<'_>,
        draw: Option<&crate::renderer::types::DrawCall>,
    ) -> Result<(), RendererError> {
        let phase_constants = phase.map_or(&[][..], |phase| phase.constants.as_slice());
        let phase_samplers = phase.map_or(&[][..], |phase| phase.samplers.as_slice());
        let GraphicsFrame { graph, slot, .. } = frame;
        if let Some(objects) = self.current_object_storage_buffer() {
            for (index, stages) in table_slots(
                pipeline,
                0,
                1,
                TableBindingKind::Buffer,
                ShaderStages::VERTEX_FRAGMENT,
            ) {
                encoder.bind_storage_buffer(objects, 0, index, stages);
            }
        }
        if let Some(snapshot) = self.bindless_manager.snapshot() {
            encoder.bind_bindless(snapshot);
        }
        if let Some(sampler) = &self.shared_sampler {
            for (index, stages) in table_slots(
                pipeline,
                1,
                1,
                TableBindingKind::Sampler,
                ShaderStages::VERTEX_FRAGMENT,
            ) {
                encoder.bind_native_sampler(&sampler.inner, index, stages);
            }
        }
        if let Some(draw) = draw.filter(|draw| !draw.skeleton.is_none()) {
            let buffer = self.skeletons[slot].get(draw.skeleton).ok_or_else(|| {
                RendererError::StaleHandle {
                    resource: "skeleton".into(),
                    detail: "graphics packet".into(),
                }
            })?;
            for group in [2, 3] {
                if packet
                    .buffers
                    .iter()
                    .any(|binding| binding.group == group && binding.binding == 0)
                    || packet
                        .constants
                        .iter()
                        .chain(phase_constants)
                        .any(|binding| binding.group == group && binding.binding == 0)
                {
                    continue;
                }
                for (index, stages) in table_slots(
                    pipeline,
                    group,
                    0,
                    TableBindingKind::Buffer,
                    ShaderStages::VERTEX_FRAGMENT,
                ) {
                    encoder.bind_storage_buffer(buffer, 0, index, stages);
                }
            }
        }
        for binding in &packet.buffers {
            let buffer = graph
                .buffer_by_id(self, binding.resource, slot)
                .ok_or_else(|| {
                    RendererError::InvalidOperation("Unresolved graphics packet buffer".into())
                })?;
            let available = buffer.desc.size.saturating_sub(binding.range.offset);
            if binding.range.offset >= buffer.desc.size
                || (binding.range.size != u64::MAX && binding.range.size > available)
            {
                return Err(RendererError::InvalidOperation(
                    "Graphics packet buffer range exceeds its allocation".into(),
                ));
            }
            for (index, stages) in table_slots(
                pipeline,
                binding.group,
                binding.binding,
                TableBindingKind::Buffer,
                binding.stages,
            ) {
                encoder.bind_storage_buffer_range_render(
                    &buffer.buffer,
                    buffer.offset + binding.range.offset,
                    binding.range.size.min(available),
                    index,
                    stages,
                );
                encoder.observe_graph_resource(binding.resource.0);
            }
        }
        for binding in &packet.images {
            let view = self.packet_image_view(graph, binding, slot)?;
            for (index, stages) in table_slots(
                pipeline,
                binding.group,
                binding.binding,
                TableBindingKind::Texture,
                binding.stages,
            ) {
                encoder.bind_native_texture(&view.inner, index, stages);
                encoder.observe_graph_resource(binding.resource.0);
            }
        }
        for binding in packet.samplers_for_phase(phase_samplers) {
            let sampler = self
                .packet_samplers
                .iter()
                .find(|(mode, _)| *mode == binding.sampling)
                .map(|(_, sampler)| sampler)
                .ok_or_else(|| {
                    RendererError::InvalidOperation(
                        "Graphics packet sampler was not prepared".into(),
                    )
                })?;
            for (index, stages) in table_slots(
                pipeline,
                binding.group,
                binding.binding,
                TableBindingKind::Sampler,
                binding.stages,
            ) {
                encoder.bind_native_sampler(&sampler.inner, index, stages);
            }
        }
        for binding in packet.constants_for_phase(phase_constants) {
            for (index, stages) in table_slots(
                pipeline,
                binding.group,
                binding.binding,
                TableBindingKind::Buffer,
                binding.stages,
            ) {
                encoder.set_push_constants(&binding.bytes, index, stages);
            }
        }
        Ok(())
    }
}
