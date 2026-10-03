//! Native UI encoding with immutable per-pass geometry and bindings.

use crate::handle::PipelineHandle;
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::pass::PassDesc;
use crate::renderer::VulkanRenderer;
use crate::renderer::types::UIDrawList;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
    /// Execute a UI draw list.
    pub(super) fn execute_ui_draw_list(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        ui_draw_list: &UIDrawList,
    ) -> Result<(), RenderGraphError> {
        if ui_draw_list.is_empty() {
            return Ok(());
        }

        let sampling = pass
            .bindings
            .samplers
            .iter()
            .find(|binding| binding.group == 0 && binding.binding == 1 && binding.stages.fragment)
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(
                    "UI pass requires its explicit sampler at group 0 binding 1".into(),
                )
            })?
            .sampling;
        if sampling.comparison.is_some() {
            return Err(RenderGraphError::InvalidConfiguration(
                "UI sampler must not compare depth".into(),
            ));
        }
        let sampler = self.graphics_sampler(sampling)?;
        let material_handle = pass.material.ok_or(RenderGraphError::InvalidConfiguration(
            "UI pass has no material specified. Use .material() on UIPass.".to_string(),
        ))?;

        let (extent, format) = self.graphics_target_config(pass)?;
        self.renderer
            .ensure_material_compiled(material_handle, format)
            .map_err(|e| {
                RenderGraphError::InvalidConfiguration(format!(
                    "UI material variant compilation failed: {}",
                    e
                ))
            })?;

        // Resolve both pipelines: regular (for vertex-based commands) and
        // instanced (for instanced commands). The instanced pipeline uses
        // vs_instanced/fs_instanced entry points with UnitQuadVertex format.
        let variant = self
            .renderer
            .material_variant(material_handle, format)
            .map_err(|e| {
                RenderGraphError::InvalidConfiguration(format!(
                    "UI material variant lookup failed: {}",
                    e
                ))
            })?
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(format!(
                    "UI material {material_handle:?} has no pipeline variant for {format:?}"
                ))
            })?;
        let (regular_pipeline, pipeline_layout) = self
            .renderer
            .asset_registry
            .get_pipeline_handles(variant.pipeline)?;

        let instanced_pipeline_handle = variant.instanced_pipeline.ok_or_else(|| {
            RenderGraphError::InvalidConfiguration(format!(
                "UI material {material_handle:?} has no instanced pipeline variant"
            ))
        })?;
        let (instanced_pipeline, _) = self
            .renderer
            .asset_registry
            .get_pipeline_handles(instanced_pipeline_handle)?;

        let slot = self.current_frame();
        let resources = &mut self.renderer.frame_resources[slot];
        let instance_buffer = if ui_draw_list.instances.is_empty() {
            None
        } else {
            Some(resources.upload(bytemuck::cast_slice(&ui_draw_list.instances))?)
        };
        let geometry = if ui_draw_list.indices.is_empty() {
            None
        } else {
            Some((
                resources.upload(bytemuck::cast_slice(&ui_draw_list.vertices))?,
                resources.upload(bytemuck::cast_slice(&ui_draw_list.indices))?,
            ))
        };
        let unit_quad = if instance_buffer.is_some() {
            Some((
                resources.upload(bytemuck::cast_slice(&crate::vertex::UNIT_QUAD_VERTICES))?,
                resources.upload(bytemuck::cast_slice(&crate::vertex::UNIT_QUAD_INDICES))?,
            ))
        } else {
            None
        };
        self.bind_ui_descriptor_sets(
            cmd,
            variant.pipeline,
            pipeline_layout,
            ui_draw_list.screen_size,
            instance_buffer,
            sampler,
        )?;

        for draw in &ui_draw_list.commands {
            let scissor = if let Some([x, y, width, height]) = draw.clip_rect {
                let scale = ui_draw_list.scale_factor;
                crate::sync::Rect2D::new(
                    (x * scale).max(0.) as i32,
                    (y * scale).max(0.) as i32,
                    (width * scale).max(0.) as u32,
                    (height * scale).max(0.) as u32,
                )
            } else {
                crate::sync::Rect2D::from_extent(extent.width, extent.height)
            };
            cmd.set_scissor(&[scissor]);
            let (vertex, index) = if draw.is_instanced {
                unit_quad.as_ref()
            } else {
                geometry.as_ref()
            }
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration("UI draw has no matching geometry".into())
            })?;
            cmd.bind_vertex_buffer(vertex.buffer, vertex.offset);
            cmd.bind_index_buffer(index.buffer, index.offset, vk::IndexType::UINT32);
            let pipeline = if draw.is_instanced {
                instanced_pipeline
            } else {
                regular_pipeline
            };
            unsafe {
                self.renderer.context.device.cmd_bind_pipeline(
                    cmd.vk_command_buffer(),
                    vk::PipelineBindPoint::GRAPHICS,
                    pipeline,
                );
            }
            if draw.is_instanced {
                cmd.draw_indexed(6, draw.count, 0, 0, draw.offset);
            } else {
                cmd.draw_indexed(draw.count, 1, draw.offset, 0, 0);
            }
        }
        cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
            extent.width,
            extent.height,
        )]);
        Ok(())
    }

    fn bind_ui_descriptor_sets(
        &mut self,
        cmd: &CommandBuffer,
        pipeline: PipelineHandle,
        pipeline_layout: vk::PipelineLayout,
        screen_size: [f32; 2],
        instances: Option<vk::DescriptorBufferInfo>,
        sampler: vk::Sampler,
    ) -> Result<(), RenderGraphError> {
        let layout = self
            .renderer
            .asset_registry
            .get_pipeline(pipeline)
            .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline))?
            .descriptor_set_layouts()
            .first()
            .copied()
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(
                    "UI pipeline has no descriptor layout".into(),
                )
            })?;
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1),
        ];
        let slot = self.current_frame();
        let resources = &mut self.renderer.frame_resources[slot];
        let set = resources.descriptors.allocate(layout, &sizes)?;
        let uniforms = resources.upload(bytemuck::cast_slice(&[
            screen_size[0],
            screen_size[1],
            1.0,
            0.0,
        ]))?;
        let image = vk::DescriptorImageInfo::default().sampler(sampler);
        let mut writes: smallvec::SmallVec<[vk::WriteDescriptorSet<'_>; 3]> = smallvec::smallvec![
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(std::slice::from_ref(&image)),
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(std::slice::from_ref(&uniforms)),
        ];
        if let Some(info) = &instances {
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(4)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(std::slice::from_ref(info)),
            );
        }
        unsafe {
            self.renderer
                .context
                .device
                .update_descriptor_sets(&writes, &[]);
        }
        cmd.bind_descriptor_sets(pipeline_layout, 0, &[set], &[]);
        cmd.bind_descriptor_sets(
            pipeline_layout,
            1,
            &[self.renderer.bindless_manager.descriptor_set().vk()],
            &[],
        );
        Ok(())
    }
}
