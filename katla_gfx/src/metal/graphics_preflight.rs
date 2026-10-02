//! Validate resolved graphics resources before creating native encoders.

use super::argument_state::ArgumentState;
use super::buffer::MetalBuffer;
use super::encoding_resources::EncodingResources;
use super::execution_plan::MetalPassRecord;
use super::metal_renderer::{MetalRenderer, OBJECT_UNIFORM_SIZE};
use super::pipeline::MetalGraphicsPipeline;
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::render_graph::{FrameGraph, PassExecutionData, PassKind};
use objc2_metal::MTLBuffer;

pub(crate) struct GraphicsPreflight<'a> {
    resources: &'a EncodingResources,
    vertex: ArgumentState,
    fragment: ArgumentState,
}

impl<'a> GraphicsPreflight<'a> {
    pub(crate) fn new(resources: &'a EncodingResources) -> Self {
        Self {
            resources,
            vertex: ArgumentState::default(),
            fragment: ArgumentState::default(),
        }
    }

    pub(crate) fn buffer(
        &self,
        buffer: &MetalBuffer,
        offset: u64,
        bytes: u64,
        index: usize,
        vertex: bool,
        fragment: bool,
    ) -> Result<(), RendererError> {
        let available = buffer.size().min(buffer.inner.length() as u64);
        if bytes == 0 || offset.checked_add(bytes).is_none_or(|end| end > available) {
            return Err(RendererError::InvalidOperation(format!(
                "Graphics buffer slot {index} has invalid native view {offset}..+{bytes} of {} bytes",
                available
            )));
        }
        self.resources.residency.add_buffer(&buffer.inner)?;
        self.resources.residency.validate_buffer(&buffer.inner)?;
        self.inline(bytes, index, vertex, fragment);
        Ok(())
    }

    pub(crate) fn full_buffer(
        &self,
        buffer: &MetalBuffer,
        index: usize,
        vertex: bool,
        fragment: bool,
    ) -> Result<(), RendererError> {
        self.buffer(buffer, 0, buffer.size(), index, vertex, fragment)
    }

    pub(crate) fn inline(&self, bytes: u64, index: usize, vertex: bool, fragment: bool) {
        if vertex {
            self.vertex.buffer(index, bytes);
        }
        if fragment {
            self.fragment.buffer(index, bytes);
        }
    }

    pub(crate) fn sampler(&self, index: usize, vertex: bool, fragment: bool) {
        if vertex {
            self.vertex.sampler(index);
        }
        if fragment {
            self.fragment.sampler(index);
        }
    }

    fn bindless(&self, renderer: &MetalRenderer) -> Result<(), RendererError> {
        if let Some(snapshot) = renderer.bindless_manager.snapshot() {
            snapshot.residency.validate_buffer(&snapshot.buffer)?;
            self.inline(snapshot.buffer.length() as u64, 9, true, true);
            self.resources.retain_bindless(snapshot);
        }
        Ok(())
    }

    pub(crate) fn pipeline(&self, pipeline: &MetalGraphicsPipeline) -> Result<(), RendererError> {
        self.vertex
            .validate(&pipeline.vertex_layout)
            .map_err(RendererError::InvalidOperation)?;
        if let Some(layout) = &pipeline.fragment_layout {
            self.fragment
                .validate(layout)
                .map_err(RendererError::InvalidOperation)?;
        }
        self.resources.retain_graphics_pipeline(pipeline);
        Ok(())
    }

    fn common(&self, renderer: &MetalRenderer) -> Result<(), RendererError> {
        if let (Some(frame), Some(objects)) = (
            renderer.current_frame_uniform_buffer(),
            renderer.current_object_storage_buffer(),
        ) {
            self.full_buffer(frame, 0, true, true)?;
            self.full_buffer(objects, 1, true, true)?;
        }
        self.bindless(renderer)?;
        if renderer.shared_sampler.is_some() {
            self.sampler(0, true, true);
        }
        if let Some(lights) = &renderer.light_culling {
            self.full_buffer(lights.light_buffer(), 3, false, true)?;
            self.full_buffer(lights.tile_index_buffer(), 4, false, true)?;
            self.full_buffer(lights.tile_count_buffer(), 5, false, true)?;
        }
        if let Some(shadow) = &renderer.shadow_cascade_buffers[renderer.frame_index()] {
            self.full_buffer(shadow, 7, false, true)?;
        }
        if renderer.shadow_sampler.is_some() {
            self.sampler(1, false, true);
        }
        Ok(())
    }
}

