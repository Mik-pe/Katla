use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::{Frame, PassExecutionData};
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
        log::debug!(
            "[GRAPHICS] PASS '{}' with frame_idx={}, draw_lists={}, ui_draw_lists={}",
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

        let color_attachment = self
            .resolve_color_attachment(pass)?
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(
                    "Pass has no color outputs. Use .write_color() for transient textures or declare output explicitly".to_string()
                )
            })?;

        let (depth_attachment, stencil_attachment) = self.resolve_depth_attachment(pass);

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
        log::debug!(
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

        let color_attachment = self.resolve_color_attachment(pass)?.ok_or_else(|| {
            RenderGraphError::InvalidConfiguration(
                "Fullscreen pass has no color outputs.".to_string(),
            )
        })?;

        cmd.begin_rendering(&[color_attachment], None, None, render_area, 1);

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

    /// Resolve the depth attachment for a pass.
    ///
    /// Returns `(depth, stencil)` where each is `Option<vk::RenderingAttachmentInfo>`.
    /// Returns `(None, None)` if the pass does not use depth.
    pub(super) fn resolve_depth_attachment(
        &self,
        pass: &PassDesc,
    ) -> (
        Option<vk::RenderingAttachmentInfo<'_>>,
        Option<vk::RenderingAttachmentInfo<'_>>,
    ) {
        if !pass.uses_depth {
            return (None, None);
        }

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

        let (depth_view, stencil) =
            if let Some(ref ds_view) = depth_texture.depth_stencil_image_view {
                (
                    ds_view.vk(),
                    Some(
                        vk::RenderingAttachmentInfo::default()
                            .image_view(ds_view.vk())
                            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                            .load_op(vk::AttachmentLoadOp::CLEAR)
                            .store_op(vk::AttachmentStoreOp::DONT_CARE)
                            .clear_value(vk::ClearValue {
                                depth_stencil: vk::ClearDepthStencilValue {
                                    depth: 0.0,
                                    stencil: 0,
                                },
                            }),
                    ),
                )
            } else {
                (depth_texture.image_view.vk(), None)
            };

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

        (Some(depth), stencil)
    }
}
