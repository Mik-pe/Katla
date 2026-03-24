use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::renderer::types::{DrawCall, DrawList};
use crate::vulkan::commandbuffer::CommandBuffer;
use crate::vulkan::vertex_attribute::AttributeType;
use ash::vk;

impl<'a> Frame<'a> {
    /// Execute a draw list.
    pub(super) fn execute_draw_list(
        &mut self,
        cmd: &CommandBuffer,
        draw_list: &DrawList,
    ) -> Result<(), RenderGraphError> {
        log::trace!(
            "execute_draw_list: {} draw calls to execute",
            draw_list.draws.len()
        );

        for draw_call in &draw_list.draws {
            log::trace!(
                "Executing draw call: mesh={:?}, material={:?}",
                draw_call.mesh,
                draw_call.material
            );
            self.execute_draw_call(cmd, draw_call)?;
        }

        log::trace!(
            "execute_draw_list: completed {} draw calls",
            draw_list.draws.len()
        );

        Ok(())
    }

    /// Execute a single draw call.
    pub(super) fn execute_draw_call(
        &mut self,
        cmd: &CommandBuffer,
        draw_call: &DrawCall,
    ) -> Result<(), RenderGraphError> {
        let mesh_handle = draw_call.mesh;
        let material_handle = draw_call.material;

        let (needs_recompile, material_format) = {
            let material = self
                .renderer
                .asset_registry
                .get_material(material_handle)
                .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;
            (!material.fully_compiled, material.color_format)
        };

        // Recompile material if invalidated (e.g., after descriptor layout change during resize)
        if needs_recompile {
            self.renderer
                .ensure_material_compiled(material_handle, material_format)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Material recompilation failed: {}",
                        e
                    ))
                })?;
        }

        let mesh = self
            .renderer
            .asset_registry
            .get_mesh(mesh_handle)
            .ok_or(RenderGraphError::InvalidMeshHandle(mesh_handle))?;

        // Get material from registry (may have been recompiled above)
        let material = self
            .renderer
            .asset_registry
            .get_material(material_handle)
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;

        // Clone pipeline_handle to avoid holding borrow across bind_descriptor_sets
        let pipeline_handle = material
            .pipeline
            .ok_or(RenderGraphError::InvalidMaterialHandle(draw_call.material))?;

        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Bind vertex buffers (SOA: position(0), normal(1), tangent(2), uv(3))
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
        cmd.bind_vertex_buffers_at_locations(&[
            (0, pos_buf),
            (1, norm_buf),
            (2, tang_buf),
            (3, uv_buf),
        ]);

        if let Some(ib) = &mesh.index_buffer {
            cmd.bind_index_buffer(ib.object(), 0, vk::IndexType::UINT32);
        }

        let index_count = mesh.index_buffer.as_ref().map(|ib| ib.count()).unwrap_or(0);

        // Material borrow ends here, allowing &mut self call below
        let _ = material;

        self.bind_descriptor_sets(cmd, layout, draw_call)?;

        cmd.draw_indexed(index_count, 1, 0, 0, draw_call.instance_index);

        Ok(())
    }

    /// Bind descriptor sets for a draw call.
    ///
    /// Descriptor set layout:
    /// - Set 0: Storage uniforms (frame_data + objects array) - always bound
    /// - Set 1: Bindless textures - always bound for current materials
    /// - Set 2: Skeleton joint matrices - bound only for skinned mesh draws
    /// - Set 3: Light culling data - bound when light culling is active
    pub(super) fn bind_descriptor_sets(
        &mut self,
        cmd: &CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        draw_call: &DrawCall,
    ) -> Result<(), RenderGraphError> {
        // Set 0: Storage uniforms (frame_data + objects array) - use per-frame descriptor set
        let storage_ds =
            self.renderer.storage_descriptor_sets[self.renderer.current_frame()].vk_set();
        cmd.bind_descriptor_sets(pipeline_layout, 0, &[storage_ds], &[]);

        // Set 1: Bindless textures (all current materials use bindless)
        let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(pipeline_layout, 1, &[bindless_ds], &[]);

        // Set 2: Skeleton joint matrices (only when draw_call has skeleton)
        if !draw_call.skeleton.is_none() {
            let skeleton_ds = self
                .renderer
                .get_skeleton_descriptor(draw_call.skeleton)
                .ok_or(RenderGraphError::InvalidSkeletonHandle(draw_call.skeleton))?;
            cmd.bind_descriptor_sets(pipeline_layout, 2, &[skeleton_ds.vk_set()], &[]);
        }

        // Set 3: Forward+ light culling data (push descriptors when active)
        if let Some(lc) = self.renderer.light_culling_buffers()
            && let Err(e) = lc.push_fragment_descriptors(cmd.vk_command_buffer(), pipeline_layout)
        {
            log::warn!("Failed to push light culling fragment descriptors: {}", e);
        }

        // Set 4: Shadow data (regular descriptor set when shadow system is active)
        self.renderer
            .bind_shadow_descriptors(cmd.vk_command_buffer(), pipeline_layout);

        Ok(())
    }
}
