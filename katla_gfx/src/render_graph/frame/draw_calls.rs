use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::renderer::VulkanRenderer;
use crate::renderer::types::PreparedDraws;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
    /// Execute prepared draws with pipeline state caching.
    pub(super) fn execute_draw_list(
        &mut self,
        cmd: &CommandBuffer,
        draws: PreparedDraws<'_>,
        color_format: crate::texture::ImageFormat,
        packet: &crate::renderer::frame_bindings::PassBindings,
        phase: &crate::renderer::frame_bindings::PassDrawPhase,
        accesses: &[crate::render_graph::BufferAccess],
    ) -> Result<(), RenderGraphError> {
        if draws.is_empty() {
            return Ok(());
        }

        let pipelines = &phase.pipelines;
        let selection = match &phase.draw {
            crate::renderer::frame_bindings::PassDraw::ObjectIndices(indices) => {
                Some(indices.as_slice())
            }
            _ => None,
        };
        let mut current_pipeline = vk::Pipeline::null();

        for draw_call in draws.iter() {
            if selection.is_some_and(|indices| !indices.contains(&draw_call.instance_index)) {
                continue;
            }
            let mesh_layout = &self
                .renderer
                .asset_registry
                .get_mesh(draw_call.mesh)
                .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?
                .layout;
            let material = if pipelines.is_empty() {
                draw_call.material
            } else {
                pipelines
                    .iter()
                    .find(|pipeline| &pipeline.vertex_layout == mesh_layout)
                    .ok_or_else(|| {
                        RenderGraphError::InvalidConfiguration(
                            "No pass pipeline matches the submitted mesh layout".into(),
                        )
                    })?
                    .material
            };
            let variant = self
                .renderer
                .material_variant(material, color_format)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Material variant lookup failed: {}",
                        e
                    ))
                })?
                .ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Material {material:?} has no pipeline variant for {color_format:?}",
                        material = draw_call.material,
                    ))
                })?;

            let (pipeline, _) = self
                .renderer
                .asset_registry
                .get_pipeline_handles(variant.pipeline)?;
            if pipeline != current_pipeline {
                unsafe {
                    self.renderer.context.device.cmd_bind_pipeline(
                        cmd.vk_command_buffer(),
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline,
                    );
                }
                current_pipeline = pipeline;
            }
            self.bind_graphics_resources(
                cmd,
                material,
                variant.pipeline,
                packet,
                draw_call.skeleton,
                accesses,
            )?;
            let mesh = self
                .renderer
                .asset_registry
                .get_mesh(draw_call.mesh)
                .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?;

            // An empty dynamic mesh draws nothing: skip instead of encoding a
            // zero-count indexed draw without a bound index buffer.
            if mesh.index_count == 0 {
                continue;
            }
            let mut buffers = smallvec::SmallVec::<[(u32, vk::Buffer); 8]>::new();
            for (binding, attribute) in mesh.attributes.iter().enumerate() {
                let buffer = mesh.get_attribute_buffer(*attribute).ok_or_else(|| {
                    crate::render_graph::RenderGraphError::InvalidConfiguration(format!(
                        "Mesh vertex attribute {attribute:?} is missing"
                    ))
                })?;
                buffers.push((binding as u32, buffer.object()));
            }
            cmd.bind_vertex_buffers_at_locations(&buffers);

            if let Some(ib) = &mesh.index_buffer {
                cmd.bind_index_buffer(ib.object(), 0, mesh.index_format.into());
            }

            cmd.draw_indexed(
                mesh.index_count,
                draw_call.instance_count().max(1),
                0,
                0,
                draw_call.instance_index,
            );
        }

        Ok(())
    }

    /// Pre-compile the pipeline variant of every material referenced by
    /// prepared draws for the pass's color format.
    pub(super) fn ensure_materials_compiled(
        &mut self,
        draws: PreparedDraws<'_>,
        color_format: crate::texture::ImageFormat,
    ) -> Result<(), RenderGraphError> {
        let mut materials_to_compile: Vec<crate::handle::MaterialHandle> = Vec::new();

        for draw_call in draws.iter() {
            if self
                .renderer
                .material_variant(draw_call.material, color_format)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Material variant lookup failed: {}",
                        e
                    ))
                })?
                .is_none()
                && !materials_to_compile.contains(&draw_call.material)
            {
                materials_to_compile.push(draw_call.material);
            }
        }

        for handle in materials_to_compile {
            self.renderer
                .ensure_material_compiled(handle, color_format)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Pipeline variant compilation failed: {}",
                        e
                    ))
                })?;
        }

        Ok(())
    }

    /// Pre-compile pipeline variants for all materials from ALL pending
    /// draw lists before command buffer recording.
    pub(crate) fn pre_compile_materials(&mut self) -> Result<(), RenderGraphError> {
        use std::collections::HashSet;

        let mut materials_to_compile: Vec<(
            crate::handle::MaterialHandle,
            crate::texture::ImageFormat,
        )> = Vec::new();
        let mut seen = HashSet::new();

        for (&pass_index, data) in &self.pending {
            let Some(pass) = self.graph.pass(pass_index) else {
                continue;
            };
            if pass.kind != Some(crate::render_graph::pass::PassKind::Geometry) {
                continue;
            }
            let (_, format) = self.graphics_target_config(pass)?;
            for draw_list in &data.draw_lists {
                for draw_call in &draw_list.draws {
                    if seen.insert((draw_call.material, format)) {
                        materials_to_compile.push((draw_call.material, format));
                    }
                }
            }
        }

        for (handle, format) in materials_to_compile {
            log::debug!("pre_compile_materials: variant for material {handle:?} in {format:?}");
            self.renderer
                .ensure_material_compiled(handle, format)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Pipeline variant pre-compilation failed: {}",
                        e
                    ))
                })?;
        }

        Ok(())
    }
}
