use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::{Frame, PassExecutionData};
use crate::render_graph::frame_graph::BACKBUFFER_NAME;
use crate::render_graph::pass::PassDesc;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl<'a> Frame<'a> {
    /// Execute a graphics pass with dynamic rendering.
    pub(super) fn execute_graphics_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        log::trace!(
            "🎨 [GRAPHICS] PASS '{}' with frame_idx={}, draw_lists={}, ui_draw_lists={}",
            pass.name,
            self.current_frame(),
            data.draw_lists.len(),
            data.ui_draw_lists.len()
        );

        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Determine color attachment:
        // 1. If pass writes to "backbuffer", use swapchain directly
        // 2. If pass writes to a transient texture, use that (frame-indexed)
        // 3. Use load_op from pass.color_attachments if available, otherwise default to CLEAR
        //    For backbuffer: use LOAD if a previous pass already wrote to it
        let color_attachment = if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
            let swapchain_view =
                self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();

            let backbuffer_written = self.resource_states.contains_key(BACKBUFFER_NAME);
            let load_op = if backbuffer_written {
                log::trace!(
                    "✅ PASS '{}': Using LOAD for backbuffer (previous pass wrote to it)",
                    pass.name
                );
                vk::AttachmentLoadOp::LOAD
            } else {
                log::warn!(
                    "⚠️  PASS '{}': Using CLEAR for backbuffer (first write) - WILL OVERWRITE PREVIOUS CONTENT!",
                    pass.name
                );
                vk::AttachmentLoadOp::CLEAR
            };

            vk::RenderingAttachmentInfo::default()
                .image_view(swapchain_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(load_op)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.1, 0.1, 0.1, 1.0],
                    },
                })
        } else if let Some(color_name) = pass.writes.first() {
            if let Some(transient) = self
                .graph
                .transient_texture(color_name, self.current_frame())
            {
                let (load_op, store_op, clear_value) = pass
                    .color_attachments
                    .iter()
                    .find(|(name, ..)| name == color_name)
                    .map(|(_, _, load_op, store_op, clear_value)| {
                        (
                            match load_op {
                                crate::render_pass::LoadOp::Load => vk::AttachmentLoadOp::LOAD,
                                crate::render_pass::LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
                                crate::render_pass::LoadOp::DontCare => {
                                    vk::AttachmentLoadOp::NONE_EXT
                                }
                            },
                            match store_op {
                                crate::render_pass::StoreOp::Store => vk::AttachmentStoreOp::STORE,
                                crate::render_pass::StoreOp::DontCare => {
                                    vk::AttachmentStoreOp::NONE_EXT
                                }
                            },
                            match clear_value {
                                crate::render_pass::ClearValue::Color(c) => {
                                    vk::ClearColorValue { float32: *c }
                                }
                                _ => vk::ClearColorValue {
                                    float32: [0.0, 0.0, 0.0, 1.0],
                                },
                            },
                        )
                    })
                    .unwrap_or((
                        vk::AttachmentLoadOp::CLEAR,
                        vk::AttachmentStoreOp::STORE,
                        vk::ClearColorValue {
                            float32: [0.1, 0.1, 0.1, 1.0],
                        },
                    ));

                vk::RenderingAttachmentInfo::default()
                    .image_view(transient.image_view.vk())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(load_op)
                    .store_op(store_op)
                    .clear_value(vk::ClearValue { color: clear_value })
            } else {
                return Err(RenderGraphError::ResourceNotFound(format!(
                    "Color target '{}' not found. Use 'backbuffer' for swapchain or create a transient resource.",
                    color_name
                )));
            }
        } else {
            return Err(RenderGraphError::InvalidConfiguration(
                "Pass has no color outputs. Use .write_color() for transient textures or declare output explicitly".to_string()
            ));
        };

        // Depth attachment (only for passes that use depth testing)
        // Use per-frame depth buffer to prevent data races when multiple frames
        // execute concurrently on the GPU (e.g., MAILBOX present mode).
        let (depth_attachment, stencil_attachment) = if pass.uses_depth {
            let frame_idx = self.current_frame();
            let depth_texture = self
                .renderer
                .frame_context
                .depth_render_textures
                .get(frame_idx)
                .expect("depth_render_textures must have an entry for current frame");

            let (load_op, store_op, clear_depth) = pass
                .depth_attachment
                .map(|(lo, so, cv)| {
                    let depth_val = match cv {
                        crate::render_pass::ClearValue::DepthStencil { depth, .. } => depth,
                        _ => 0.0,
                    };
                    (lo.into(), so.into(), depth_val)
                })
                .unwrap_or((
                    vk::AttachmentLoadOp::CLEAR,
                    vk::AttachmentStoreOp::STORE,
                    0.0,
                ));

            let depth_view = depth_texture.image_view.vk();
            let depth = vk::RenderingAttachmentInfo::default()
                .image_view(depth_view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(load_op)
                .store_op(store_op)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: clear_depth,
                        stencil: 0,
                    },
                });

            // Provide stencil attachment when the depth format has a stencil component.
            // Without this, the stencil aspect becomes UNDEFINED after the render pass,
            // which can cause issues with subsequent passes that use stencil (e.g., outline).
            let stencil = if depth_texture.depth_stencil_image_view.is_some() {
                Some(
                    vk::RenderingAttachmentInfo::default()
                        .image_view(
                            depth_texture
                                .depth_stencil_image_view
                                .as_ref()
                                .unwrap()
                                .vk(),
                        )
                        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                        .load_op(vk::AttachmentLoadOp::CLEAR)
                        .store_op(vk::AttachmentStoreOp::DONT_CARE)
                        .clear_value(vk::ClearValue {
                            depth_stencil: vk::ClearDepthStencilValue {
                                depth: 0.0,
                                stencil: 0,
                            },
                        }),
                )
            } else {
                None
            };

            (Some(depth), stencil)
        } else {
            (None, None)
        };

        cmd.begin_rendering(
            &[color_attachment],
            depth_attachment.as_ref(),
            stencil_attachment.as_ref(),
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

        for draw_list in &data.draw_lists {
            self.execute_draw_list(cmd, draw_list)?;
        }

        for ui_draw_list in &data.ui_draw_lists {
            self.execute_ui_draw_list(cmd, pass, ui_draw_list)?;
        }

        cmd.end_rendering();

        Ok(())
    }

    /// Execute a fullscreen pass (draws a fullscreen triangle).
    pub(super) fn execute_fullscreen_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        pipeline_handle: crate::handle::PipelineHandle,
    ) -> Result<(), RenderGraphError> {
        let current_frame = self.current_frame();
        log::trace!(
            "[FULLSCREEN] Pass '{}' execution: frame_idx={}, writes={:?}, reads={:?}",
            pass.name,
            current_frame,
            pass.writes,
            pass.reads
        );

        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Determine color attachment:
        // 1. If pass writes to "backbuffer", use swapchain directly
        // 2. If pass writes to a transient texture, use that (frame-indexed)
        // 3. Use load_op from pass.color_attachments if available, otherwise default to CLEAR
        let color_attachment = if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
            let swapchain_view =
                self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();
            vk::RenderingAttachmentInfo::default()
                .image_view(swapchain_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.1, 0.1, 0.1, 1.0],
                    },
                })
        } else if let Some(color_name) = pass.writes.first() {
            let frame_idx = self.current_frame();
            if let Some(transient) = self.graph.transient_texture(color_name, frame_idx) {
                log::trace!(
                    "[FULLSCREEN] Pass '{}' writing to transient texture '{}' at frame_idx={}, format={:?}, extent={}x{}",
                    pass.name,
                    color_name,
                    frame_idx,
                    transient.format,
                    transient.extent.width,
                    transient.extent.height
                );

                let (load_op, store_op, clear_value) = pass
                    .color_attachments
                    .iter()
                    .find(|(name, ..)| name == color_name)
                    .map(|(_, _, load_op, store_op, clear_value)| {
                        (
                            match load_op {
                                crate::render_pass::LoadOp::Load => vk::AttachmentLoadOp::LOAD,
                                crate::render_pass::LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
                                crate::render_pass::LoadOp::DontCare => {
                                    vk::AttachmentLoadOp::NONE_EXT
                                }
                            },
                            match store_op {
                                crate::render_pass::StoreOp::Store => vk::AttachmentStoreOp::STORE,
                                crate::render_pass::StoreOp::DontCare => {
                                    vk::AttachmentStoreOp::NONE_EXT
                                }
                            },
                            match clear_value {
                                crate::render_pass::ClearValue::Color(c) => {
                                    vk::ClearColorValue { float32: *c }
                                }
                                _ => vk::ClearColorValue {
                                    float32: [0.0, 0.0, 0.0, 1.0],
                                },
                            },
                        )
                    })
                    .unwrap_or((
                        vk::AttachmentLoadOp::CLEAR,
                        vk::AttachmentStoreOp::STORE,
                        vk::ClearColorValue {
                            float32: [0.1, 0.1, 0.1, 1.0],
                        },
                    ));

                vk::RenderingAttachmentInfo::default()
                    .image_view(transient.image_view.vk())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(load_op)
                    .store_op(store_op)
                    .clear_value(vk::ClearValue { color: clear_value })
            } else {
                return Err(RenderGraphError::ResourceNotFound(format!(
                    "Color target '{}' not found. Use 'backbuffer' for swapchain or create a transient resource.",
                    color_name
                )));
            }
        } else {
            return Err(RenderGraphError::InvalidConfiguration(
                "Pass has no color outputs. Use 'backbuffer' for swapchain or create a transient resource.".to_string()
            ));
        };

        cmd.begin_rendering(
            &[color_attachment],
            None, // No depth attachment for fullscreen passes
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

        let storage_ds =
            self.renderer.storage_descriptor_sets[self.renderer.current_frame()].vk_set();
        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

        let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(layout, 1, &[bindless_ds], &[]);

        // Skip fullscreen draw for tonemap passes with no HDR input (e.g., background clear pass).
        // The render pass clear color already provides the desired output.
        // Non-tonemap fullscreen passes (e.g., sky) always draw.
        let skip_draw = pass
            .tonemap_params
            .as_ref()
            .is_some_and(|p| p.hdr_texture_index.is_none());

        if !skip_draw {
            cmd.draw_array(3, 1, 0, 0);
        }

        cmd.end_rendering();

        Ok(())
    }
}
