use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::frame_graph::FrameGraph;
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

                log::trace!("Drew {} particles successfully", alive_count);
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
    ///
    /// Compute passes perform general-purpose GPU computation without rendering to attachments.
    /// Used for particle simulation, physics, and other compute-intensive tasks.
    ///
    /// # Compute-Specific Behavior
    ///
    /// 1. **Bind compute pipeline**: Set pipeline for compute work
    /// 2. **Bind descriptor sets**: Set 0 (static buffers) + Set 1 (push descriptors if needed)
    /// 3. **Dispatch compute shader**: Execute with specified workgroup count
    pub(super) fn execute_compute_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        pipeline_handle: crate::handle::PipelineHandle,
    ) -> Result<(), RenderGraphError> {
        let current_frame = self.current_frame();
        log::trace!(
            "[COMPUTE] Pass '{}' execution: frame_idx={}, pipeline={:?}",
            pass.name,
            current_frame,
            pipeline_handle
        );

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

        let current_frame = self.current_frame();

        // Bind descriptor sets if particle system is active
        // Note: Particle system manages its own descriptor sets
        if let Some(ref mut particle_system) = self.renderer.particle_system
            && pass.name.contains("particle")
        {
            log::trace!("Executing particle compute pass '{}'", pass.name);

            // Use pre-calculated workgroup count from frame graph
            // These were calculated in renderer.rs based on current particle state
            let workgroup_count = if pass.name.contains("emit") {
                self.graph.particle_emit_workgroup_count
            } else if pass.name.contains("simulate") {
                self.graph.particle_simulate_workgroup_count
            } else {
                log::warn!(
                    "Unknown particle compute pass '{}', using default workgroup count",
                    pass.name
                );
                1
            };

            // Before recording dispatch
            if workgroup_count == 0 {
                log::debug!(
                    "Skipping particle compute pass '{}' - workgroup_count is 0",
                    pass.name
                );
                return Ok(()); // Skip dispatch
            }

            // Record the appropriate dispatch based on pass name
            if pass.name.contains("emit") {
                // Update compute descriptor bindings for EMIT pass
                // CRITICAL: Emit needs binding 2 to point to alive[frame_index]
                // so that newly emitted particles are appended where simulate will read them
                particle_system
                    .update_compute_descriptor_binding(current_frame)
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Failed to update particle compute descriptor binding: {}",
                            e
                        ))
                    })?;
                particle_system
                    .record_emit_dispatch(
                        cmd.vk_command_buffer(),
                        &self.renderer.asset_registry,
                        workgroup_count,
                        current_frame,
                    )
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Particle emit dispatch failed: {}",
                            e
                        ))
                    })?;
                // Mark that emit ran this frame so simulate knows not to
                // overwrite emit_count.
                let graph_ptr = self.graph as *const FrameGraph as *mut FrameGraph;
                unsafe {
                    (*graph_ptr).particle_emit_ran = true;
                }
            } else if pass.name.contains("simulate") {
                // Update compute descriptor bindings for SIMULATE pass
                // CRITICAL: Simulate needs binding 3 to point to alive[(frame+1)%2]
                // so that survivors are written to the region render will read from
                particle_system
                    .update_compute_descriptor_binding(current_frame)
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Failed to update particle compute descriptor binding: {}",
                            e
                        ))
                    })?;

                // Reset counters before simulate.
                // When emit was skipped, alive_count and emit_count must be reset
                // here since emit didn't do it.
                let emit_ran = self.graph.particle_emit_ran;
                particle_system.reset_simulate_counters(
                    cmd.vk_command_buffer(),
                    emit_ran,
                    current_frame,
                );

                particle_system
                    .record_simulate_dispatch(
                        cmd.vk_command_buffer(),
                        &self.renderer.asset_registry,
                        workgroup_count,
                        current_frame,
                    )
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Particle simulate dispatch failed: {}",
                            e
                        ))
                    })?;

                // No swap needed — simulate writes survivors to alive[(frame+1)%2] via
                // descriptor offset flip in update_compute_descriptor_binding. The render
                // pass reads from the same region via update_render_descriptor_binding.

                // Record particle debug readback if requested this frame
                // SAFETY: We need to access the graph's debug readback flag through the Frame's graph reference
                // This is safe because we're in the middle of frame execution and have exclusive access
                let graph_ptr = self.graph as *const FrameGraph as *mut FrameGraph;
                unsafe {
                    if (*graph_ptr).particle_debug_readback {
                        log::info!("Recording particle debug readback after simulate pass");
                        particle_system
                            .record_debug_readback(cmd.vk_command_buffer(), current_frame)
                            .map_err(|e| {
                                RenderGraphError::VulkanError(format!(
                                    "Particle debug readback failed: {}",
                                    e
                                ))
                            })?;
                        // Reset flag after recording
                        (*graph_ptr).particle_debug_readback = false;
                    }
                }
            }

            return Ok(());
        }

        // Generic compute dispatch for non-particle compute passes
        // TODO: Calculate workgroup count based on work size
        unsafe {
            device.cmd_dispatch(cmd.vk_command_buffer(), 64, 1, 1);
        }

        log::trace!("Compute pass '{}' executed successfully", pass.name);
        Ok(())
    }
}
