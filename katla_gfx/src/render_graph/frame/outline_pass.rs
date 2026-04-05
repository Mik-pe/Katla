use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::draw_helpers::{
    DescriptorConfig, DrawParams, draw_meshes_with_skinning,
};
use crate::render_graph::frame::{Frame, PassExecutionData};
use crate::render_graph::pass::PassDesc;
use crate::renderer::outline::{OutlinePushConstants, compute_outline_width};
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl<'a> Frame<'a> {
    /// Execute the outline pass — stencil-based selection highlight.
    ///
    /// Three sub-passes within a single render pass:
    /// 1. Stencil mark: render selected objects to stencil buffer (bit 0 = visible)
    /// 2. Occlusion mark: set stencil bit 1 where selected objects are occluded
    /// 3. Outline draw: render inverted-culled shell where stencil == 0
    ///
    /// Self-occlusion prevention: The occlusion mark uses compare_mask=0x01 to
    /// only process pixels where bit 0 is clear (no visible front face). This
    /// prevents back faces of the same object from being incorrectly marked as
    /// occluded. The stencil indicator pass reads stencil == 2 (both bits set).
    pub(super) fn execute_outline_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        if data.draw_lists.is_empty() {
            log::debug!("[OUTLINE] No draw lists, skipping outline pass");
            return Ok(());
        }

        let frame_idx = self.current_frame();
        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        log::debug!(
            "[OUTLINE] frame_idx={}, draw_lists={}",
            frame_idx,
            data.draw_lists.len()
        );

        // Determine color attachment (should be hdr_color)
        let color_attachment = if let Some(color_name) = pass.writes.first() {
            if let Some(transient) = self.graph.transient_texture(color_name, frame_idx) {
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
                        vk::AttachmentLoadOp::LOAD,
                        vk::AttachmentStoreOp::STORE,
                        vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    ));

                vk::RenderingAttachmentInfo::default()
                    .image_view(transient.image_view.vk())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(load_op)
                    .store_op(store_op)
                    .clear_value(vk::ClearValue { color: clear_value })
            } else {
                log::debug!(
                    "[OUTLINE] Color target '{}' not found, skipping",
                    color_name
                );
                return Ok(());
            }
        } else {
            log::debug!("[OUTLINE] No color outputs, skipping");
            return Ok(());
        };

        let ds_view = self
            .renderer
            .frame_context
            .depth_render_textures
            .get(frame_idx)
            .and_then(|t| t.depth_stencil_image_view.as_ref().map(|v| v.vk()))
            .expect("depth_stencil_image_view must exist for D32SfloatS8Uint format");

        // Load depth from geometry pass (has all objects' depth for correct
        // outline depth testing). Clear stencil (not written by geometry pass).
        let depth_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(ds_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        let stencil_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(ds_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        cmd.begin_rendering(
            &[color_attachment],
            Some(&depth_attachment),
            Some(&stencil_attachment),
            render_area,
            1,
        );

        cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
            0.0,
            0.0,
            extent.width as f32,
            extent.height as f32,
        )]);

        let scissor = crate::sync::Rect2D {
            x: 0,
            y: 0,
            width: extent.width,
            height: extent.height,
        };
        cmd.set_scissor(&[scissor]);

        // === Sub-pass 1: Stencil Mark ===
        self.execute_stencil_mark(cmd, &data)?;

        // === Sub-pass 2: Occlusion Mark ===
        self.execute_occlusion_mark(cmd, &data)?;

        // === Sub-pass 3: Outline Draw ===
        self.execute_outline_draw(cmd, &data)?;

        cmd.end_rendering();

        Ok(())
    }

    /// Render selected objects to the stencil buffer (reference value 1).
    fn execute_stencil_mark(
        &mut self,
        cmd: &CommandBuffer,
        data: &PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.stencil_mark_pipeline)
            .ok_or(RenderGraphError::InvalidConfiguration(
                "Stencil mark pipeline not initialized".to_string(),
            ))?;

        let (skinned_pipeline, skinned_layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.stencil_mark_skinned_pipeline)
            .map(|(p, l)| (Some(p), Some(l)))
            .unwrap_or((None, None));

        self.draw_with_pipelines(
            cmd,
            data,
            pipeline,
            layout,
            skinned_pipeline,
            skinned_layout,
            Vec::new(),
        )
    }

    /// Promote stencil 1→2 where selected objects are occluded by scene geometry.
    fn execute_occlusion_mark(
        &mut self,
        cmd: &CommandBuffer,
        data: &PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.occlusion_mark_pipeline)
            .ok_or(RenderGraphError::InvalidConfiguration(
                "Occlusion mark pipeline not initialized".to_string(),
            ))?;

        let (skinned_pipeline, skinned_layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.occlusion_mark_skinned_pipeline)
            .map(|(p, l)| (Some(p), Some(l)))
            .unwrap_or((None, None));

        self.draw_with_pipelines(
            cmd,
            data,
            pipeline,
            layout,
            skinned_pipeline,
            skinned_layout,
            Vec::new(),
        )
    }

    /// Render the outline shell with inverted culling where stencil != 1.
    fn execute_outline_draw(
        &mut self,
        cmd: &CommandBuffer,
        data: &PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.outline_draw_pipeline)
            .ok_or(RenderGraphError::InvalidConfiguration(
                "Outline draw pipeline not initialized".to_string(),
            ))?;

        let (skinned_pipeline, skinned_layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.outline_draw_skinned_pipeline)
            .map(|(p, l)| (Some(p), Some(l)))
            .unwrap_or((None, None));

        let extent = self.renderer.frame_context.swapchain.get_extent();
        let frame_idx = self.current_frame();
        let mut push_constants = OutlinePushConstants::default();
        push_constants.outline_width = compute_outline_width(extent.height as f32);

        // Update the uniform buffer for this frame
        unsafe {
            let ptr = self
                .renderer
                .context
                .map_buffer(&self.renderer.outline.params_allocations[frame_idx])
                .expect("Failed to map buffer");
            std::ptr::copy_nonoverlapping(
                &push_constants as *const _ as *const u8,
                ptr,
                std::mem::size_of::<OutlinePushConstants>(),
            );
        }
        let _ = self.renderer.context.flush_mapped_memory(
            &self.renderer.outline.params_allocations[frame_idx],
            0,
            std::mem::size_of::<OutlinePushConstants>() as u64,
        );

        // Non-skinned: outline params at set 1
        // Skinned: outline params at set 3
        let params_ds = self.renderer.outline.params_descriptor_sets[frame_idx];

        let extra_sets = vec![(1u32, params_ds), (3u32, params_ds)];

        self.draw_with_pipelines(
            cmd,
            data,
            pipeline,
            layout,
            skinned_pipeline,
            skinned_layout,
            extra_sets,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_with_pipelines(
        &mut self,
        cmd: &CommandBuffer,
        data: &PassExecutionData,
        pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        skinned_pipeline: Option<vk::Pipeline>,
        skinned_layout: Option<vk::PipelineLayout>,
        extra_sets: Vec<(u32, vk::DescriptorSet)>,
    ) -> Result<(), RenderGraphError> {
        let frame_idx = self.current_frame();
        draw_meshes_with_skinning(DrawParams {
            cmd,
            renderer: self.renderer,
            draw_lists: &data.draw_lists,
            pipeline,
            layout,
            skinned_pipeline,
            skinned_layout,
            frame_idx,
            descriptors: DescriptorConfig {
                bind_textures: false,
                skeleton_set: 2,
                extra_sets,
            },
        })
    }

    /// Execute the stencil indicator pass — writes 1.0 to an R8 texture where stencil == 2.
    /// This texture is later sampled by the tonemap shader to apply the wallhack overlay tint.
    /// When no draw lists are submitted (nothing selected), the texture is still cleared
    /// to prevent stale overlay from a previous selection.
    pub(super) fn execute_stencil_indicator_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        let frame_idx = self.current_frame();
        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        let color_attachment = if let Some(color_name) = pass.writes.first() {
            if let Some(transient) = self.graph.transient_texture(color_name, frame_idx) {
                vk::RenderingAttachmentInfo::default()
                    .image_view(transient.image_view.vk())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    })
            } else {
                return Ok(());
            }
        } else {
            return Ok(());
        };

        // Always begin a render pass to clear the stencil indicator texture,
        // even when nothing is selected. Without this, stale overlay data
        // from a previous selection persists on screen.
        let ds_view = self
            .renderer
            .frame_context
            .depth_render_textures
            .get(frame_idx)
            .and_then(|t| t.depth_stencil_image_view.as_ref().map(|v| v.vk()))
            .expect("depth_stencil_image_view must exist for D32SfloatS8Uint format");

        let depth_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(ds_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        let stencil_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(ds_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        cmd.begin_rendering(
            &[color_attachment],
            Some(&depth_attachment),
            Some(&stencil_attachment),
            render_area,
            1,
        );

        if data.draw_lists.is_empty() {
            cmd.end_rendering();
            return Ok(());
        }

        cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
            0.0,
            0.0,
            extent.width as f32,
            extent.height as f32,
        )]);

        let scissor = crate::sync::Rect2D {
            x: 0,
            y: 0,
            width: extent.width,
            height: extent.height,
        };
        cmd.set_scissor(&[scissor]);

        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.stencil_indicator_pipeline)
            .ok_or(RenderGraphError::InvalidConfiguration(
                "Stencil indicator pipeline not initialized".to_string(),
            ))?;

        let (skinned_pipeline, skinned_layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.stencil_indicator_skinned_pipeline)
            .map(|(p, l)| (Some(p), Some(l)))
            .unwrap_or((None, None));

        self.draw_with_pipelines(
            cmd,
            &data,
            pipeline,
            layout,
            skinned_pipeline,
            skinned_layout,
            Vec::new(),
        )?;

        cmd.end_rendering();

        Ok(())
    }
}
