use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::renderer::types::DrawList;
use crate::vulkan::commandbuffer::CommandBuffer;
use crate::vulkan::vertex_attribute::AttributeType;
use ash::vk;

impl<'a> Frame<'a> {
    /// Execute a draw list with pipeline state caching.
    ///
    /// Tracks the currently bound pipeline and skeleton descriptor to skip
    /// redundant Vulkan state changes when consecutive draw calls share the
    /// same material/pipeline or skeleton.
    pub(super) fn execute_draw_list(
        &mut self,
        cmd: &CommandBuffer,
        draw_list: &DrawList,
    ) -> Result<(), RenderGraphError> {
        if draw_list.draws.is_empty() {
            return Ok(());
        }

        self.ensure_materials_compiled(draw_list)?;

        let mut current_pipeline = vk::Pipeline::null();
        let mut current_layout = vk::PipelineLayout::null();
        let mut current_skeleton = vk::DescriptorSet::null();

        for draw_call in &draw_list.draws {
            let (pipeline, layout) = {
                let material = self
                    .renderer
                    .asset_registry
                    .get_material(draw_call.material)
                    .ok_or(RenderGraphError::InvalidMaterialHandle(draw_call.material))?;

                let pipeline_handle = material
                    .pipeline
                    .ok_or(RenderGraphError::InvalidMaterialHandle(draw_call.material))?;

                self.renderer
                    .asset_registry
                    .get_pipeline_handles(pipeline_handle)?
            };

            if pipeline != current_pipeline {
                unsafe {
                    self.renderer.context.device.cmd_bind_pipeline(
                        cmd.vk_command_buffer(),
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline,
                    );
                }
                let frame_idx = self.renderer.current_frame();

                // Bind Set 0: storage uniforms
                let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
                cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

                // Bind Set 1: bindless textures
                let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
                cmd.bind_descriptor_sets(layout, 1, &[bindless_ds], &[]);

                // Bind Set 2: skeleton (bound per-draw below) or empty placeholder
                let empty_ds = self.renderer.empty_descriptor_set(frame_idx);
                cmd.bind_descriptor_sets(layout, 2, &[empty_ds], &[]);

                // Bind Set 3: light culling (push descriptors)
                if let Some(lc) = self.renderer.light_culling_buffers()
                    && let Err(e) = lc.push_fragment_descriptors(cmd.vk_command_buffer(), layout)
                {
                    log::warn!("Failed to push light culling fragment descriptors: {}", e);
                }

                // Bind Set 4: shadow descriptors
                self.renderer
                    .bind_shadow_descriptors(cmd.vk_command_buffer(), layout);

                current_pipeline = pipeline;
                current_layout = layout;
                current_skeleton = vk::DescriptorSet::null();
            }

            let is_skinned = !draw_call.skeleton.is_none();
            let skel_ds = if is_skinned {
                self.renderer
                    .get_skeleton_descriptor(draw_call.skeleton)
                    .ok_or(RenderGraphError::InvalidSkeletonHandle(draw_call.skeleton))?
                    .vk_set()
            } else {
                vk::DescriptorSet::null()
            };

            if skel_ds != current_skeleton {
                if is_skinned {
                    cmd.bind_descriptor_sets(current_layout, 2, &[skel_ds], &[]);
                }
                current_skeleton = skel_ds;
            }

            let mesh = self
                .renderer
                .asset_registry
                .get_mesh(draw_call.mesh)
                .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?;

            let pos_buf = mesh
                .get_attribute_buffer(AttributeType::Position)
                .map(|vb| vb.object())
                .unwrap_or(vk::Buffer::null());
            let norm_buf = mesh
                .get_attribute_buffer(AttributeType::Normal)
                .map(|vb| vb.object())
                .unwrap_or(vk::Buffer::null());
            let tang_buf = mesh
                .get_attribute_buffer(AttributeType::Tangent)
                .map(|vb| vb.object())
                .unwrap_or(vk::Buffer::null());
            let uv_buf = mesh
                .get_attribute_buffer(AttributeType::TexCoord0)
                .map(|vb| vb.object())
                .unwrap_or(vk::Buffer::null());

            if is_skinned {
                let joints_buf = mesh
                    .get_attribute_buffer(AttributeType::JointIndices)
                    .map(|vb| vb.object())
                    .unwrap_or(vk::Buffer::null());
                let weights_buf = mesh
                    .get_attribute_buffer(AttributeType::JointWeights)
                    .map(|vb| vb.object())
                    .unwrap_or(vk::Buffer::null());
                cmd.bind_vertex_buffers_at_locations(&[
                    (0, pos_buf),
                    (1, norm_buf),
                    (2, tang_buf),
                    (3, uv_buf),
                    (4, joints_buf),
                    (5, weights_buf),
                ]);
            } else {
                cmd.bind_vertex_buffers_at_locations(&[
                    (0, pos_buf),
                    (1, norm_buf),
                    (2, tang_buf),
                    (3, uv_buf),
                    (4, vk::Buffer::null()),
                    (5, vk::Buffer::null()),
                ]);
            }

            if let Some(ib) = &mesh.index_buffer {
                cmd.bind_index_buffer(ib.object(), 0, vk::IndexType::UINT32);
            }

            let index_count = mesh.index_buffer.as_ref().map(|ib| ib.count()).unwrap_or(0);

            cmd.draw_indexed(index_count, 1, 0, 0, draw_call.instance_index);
        }

        Ok(())
    }

    /// Pre-compile all materials in a draw list.
    pub(super) fn ensure_materials_compiled(
        &mut self,
        draw_list: &DrawList,
    ) -> Result<(), RenderGraphError> {
        let mut materials_to_compile: Vec<(
            crate::handle::MaterialHandle,
            crate::texture::ImageFormat,
        )> = Vec::new();

        for draw_call in &draw_list.draws {
            if let Some(material) = self
                .renderer
                .asset_registry
                .get_material(draw_call.material)
                && !material.fully_compiled
            {
                materials_to_compile.push((draw_call.material, material.color_format));
            }
        }

        for (handle, format) in materials_to_compile {
            self.renderer
                .ensure_material_compiled(handle, format)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Material recompilation failed: {}",
                        e
                    ))
                })?;
        }

        Ok(())
    }

    /// Pre-compile all materials from ALL pending draw lists before command buffer recording.
    pub(crate) fn pre_compile_materials(&mut self) -> Result<(), RenderGraphError> {
        use std::collections::HashSet;

        let mut materials_to_compile: Vec<(
            crate::handle::MaterialHandle,
            crate::texture::ImageFormat,
        )> = Vec::new();
        let mut seen = HashSet::new();

        for data in self.pending.values() {
            for draw_list in &data.draw_lists {
                for draw_call in &draw_list.draws {
                    if seen.insert(draw_call.material)
                        && let Some(material) = self
                            .renderer
                            .asset_registry
                            .get_material(draw_call.material)
                        && !material.fully_compiled
                    {
                        materials_to_compile.push((draw_call.material, material.color_format));
                    }
                }
            }
        }

        for (handle, format) in materials_to_compile {
            log::debug!("pre_compile_materials: compiling material {:?}", handle);
            self.renderer
                .ensure_material_compiled(handle, format)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Material pre-compilation failed: {}",
                        e
                    ))
                })?;
        }

        Ok(())
    }
}