impl MetalRenderer {
    pub(crate) fn preflight_graphics_record(
        &self,
        resources: &EncodingResources,
        record: &MetalPassRecord,
        data: &PassExecutionData,
        graph: &FrameGraph<Self>,
        slot: usize,
    ) -> Result<(), RendererError> {
        let plan = GraphicsPreflight::new(resources);
        match record.kind {
            PassKind::Fullscreen => {
                plan.bindless(self)?;
                if let Some(buffer) = &self.dummy_vertex_buffer {
                    plan.full_buffer(buffer, 10, true, false)?;
                }
                if self.shared_sampler.is_some() {
                    plan.sampler(0, false, true);
                }
                plan.inline(
                    std::mem::size_of::<crate::renderer::types::FrameUniforms>() as u64,
                    0,
                    true,
                    true,
                );
                plan.pipeline(self.tonemap_pipeline.as_ref().ok_or_else(|| {
                    RendererError::InvalidOperation("Tonemap pipeline missing".into())
                })?)?;
            }
            PassKind::Particles => {
                if let Some(system) = &self.particle_system
                    && let Some(pipeline) = system.render_pipeline()
                {
                    use crate::render_graph::BuiltinBuffer::*;
                    for (role, index) in [
                        (ParticleData, 0),
                        (ParticleDeadList, 1),
                        (ParticleAliveWrite, 2),
                        (ParticleAliveWrite, 3),
                        (ParticleCounters, 4),
                    ] {
                        let (buffer, offset, bytes) = system
                            .buffer_slice(role, self.frame_index)
                            .ok_or_else(|| {
                            RendererError::InvalidOperation("Particle render buffer missing".into())
                        })?;
                        plan.buffer(buffer, offset, bytes, index, true, false)?;
                    }
                    let frame = self.current_frame_uniform_buffer().ok_or_else(|| {
                        RendererError::InvalidOperation("Particle frame uniforms missing".into())
                    })?;
                    plan.full_buffer(frame, 5, true, false)?;
                    let indirect = system
                        .builtin_buffer(ParticleIndirect, self.frame_index)
                        .ok_or_else(|| {
                            RendererError::InvalidOperation(
                                "Particle indirect buffer missing".into(),
                            )
                        })?;
                    plan.buffer(indirect, 0, 16, 30, false, false)?;
                    plan.pipeline(pipeline)?;
                }
            }
            PassKind::Ui => {
                if let Some(list) = data.ui_draw_lists.first().filter(|list| !list.is_empty()) {
                    plan.bindless(self)?;
                    if self.shared_sampler.is_some() {
                        plan.sampler(0, false, true);
                    }
                    let material = record.material.ok_or_else(|| {
                        RendererError::InvalidOperation("UI pass has no material".into())
                    })?;
                    let pipeline =
                        self.material_pipeline(material, record.color_attachments[0].format)?;
                    self.ui_renderers[slot].preflight_commands(&plan, list, &pipeline)?;
                }
            }
            _ => {
                if record.kind == PassKind::Geometry {
                    plan.common(self)?;
                    for access in &record.image_accesses {
                        if access.usage == crate::render_graph::ResourceAccessUsage::Sampled
                            && let Some(texture) =
                                graph.transient_texture_by_id(access.resource, slot)
                            && texture.format == crate::texture::ImageFormat::D32Sfloat
                        {
                            resources.residency.add_texture(&texture.view.inner)?;
                            resources.residency.validate_texture(&texture.view.inner)?;
                            plan.fragment.texture(1);
                        }
                    }
                    if record.color_attachments[0].load_op == crate::render_pass::LoadOp::Clear
                        && let Some(pipeline) = &self.sky_pipeline
                    {
                        if let Some(buffer) = &self.dummy_vertex_buffer {
                            plan.full_buffer(buffer, 10, true, false)?;
                        }
                        plan.pipeline(pipeline)?;
                    }
                }
                for draw in data.prepared().iter() {
                    if record.kind == PassKind::Shadow && draw.is_billboard {
                        continue;
                    }
                    let mesh =
                        self.meshes
                            .get(draw.mesh)
                            .ok_or_else(|| RendererError::StaleHandle {
                                resource: "mesh".into(),
                                detail: format!("graphics pass '{}'", record.name),
                            })?;
                    if mesh.index_count == 0 {
                        continue;
                    }
                    let material = self.materials.get(draw.material).ok_or_else(|| {
                        RendererError::StaleHandle {
                            resource: "material".into(),
                            detail: format!("graphics pass '{}'", record.name),
                        }
                    })?;
                    let skinned = !draw.skeleton.is_none();
                    let pipelines = match record.kind {
                        PassKind::Geometry => {
                            let key =
                                crate::renderer::pipeline_variant::PipelineVariantKey::resolve(
                                    &material.descriptor,
                                    record.color_attachments[0].format,
                                );
                            vec![material.variants.get(&key)]
                        }
                        PassKind::Shadow => vec![if skinned {
                            self.shadow
                                .pipeline_skinned()
                                .or_else(|| self.shadow.pipeline())
                        } else {
                            self.shadow.pipeline()
                        }],
                        PassKind::DepthPrepass => vec![if skinned {
                            self.depth_prepass
                                .pipeline_skinned()
                                .or_else(|| self.depth_prepass.pipeline())
                        } else if draw.is_billboard {
                            self.depth_prepass
                                .pipeline_billboard()
                                .or_else(|| self.depth_prepass.pipeline())
                        } else {
                            self.depth_prepass.pipeline()
                        }],
                        PassKind::ObjectId => vec![if skinned {
                            self.picking
                                .pipeline_skinned()
                                .or_else(|| self.picking.pipeline())
                        } else {
                            self.picking.pipeline()
                        }],
                        PassKind::Outline => vec![
                            if skinned {
                                self.outline
                                    .stencil_mark_skinned_pipeline()
                                    .or_else(|| self.outline.stencil_mark_pipeline())
                            } else {
                                self.outline.stencil_mark_pipeline()
                            },
                            if skinned {
                                self.outline
                                    .outline_draw_skinned_pipeline()
                                    .or_else(|| self.outline.outline_draw_pipeline())
                            } else {
                                self.outline.outline_draw_pipeline()
                            },
                        ],
                        _ => {
                            return Err(RendererError::InvalidOperation(format!(
                                "Unsupported graphics preflight {:?}",
                                record.kind
                            )));
                        }
                    };
                    let frame = self.current_frame_uniform_buffer().ok_or_else(|| {
                        RendererError::InvalidOperation("Frame uniforms missing".into())
                    })?;
                    let objects = self.current_object_storage_buffer().ok_or_else(|| {
                        RendererError::InvalidOperation("Object storage missing".into())
                    })?;
                    plan.full_buffer(frame, 0, true, true)?;
                    let object_offset = u64::from(draw.instance_index) * OBJECT_UNIFORM_SIZE;
                    let object_bytes =
                        u64::from(draw.instance_count().max(1)) * OBJECT_UNIFORM_SIZE;
                    plan.buffer(objects, object_offset, object_bytes, 1, true, true)?;
                    plan.buffer(
                        &mesh.vertex_buffer,
                        0,
                        u64::from(mesh.vertex_count) * u64::from(mesh.vertex_stride),
                        10,
                        true,
                        false,
                    )?;
                    plan.buffer(
                        &mesh.index_buffer,
                        0,
                        u64::from(mesh.index_count) * 4,
                        30,
                        false,
                        false,
                    )?;
                    if record.kind == PassKind::Shadow {
                        let cascade = self.shadow_cascade_encode_buffers[slot]
                            .as_ref()
                            .ok_or_else(|| {
                                RendererError::InvalidOperation(
                                    "Shadow cascade data missing".into(),
                                )
                            })?;
                        plan.full_buffer(cascade, 2, true, true)?;
                        plan.inline(16, 3, true, false);
                    }
                    if record.kind == PassKind::DepthPrepass && draw.is_billboard {
                        plan.bindless(self)?;
                        if self.shared_sampler.is_some() {
                            plan.sampler(0, false, true);
                        }
                    }
                    if skinned {
                        let skeleton =
                            self.skeletons[slot].get(draw.skeleton).ok_or_else(|| {
                                RendererError::StaleHandle {
                                    resource: "skeleton".into(),
                                    detail: format!("graphics pass '{}'", record.name),
                                }
                            })?;
                        plan.full_buffer(
                            skeleton,
                            if record.kind == PassKind::Shadow {
                                4
                            } else {
                                2
                            },
                            true,
                            true,
                        )?;
                    }
                    for (index, pipeline) in pipelines.into_iter().enumerate() {
                        let Some(pipeline) = pipeline else {
                            if record.kind == PassKind::Outline {
                                continue;
                            }
                            return Err(RendererError::InvalidOperation(format!(
                                "Graphics pipeline missing for '{}'",
                                record.name
                            )));
                        };
                        if record.kind == PassKind::Outline && index == 1 {
                            plan.inline(
                                std::mem::size_of::<super::outline::OutlinePushConstants>() as u64,
                                if skinned { 3 } else { 2 },
                                true,
                                true,
                            );
                        }
                        plan.pipeline(pipeline)?;
                    }
                }
            }
        }
        resources.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice, MTLResourceOptions};

