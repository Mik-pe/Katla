//! Validate resolved graphics resources before creating native encoders.

use super::argument_state::ArgumentState;
use super::buffer::MetalBuffer;
use super::encoding_resources::EncodingResources;
use super::execution_plan::MetalPassRecord;
use super::metal_renderer::MetalRenderer;
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
        self.preflight_graphics_record_resources(resources, record, data, graph, slot)
            .map_err(|error| {
                preflight_context(error, || format!("Graphics pass '{}'", record.name))
            })
    }

    fn preflight_graphics_record_resources(
        &self,
        resources: &EncodingResources,
        record: &MetalPassRecord,
        data: &PassExecutionData,
        graph: &FrameGraph<Self>,
        slot: usize,
    ) -> Result<(), RendererError> {
        use super::binding_schema::TableBindingKind;
        use super::graphics_packet::{phases, table_slots};
        use crate::backend::command::ShaderStages;
        use crate::renderer::frame_bindings::PassDraw;
        use crate::renderer::graphics_interface::{GraphicsBindingKind, GraphicsBindingLayout};
        for phase in phases(record) {
            if let PassDraw::Indirect { resource, offset } = phase.draw {
                let buffer = graph.buffer_by_id(self, resource, slot).ok_or_else(|| {
                    RendererError::InvalidOperation("Indirect drawing buffer missing".into())
                })?;
                let available = buffer
                    .desc
                    .size
                    .min(buffer.buffer.size().saturating_sub(buffer.offset));
                if !offset.is_multiple_of(4)
                    || offset.checked_add(16).is_none_or(|end| end > available)
                {
                    return Err(RendererError::InvalidOperation(
                        "Indirect draw exceeds its native buffer allocation".into(),
                    ));
                }
                resources.residency.add_buffer(&buffer.buffer.inner)?;
            }
            let pipelines = if phase.pipelines.is_empty() {
                &record.bindings.pipelines
            } else {
                &phase.pipelines
            };
            let selected = match phase.draw {
                PassDraw::Submissions | PassDraw::ObjectIndices(_)
                    if record.kind == PassKind::Ui
                        && data.ui_draw_lists.iter().any(|list| !list.is_empty()) =>
                {
                    vec![(
                        record.material.ok_or_else(|| {
                            RendererError::InvalidOperation("UI material missing".into())
                        })?,
                        None,
                    )]
                }
                PassDraw::Submissions | PassDraw::ObjectIndices(_) => data
                    .prepared()
                    .iter()
                    .filter(|draw| match &phase.draw {
                        PassDraw::ObjectIndices(indices) => indices.iter().any(|index| {
                            *index >= draw.instance_index
                                && *index
                                    < draw
                                        .instance_index
                                        .saturating_add(draw.instance_count().max(1))
                        }),
                        _ => true,
                    })
                    .map(|draw| {
                        self.packet_material(pipelines, draw)
                            .map(|material| (material, Some(draw)))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => vec![(
                    pipelines
                        .first()
                        .map(|pipeline| pipeline.material)
                        .or(record.material)
                        .ok_or_else(|| {
                            RendererError::InvalidOperation(
                                "Generated geometry requires a pipeline".into(),
                            )
                        })?,
                    None,
                )],
            };
            for (material_handle, draw) in selected {
                let material = self.materials.get(material_handle).ok_or_else(|| {
                    RendererError::InvalidOperation(format!(
                        "Graphics material {material_handle:?} missing"
                    ))
                })?;
                (|| -> Result<(), RendererError> {
                    let pipeline =
                        self.material_pipeline(material_handle, Self::packet_format(record))?;
                    if material.interface.color_attachment_count() != record.color_attachments.len()
                        || material.descriptor.color_attachment
                            != !record.color_attachments.is_empty()
                        || material.descriptor.depth_format
                            != record.depth_attachment.map(|attachment| attachment.format)
                    {
                        return Err(RendererError::InvalidOperation(format!(
                            "Graphics pipeline attachments do not match pass '{}'",
                            record.name
                        )));
                    }
                    let plan = GraphicsPreflight::new(resources);
                    let mut provided = Vec::new();
                    if record.kind == PassKind::Ui {
                        for (index, stages) in table_slots(
                            &pipeline,
                            0,
                            3,
                            TableBindingKind::Buffer,
                            ShaderStages::VERTEX_FRAGMENT,
                        ) {
                            plan.inline(16, index as usize, stages.vertex, stages.fragment);
                        }
                        provided.push(GraphicsBindingLayout {
                            group: 0,
                            binding: 3,
                            stages: ShaderStages::VERTEX_FRAGMENT,
                            kind: GraphicsBindingKind::Buffer {
                                usage: crate::render_graph::BufferUsage::Uniform,
                                mode: crate::render_graph::ResourceAccessMode::Read,
                                minimum_bytes: 16,
                            },
                            array: false,
                        });
                    }

                    if let Some(objects) = self.current_object_storage_buffer() {
                        for (index, stages) in table_slots(
                            &pipeline,
                            0,
                            1,
                            TableBindingKind::Buffer,
                            ShaderStages::VERTEX_FRAGMENT,
                        ) {
                            plan.full_buffer(
                                objects,
                                index as usize,
                                stages.vertex,
                                stages.fragment,
                            )?;
                        }
                        provided.push(GraphicsBindingLayout {
                            group: 0,
                            binding: 1,
                            stages: ShaderStages::VERTEX_FRAGMENT,
                            kind: GraphicsBindingKind::Buffer {
                                usage: crate::render_graph::BufferUsage::Storage,
                                mode: crate::render_graph::ResourceAccessMode::Read,
                                minimum_bytes: objects.size(),
                            },
                            array: false,
                        });
                    }
                    if self.bindless_manager.snapshot().is_some() {
                        plan.bindless(self)?;
                        provided.push(GraphicsBindingLayout {
                            group: 1,
                            binding: 0,
                            stages: ShaderStages::VERTEX_FRAGMENT,
                            kind: GraphicsBindingKind::Image { storage: false },
                            array: true,
                        });
                    }
                    if self.shared_sampler.is_some() {
                        for (index, stages) in table_slots(
                            &pipeline,
                            1,
                            1,
                            TableBindingKind::Sampler,
                            ShaderStages::VERTEX_FRAGMENT,
                        ) {
                            plan.sampler(index as usize, stages.vertex, stages.fragment);
                        }
                        provided.push(GraphicsBindingLayout {
                            group: 1,
                            binding: 1,
                            stages: ShaderStages::VERTEX_FRAGMENT,
                            kind: GraphicsBindingKind::Sampler { comparison: false },
                            array: false,
                        });
                    }
                    if let Some(draw) = draw {
                        let mesh = self.meshes.get(draw.mesh).ok_or_else(|| {
                            RendererError::InvalidOperation("Submitted mesh missing".into())
                        })?;
                        if mesh.index_count == 0 {
                            return Ok(());
                        }
                        plan.full_buffer(&mesh.vertex_buffer, 10, true, false)?;
                        plan.full_buffer(&mesh.index_buffer, 30, false, false)?;
                        if !draw.skeleton.is_none() {
                            let skeleton =
                                self.skeletons[slot].get(draw.skeleton).ok_or_else(|| {
                                    RendererError::InvalidOperation(
                                        "Submitted skeleton missing".into(),
                                    )
                                })?;
                            for group in [2, 3] {
                                if record
                                    .bindings
                                    .buffers
                                    .iter()
                                    .any(|binding| binding.group == group && binding.binding == 0)
                                    || record
                                        .bindings
                                        .constants
                                        .iter()
                                        .chain(&phase.constants)
                                        .any(|binding| {
                                            binding.group == group && binding.binding == 0
                                        })
                                {
                                    continue;
                                }
                                for (index, stages) in table_slots(
                                    &pipeline,
                                    group,
                                    0,
                                    TableBindingKind::Buffer,
                                    ShaderStages::VERTEX_FRAGMENT,
                                ) {
                                    plan.full_buffer(
                                        skeleton,
                                        index as usize,
                                        stages.vertex,
                                        stages.fragment,
                                    )?;
                                    if let Some(existing) = provided.iter_mut().find(|binding| {
                                        binding.group == group && binding.binding == 0
                                    }) {
                                        existing.stages.vertex |= stages.vertex;
                                        existing.stages.fragment |= stages.fragment;
                                    } else {
                                        provided.push(GraphicsBindingLayout {
                                            group,
                                            binding: 0,
                                            stages,
                                            kind: GraphicsBindingKind::Buffer {
                                                usage: crate::render_graph::BufferUsage::Storage,
                                                mode: crate::render_graph::ResourceAccessMode::Read,
                                                minimum_bytes: skeleton.size(),
                                            },
                                            array: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    let mut packet = record.bindings.clone();
                    packet.phases.clear();
                    for binding in &phase.constants {
                        packet.constants.retain(|base| {
                            base.group != binding.group || base.binding != binding.binding
                        });
                        packet.constants.push(binding.clone());
                    }
                    provided.retain(|implicit| {
                        !packet.buffers.iter().any(|binding| {
                            binding.group == implicit.group && binding.binding == implicit.binding
                        }) && !packet.images.iter().any(|binding| {
                            binding.group == implicit.group && binding.binding == implicit.binding
                        }) && !packet.samplers.iter().any(|binding| {
                            binding.group == implicit.group && binding.binding == implicit.binding
                        }) && !packet.constants.iter().any(|binding| {
                            binding.group == implicit.group && binding.binding == implicit.binding
                        })
                    });
                    material
                        .interface
                        .validate_buffer_accesses(&packet, &record.buffer_accesses)
                        .map_err(RendererError::InvalidOperation)?;
                    material
                        .interface
                        .validate_bindings(&packet, &provided, |resource| {
                            graph
                                .buffer_by_id(self, resource, slot)
                                .map(|buffer| buffer.desc.size)
                        })
                        .map_err(RendererError::InvalidOperation)?;
                    self.preflight_packet_bindings(
                        &plan, resources, &pipeline, &packet, graph, slot,
                    )?;
                    plan.pipeline(&pipeline)?;
                    Ok(())
                })()
                .map_err(|error| {
                    preflight_context(error, || {
                        let buffers = record
                            .bindings
                            .buffers
                            .iter()
                            .map(|binding| {
                                format!(
                                    "{}:{}=resource {}",
                                    binding.group, binding.binding, binding.resource.0
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!(
                            "material {material_handle:?} shader '{}' [buffers: {buffers}]",
                            material.descriptor.shader_path
                        )
                    })
                })?;
            }
        }
        if record.kind == PassKind::Ui
            && let Some(list) = data.ui_draw_lists.first().filter(|list| !list.is_empty())
        {
            let material = record
                .material
                .ok_or_else(|| RendererError::InvalidOperation("UI material missing".into()))?;
            (|| -> Result<(), RendererError> {
                let plan = GraphicsPreflight::new(resources);
                plan.bindless(self)?;
                let pipeline = self.material_pipeline(material, Self::packet_format(record))?;
                for (index, stages) in table_slots(
                    &pipeline,
                    1,
                    1,
                    TableBindingKind::Sampler,
                    ShaderStages::FRAGMENT,
                ) {
                    plan.sampler(index as usize, stages.vertex, stages.fragment);
                }
                self.preflight_packet_bindings(
                    &plan,
                    resources,
                    &pipeline,
                    &record.bindings,
                    graph,
                    slot,
                )?;
                self.ui_renderers[slot].preflight_commands(&plan, list, &pipeline)?;
                Ok(())
            })()
            .map_err(|error| {
                preflight_context(error, || {
                    let shader = self
                        .materials
                        .get(material)
                        .map(|material| material.descriptor.shader_path.as_str())
                        .unwrap_or("<missing>");
                    format!("material {material:?} shader '{shader}'")
                })
            })?;
        }
        resources.check()
    }
    fn preflight_packet_bindings(
        &self,
        plan: &GraphicsPreflight<'_>,
        resources: &EncodingResources,
        pipeline: &MetalGraphicsPipeline,
        packet: &crate::renderer::frame_bindings::PassBindings,
        graph: &FrameGraph<Self>,
        slot: usize,
    ) -> Result<(), RendererError> {
        use super::binding_schema::TableBindingKind;
        use super::graphics_packet::table_slots;
        for binding in &packet.buffers {
            let buffer = graph
                .buffer_by_id(self, binding.resource, slot)
                .ok_or_else(|| RendererError::InvalidOperation("Graphics buffer missing".into()))?;
            let bytes = binding
                .range
                .size
                .min(buffer.desc.size.saturating_sub(binding.range.offset));
            for (index, stages) in table_slots(
                pipeline,
                binding.group,
                binding.binding,
                TableBindingKind::Buffer,
                binding.stages,
            ) {
                plan.buffer(
                    &buffer.buffer,
                    buffer.offset + binding.range.offset,
                    bytes,
                    index as usize,
                    stages.vertex,
                    stages.fragment,
                )?;
            }
        }
        for binding in &packet.images {
            let view = self.packet_image_view(graph, binding, slot)?;
            resources.residency.add_texture(&view.inner)?;
            for (index, stages) in table_slots(
                pipeline,
                binding.group,
                binding.binding,
                TableBindingKind::Texture,
                binding.stages,
            ) {
                if stages.vertex {
                    plan.vertex.texture(index as usize);
                }
                if stages.fragment {
                    plan.fragment.texture(index as usize);
                }
            }
        }
        for binding in &packet.samplers {
            for (index, stages) in table_slots(
                pipeline,
                binding.group,
                binding.binding,
                TableBindingKind::Sampler,
                binding.stages,
            ) {
                plan.sampler(index as usize, stages.vertex, stages.fragment);
            }
        }
        for binding in &packet.constants {
            for (index, stages) in table_slots(
                pipeline,
                binding.group,
                binding.binding,
                TableBindingKind::Buffer,
                binding.stages,
            ) {
                plan.inline(
                    binding.bytes.len() as u64,
                    index as usize,
                    stages.vertex,
                    stages.fragment,
                );
            }
        }
        Ok(())
    }
}

fn preflight_context(error: RendererError, context: impl FnOnce() -> String) -> RendererError {
    match error {
        RendererError::InvalidOperation(reason) => {
            RendererError::InvalidOperation(format!("{}: {reason}", context()))
        }
        other => other,
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
            .create_graphics_pipeline(crate::metal::context::GraphicsPipelineConfig {
                vertex_function: &shader.module.entry_points["vs_main"],
                fragment_function: Some(&shader.module.entry_points["fs_main"]),
                color_formats: &[objc2_metal::MTLPixelFormat::BGRA8Unorm_sRGB],
                depth_format: None,
                depth_write_enabled: false,
                depth_compare: crate::pipeline::CompareOp::Always,
                cull_mode: objc2_metal::MTLCullMode::None,
                front_face: objc2_metal::MTLWinding::Clockwise,
                vertex_descriptor: &objc2_metal::MTLVertexDescriptor::new(),
                alpha_blended: false,
                portable: None,
            })
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
