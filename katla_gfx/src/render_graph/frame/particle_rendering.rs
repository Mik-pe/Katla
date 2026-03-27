use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::pass::PassDesc;
use crate::render_graph::transient_texture::TransientTexture;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl<'a> Frame<'a> {
    /// Render particles to a texture using the particle system.
    ///
    /// This starts a new render pass targeting the specified texture.
    pub(super) fn render_particles_to_texture(
        &mut self,
        cmd: &CommandBuffer,
        texture: &TransientTexture,
    ) -> Result<(), RenderGraphError> {
        let _frame_idx = self.current_frame();
        use ash::vk;

        let particle_system = self.renderer.particle_system.as_ref().ok_or_else(|| {
            RenderGraphError::InvalidConfiguration("Particle system not initialized".to_string())
        })?;

        let alive_count = particle_system.alive_count();
        if alive_count == 0 {
            return Ok(()); // No particles to render
        }

        // Transition hdr_color to COLOR_ATTACHMENT_OPTIMAL for particle rendering
        // (it may be in SHADER_READ_ONLY_OPTIMAL from the geometry post-barrier)
        let current_layout = texture.current_layout();
        if current_layout != vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL {
            let barrier = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_READ)
                .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(
                    vk::AccessFlags2::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                )
                .old_layout(current_layout)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .image(texture.image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            let dependency_info =
                vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));
            unsafe {
                self.renderer
                    .context
                    .device
                    .cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dependency_info);
            }
            texture.set_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        }

        let color_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(texture.image_view.vk())
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD) // Load existing HDR output (sky + geometry)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.0; 4] },
            });

        // Depth attachment for depth testing against scene geometry
        let frame_idx = self.current_frame();
        let depth_view = self
            .renderer
            .frame_context
            .depth_render_textures
            .get(frame_idx)
            .map(|t| t.image_view.vk())
            .expect("depth_render_textures must have an entry for current frame");
        let depth_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(depth_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD) // Keep geometry depth
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        let rendering_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: texture.extent,
            })
            .layer_count(1)
            .color_attachments(std::slice::from_ref(&color_attachment))
            .depth_attachment(&depth_attachment);

        // Issue compute-to-graphics buffer barriers BEFORE entering dynamic rendering.
        // Buffer/image memory barriers are not allowed inside cmd_begin_rendering/end_rendering
        // without VK_KHR_dynamic_rendering_local_read.
        if let Some(ref particle_system) = self.renderer.particle_system {
            particle_system.pre_render_barriers(cmd.vk_command_buffer(), frame_idx);
        }

        unsafe {
            self.renderer
                .context
                .device
                .cmd_begin_rendering(cmd.vk_command_buffer(), &rendering_info);
        }

        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: texture.extent.width as f32,
            height: texture.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: texture.extent,
        };

        unsafe {
            self.renderer.context.device.cmd_set_viewport(
                cmd.vk_command_buffer(),
                0,
                std::slice::from_ref(&viewport),
            );
            self.renderer.context.device.cmd_set_scissor(
                cmd.vk_command_buffer(),
                0,
                std::slice::from_ref(&scissor),
            );
        }

        // Get storage descriptor set first to avoid borrow conflicts
        let storage_descriptor_set = if self.renderer.particle_system.is_some() {
            Some(self.renderer.storage_descriptor_sets[self.renderer.current_frame()].vk_set())
        } else {
            None
        };

        let current_frame = self.current_frame();

        if let Some(ref mut particle_system) = self.renderer.particle_system {
            if let Some(pipeline_handle) = particle_system.render_pipeline_handle() {
                // Update render descriptor set to point to the correct frame's alive list
                // (the one simulate just wrote survivors to)
                if let Err(e) = particle_system.update_render_descriptor_binding(current_frame) {
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

                let storage_ds = storage_descriptor_set.ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(
                        "Storage descriptor set not available".to_string(),
                    )
                })?;

                particle_system
                    .render(
                        cmd.vk_command_buffer(),
                        vk::RenderPass::null(), // Using dynamic rendering, not needed
                        vk_pipeline,
                        vk_layout,
                        storage_ds,
                        current_frame,
                    )
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!("Particle render failed: {}", e))
                    })?;

                log::debug!("Drew {} particles successfully", alive_count);
            } else {
                log::warn!("Particle render pipeline not created, skipping particle rendering");
            }
        } else {
            log::warn!("Particle system not available, skipping particle rendering");
        }

        unsafe {
            self.renderer
                .context
                .device
                .cmd_end_rendering(cmd.vk_command_buffer());
        }

        // Transition texture back to SHADER_READ_ONLY_OPTIMAL for subsequent sampling (e.g., UI)
        let old_layout = texture.current_layout();
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
            .old_layout(old_layout)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(texture.image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let dependency_info =
            vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));

        unsafe {
            self.renderer
                .context
                .device
                .cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dependency_info);
        }

        texture.set_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        Ok(())
    }

    /// Execute a compute pass (GPU compute work).
    pub(super) fn execute_compute_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        pipeline_handle: crate::handle::PipelineHandle,
    ) -> Result<(), RenderGraphError> {
        let current_frame = self.current_frame();
        log::debug!(
            "[COMPUTE] Pass '{}' execution: frame_idx={}, pipeline={:?}",
            pass.name,
            current_frame,
            pipeline_handle
        );

        // If the pass has a custom compute callback, use it
        if let Some(ref compute_fn) = pass.compute_fn {
            return compute_fn(self, cmd, pipeline_handle);
        }

        // Generic compute dispatch: bind pipeline and dispatch
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

        // Use dispatch data if provided, otherwise use a default
        let data = self
            .pending
            .get(&self.graph.pass_index(&pass.name).unwrap_or(0))
            .cloned()
            .unwrap_or_default();
        let (x, y, z) = data.dispatch.unwrap_or((64, 1, 1));

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
