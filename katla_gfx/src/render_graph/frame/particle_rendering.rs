use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::pass::PassDesc;
use crate::renderer::VulkanRenderer;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
    /// Execute the particle render pass.
    ///
    /// Renders GPU-simulated particles with alpha blending. Layout transitions
    /// are handled by the render graph barrier system — this method only records
    /// the dynamic rendering scope and draw calls.
    pub(super) fn execute_particle_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
    ) -> Result<(), RenderGraphError> {
        let particle_system = self.renderer.particle_system.as_ref();
        let alive_count = particle_system.map_or(0, |ps| ps.alive_count());
        if alive_count == 0 {
            log::debug!("[PARTICLES] No alive particles, skipping pass");
            return Ok(());
        }

        let frame_idx = self.current_frame();

        // Resolve attachments using the same helpers as other graphics passes.
        let color_attachment = self.resolve_color_attachment(pass)?.ok_or_else(|| {
            RenderGraphError::InvalidConfiguration(
                "Particle pass has no color outputs.".to_string(),
            )
        })?;

        let (depth_attachment, _stencil_attachment) = self.resolve_depth_attachment(pass)?;

        let color_id = pass.writes.first().copied();
        let transient = color_id.and_then(|id| self.graph.transient_texture_by_id(id, frame_idx));
        let extent = transient
            .map(|t| t.extent)
            .unwrap_or_else(|| self.renderer.frame_context.swapchain.get_extent());

        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Issue compute-to-graphics buffer barriers BEFORE entering dynamic rendering.
        if let Some(ref ps) = self.renderer.particle_system {
            ps.pre_render_barriers(cmd.vk_command_buffer(), frame_idx);
        }

        cmd.begin_rendering(
            &[color_attachment],
            depth_attachment.as_ref(),
            None,
            render_area,
            1,
        );

        cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
            0.0,
            0.0,
            extent.width as f32,
            extent.height as f32,
        )]);
        cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
            extent.width,
            extent.height,
        )]);

        let storage_descriptor_set =
            self.renderer.storage_descriptor_sets[self.renderer.current_frame()].vk_set();

        if let Some(ref mut ps) = self.renderer.particle_system {
            if let Some(pipeline_handle) = ps.render_pipeline_handle() {
                if let Err(e) = ps.update_render_descriptor_binding(frame_idx) {
                    log::warn!("Failed to update particle render descriptor binding: {}", e);
                }

                let pipeline_asset = self
                    .renderer
                    .asset_registry
                    .get_pipeline(pipeline_handle)
                    .ok_or_else(|| {
                        RenderGraphError::InvalidConfiguration(format!(
                            "Particle pipeline {:?} not found in registry",
                            pipeline_handle
                        ))
                    })?;

                let vk_pipeline = pipeline_asset.vk_pipeline();
                let vk_layout = pipeline_asset.vk_layout();

                ps.render(
                    cmd.vk_command_buffer(),
                    vk::RenderPass::null(),
                    vk_pipeline,
                    vk_layout,
                    storage_descriptor_set,
                    frame_idx,
                )
                .map_err(|e| {
                    RenderGraphError::BackendError(format!("Particle render failed: {}", e))
                })?;

                log::debug!("[PARTICLES] Drew {} particles", alive_count);
            } else {
                log::warn!("Particle render pipeline not created, skipping");
            }
        }

        cmd.end_rendering();

        Ok(())
    }

    /// Execute a compute pass (GPU compute work).
    pub(super) fn execute_compute_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        pipeline_handle: crate::handle::PipelineHandle,
        dispatch: Option<(u32, u32, u32)>,
    ) -> Result<(), RenderGraphError> {
        let current_frame = self.current_frame();
        log::debug!(
            "[COMPUTE] Pass '{}' execution: frame_idx={}, pipeline={:?}",
            pass.name,
            current_frame,
            pipeline_handle
        );

        if let Some(ref compute_fn) = pass.compute_fn {
            return compute_fn(self, cmd, pipeline_handle);
        }

        let device = &self.renderer.context.device;
        let compute_pipeline = self
            .renderer
            .asset_registry
            .get_pipeline(pipeline_handle)
            .ok_or_else(|| {
                RenderGraphError::PipelineNotSet(format!(
                    "Pipeline {:?} not found",
                    pipeline_handle
                ))
            })?;

        let vk_pipeline = compute_pipeline.vk_pipeline();

        unsafe {
            device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::COMPUTE,
                vk_pipeline,
            );
        }

        let (x, y, z) = dispatch.unwrap_or((64, 1, 1));

        unsafe {
            device.cmd_dispatch(cmd.vk_command_buffer(), x, y, z);
        }

        log::debug!(
            "Compute pass '{}' dispatched ({}, {}, {})",
            pass.name,
            x,
            y,
            z
        );
        Ok(())
    }
}