    #[test]
    fn test_native_buffer_view_fails_before_any_argument_table_or_encoder_allocation() {
        let device = MTLCreateSystemDefaultDevice().unwrap();
        let resources = EncodingResources::new(&device, "graphics.preflight");
        let native = device
            .newBufferWithLength_options(64, MTLResourceOptions::StorageModeShared)
            .unwrap();
        let buffer = MetalBuffer::new(native, 128);
        let preflight = GraphicsPreflight::new(&resources);
        assert!(preflight.buffer(&buffer, 48, 32, 0, true, false).is_err());
        assert!(
            preflight
                .buffer(&buffer, u64::MAX, 4, 0, true, false)
                .is_err()
        );
        assert_eq!(resources.diagnostics().argument_table_count, 0);
        assert_eq!(resources.diagnostics().submission.allocation_count, 0);
        preflight.buffer(&buffer, 48, 16, 0, true, false).unwrap();
        assert_eq!(resources.diagnostics().submission.allocation_count, 1);
    }

    #[test]
    fn test_native_pipeline_missing_and_undersized_bindings_fail_before_encoding() {
        let context = super::super::context::MetalContext::init_headless().unwrap();
        let shader = super::super::shader::compile_wgsl_to_metal(
            &context.device,
            "@group(0) @binding(0) var<uniform> params: vec4f;
             @vertex fn vs_main() -> @builtin(position) vec4f { return params; }
             @fragment fn fs_main() -> @location(0) vec4f { return vec4f(1.0); }",
            &["vs_main", "fs_main"],
            super::super::binding_schema::ShaderProfile::Graphics,
        )
        .unwrap();
        let pipeline = context
            .create_graphics_pipeline(
                &shader.module.entry_points["vs_main"],
                Some(&shader.module.entry_points["fs_main"]),
                &[objc2_metal::MTLPixelFormat::BGRA8Unorm_sRGB],
                None,
                false,
                crate::pipeline::CompareOp::Always,
                objc2_metal::MTLCullMode::None,
                objc2_metal::MTLWinding::Clockwise,
            )
            .unwrap();
        let resources = EncodingResources::new(&context.device, "graphics.required_bindings");
        let plan = GraphicsPreflight::new(&resources);
        assert!(
            plan.pipeline(&pipeline)
                .unwrap_err()
                .to_string()
                .contains("missing")
        );
        let buffer = context.create_buffer(16, true).unwrap();
        plan.buffer(&buffer, 0, 15, 0, true, false).unwrap();
        assert!(
            plan.pipeline(&pipeline)
                .unwrap_err()
                .to_string()
                .contains("requires 16 bytes")
        );
        plan.buffer(&buffer, 0, 16, 0, true, false).unwrap();
        plan.pipeline(&pipeline).unwrap();
        assert_eq!(resources.diagnostics().argument_table_count, 0);
        resources.residency.commit();
        let late = context.create_buffer(16, true).unwrap();
        assert!(plan.buffer(&late, 0, 16, 0, true, false).is_err());
        assert_eq!(resources.diagnostics().argument_table_count, 0);
    }
}
