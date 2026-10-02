use super::{Frame, PassExecutionData};
use crate::render_graph::{PassDesc, RenderGraphError};
use crate::renderer::VulkanRenderer;
use crate::renderer::frame_bindings::{PassDraw, PassDrawPhase};
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
    pub(super) fn execute_graphics_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        let extent = self.color_target_extent(pass);
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };
        let color = self.resolve_color_attachments(pass)?;
        let (depth, stencil) = self.resolve_frame_depth_attachments(pass)?;
        if color.is_empty() && depth.is_none() {
            return Err(RenderGraphError::InvalidConfiguration(
                "Graphics pass has no declared attachments".into(),
            ));
        }
        let format = pass
            .output_format
            .unwrap_or(crate::texture::ImageFormat::Auto);
        let default_phase = PassDrawPhase {
            pipelines: pass.bindings.pipelines.clone(),
            constants: Vec::new(),
            viewport: None,
            draw: if data.prepared().is_empty()
                && pass
                    .material
                    .and_then(|material| self.renderer.asset_registry.get_material(material))
                    .is_some_and(|material| material.descriptor.vertex.is_empty())
                && data.ui_draw_lists.is_empty()
            {
                PassDraw::Vertices {
                    count: 3,
                    instances: 1,
                }
            } else {
                PassDraw::Submissions
            },
        };
        let phases = if pass.bindings.phases.is_empty() {
            std::slice::from_ref(&default_phase)
        } else {
            &pass.bindings.phases
        };
        for phase in phases {
            for pipeline in &phase.pipelines {
                self.renderer
                    .ensure_material_compiled(pipeline.material, format)?;
            }
        }
        if let Some(material) = pass.material {
            self.renderer.ensure_material_compiled(material, format)?;
        }
        if phases.iter().any(|phase| phase.pipelines.is_empty()) {
            self.ensure_materials_compiled(data.prepared(), format)?;
        }
        cmd.begin_rendering(&color, depth.as_ref(), stencil.as_ref(), render_area, 1);
        self.capture_render_encoder(pass, &color, depth.as_ref(), stencil.as_ref());
        cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
            extent.width,
            extent.height,
        )]);
        for phase in phases {
            let viewport = phase.viewport.unwrap_or_else(|| {
                crate::Rect::new([0.0, 0.0], [extent.width as f32, extent.height as f32])
            });
            cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
                viewport.min[0],
                viewport.min[1],
                viewport.width(),
                viewport.height(),
            )]);
            let mut packet = pass.bindings.clone();
            packet.phases.clear();
            for constant in &phase.constants {
                packet.constants.retain(|existing| {
                    (existing.group, existing.binding) != (constant.group, constant.binding)
                });
                packet.constants.push(constant.clone());
            }
            match &phase.draw {
                PassDraw::Submissions | PassDraw::ObjectIndices(_) => self.execute_draw_list(
                    cmd,
                    data.prepared(),
                    format,
                    &packet,
                    phase,
                    &pass.buffer_accesses,
                )?,
                PassDraw::Vertices { .. } | PassDraw::Indirect { .. } => {
                    let material = phase
                        .pipelines
                        .first()
                        .map(|pipeline| pipeline.material)
                        .or(pass.material)
                        .ok_or_else(|| {
                            RenderGraphError::PipelineNotSet(
                                "Generated geometry requires a material".into(),
                            )
                        })?;
                    let variant = self
                        .renderer
                        .material_variant(material, format)?
                        .ok_or_else(|| {
                            RenderGraphError::PipelineNotSet(
                                "Generated geometry variant unavailable".into(),
                            )
                        })?;
                    let (pipeline, _) = self
                        .renderer
                        .asset_registry
                        .get_pipeline_handles(variant.pipeline)?;
                    unsafe {
                        self.renderer.context.device.cmd_bind_pipeline(
                            cmd.vk_command_buffer(),
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline,
                        );
                    }
                    self.bind_graphics_resources(
                        cmd,
                        material,
                        variant.pipeline,
                        &packet,
                        crate::SkeletonHandle::NONE,
                        &pass.buffer_accesses,
                    )?;
                    match phase.draw {
                        PassDraw::Vertices { count, instances } => {
                            cmd.draw_array(count, instances, 0, 0)
                        }
                        PassDraw::Indirect { resource, offset } => {
                            let buffer = self
                                .graph
                                .buffer_by_id(self.renderer, resource, self.current_frame())
                                .ok_or_else(|| {
                                    RenderGraphError::ResourceNotFound(
                                        "Indirect draw buffer unavailable".into(),
                                    )
                                })?;
                            if offset.checked_add(16).is_none_or(|end| end > buffer.size()) {
                                return Err(RenderGraphError::InvalidConfiguration(
                                    "Indirect draw exceeds buffer".into(),
                                ));
                            }
                            unsafe {
                                self.renderer.context.device.cmd_draw_indirect(
                                    cmd.vk_command_buffer(),
                                    buffer.vk_buffer(),
                                    buffer.offset + offset,
                                    1,
                                    16,
                                );
                            }
                            self.capture_bound_resource(resource);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
        for list in &data.ui_draw_lists {
            self.execute_ui_draw_list(cmd, pass, list)?;
        }
        cmd.end_rendering();
        Ok(())
    }
}
