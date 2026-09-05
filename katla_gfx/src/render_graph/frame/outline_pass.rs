use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::draw_helpers::{
    DescriptorConfig, DrawParams, draw_meshes_with_skinning,
};
use crate::render_graph::frame::{Frame, PassExecutionData};
use crate::render_graph::pass::PassDesc;
use crate::renderer::VulkanRenderer;
use crate::renderer::outline::{OutlinePushConstants, compute_outline_width};
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
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
        let extent = self.color_target_extent(pass);

        // Compute a tight scissor rect from the selected entity's screen-space bounds.
        // This avoids clearing/loading/storing the full-resolution stencil buffer on
        // tile-based GPUs (Apple Silicon), which is the main cause of the framerate
        // drop when an entity is selected.
        let scissor_rect =
            compute_outline_scissor(&data.draw_lists, &self.renderer.frame_uniforms, extent);

        let render_area = scissor_rect;

        log::debug!(
            "[OUTLINE] frame_idx={}, draw_lists={}",
            frame_idx,
            data.draw_lists.len()
        );

        // Determine color attachment (should be hdr_color)
        let color_attachment = if let Some(&color_id) = pass.writes.first() {
            if let Some(transient) = self.graph.transient_texture_by_id(color_id, frame_idx) {
                let (load_op, store_op, clear_value) = pass
                    .color_attachments
                    .iter()
                    .find(|(id, ..)| *id == color_id)
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
                    self.graph.resource_name(color_id).unwrap_or("?")
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
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(
                    "depth_stencil_image_view must exist for D32SfloatS8Uint format".to_string(),
                )
            })?;
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

        // Viewport must match the geometry pass (full swapchain extent) so the
        // projection matrix maps clip coords to the same pixel positions.
        // Only the scissor is tightened to the outline region.
        cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
            0.0,
            0.0,
            extent.width as f32,
            extent.height as f32,
        )]);

        cmd.set_scissor(&[crate::sync::Rect2D {
            x: render_area.offset.x,
            y: render_area.offset.y,
            width: render_area.extent.width,
            height: render_area.extent.height,
        }]);

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

        let skinned = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.stencil_mark_skinned_pipeline);

        self.draw_outline_meshes(cmd, data, pipeline, layout, skinned, Vec::new(), Vec::new())
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

        let skinned = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.occlusion_mark_skinned_pipeline);

        self.draw_outline_meshes(cmd, data, pipeline, layout, skinned, Vec::new(), Vec::new())
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

        let skinned = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.outline_draw_skinned_pipeline);

        let extent = self.renderer.frame_context.scene_extent;
        let frame_idx = self.current_frame();
        let mut push_constants = OutlinePushConstants::default();
        push_constants.outline_width = compute_outline_width(extent.height as f32);

        // Update the uniform buffer for this frame
        unsafe {
            let ptr = self
                .renderer
                .context
                .map_buffer(&self.renderer.outline.params_allocations[frame_idx])?;
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

        // Non-skinned: outline params at set 1 (pipeline layout: [storage, outline_params])
        // Skinned: outline params at set 3 (pipeline layout: [storage, empty, skeleton, outline_params])
        let params_ds = self.renderer.outline.params_descriptor_sets[frame_idx];

        let extra_sets = vec![(1u32, params_ds)];
        let skinned_extra_sets = vec![(3u32, params_ds)];

        self.draw_outline_meshes(
            cmd,
            data,
            pipeline,
            layout,
            skinned,
            extra_sets,
            skinned_extra_sets,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_outline_meshes(
        &mut self,
        cmd: &CommandBuffer,
        data: &PassExecutionData,
        pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        skinned: Option<(vk::Pipeline, vk::PipelineLayout)>,
        extra_sets: Vec<(u32, vk::DescriptorSet)>,
        skinned_extra_sets: Vec<(u32, vk::DescriptorSet)>,
    ) -> Result<(), RenderGraphError> {
        let (skinned_pipeline, skinned_layout) = skinned
            .map(|(p, l)| (Some(p), Some(l)))
            .unwrap_or((None, None));
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
                skinned_extra_sets,
            },
            billboard_pipeline: None,
            billboard_layout: None,
            exclude_billboards: true,
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
        let extent = self.color_target_extent(pass);
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        let color_attachment = if let Some(&color_id) = pass.writes.first() {
            if let Some(transient) = self.graph.transient_texture_by_id(color_id, frame_idx) {
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
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(
                    "depth_stencil_image_view must exist for D32SfloatS8Uint format".to_string(),
                )
            })?;

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

        let indicator_scissor =
            compute_outline_scissor(&data.draw_lists, &self.renderer.frame_uniforms, extent);

        // Viewport must match the geometry pass (full swapchain extent) so the
        // projection matrix maps clip coords to the same pixel positions.
        cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
            0.0,
            0.0,
            extent.width as f32,
            extent.height as f32,
        )]);

        cmd.set_scissor(&[crate::sync::Rect2D {
            x: indicator_scissor.offset.x,
            y: indicator_scissor.offset.y,
            width: indicator_scissor.extent.width,
            height: indicator_scissor.extent.height,
        }]);

        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.stencil_indicator_pipeline)
            .ok_or(RenderGraphError::InvalidConfiguration(
                "Stencil indicator pipeline not initialized".to_string(),
            ))?;

        let skinned = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(self.renderer.outline.stencil_indicator_skinned_pipeline);

        self.draw_outline_meshes(
            cmd,
            &data,
            pipeline,
            layout,
            skinned,
            Vec::new(),
            Vec::new(),
        )?;

        cmd.end_rendering();

        Ok(())
    }
}

/// Compute a tight scissor rect around the selected entity's screen-space projection.
///
/// Transforms unit-cube corners through the model matrix to get world-space bounds,
/// then projects to screen. Adds padding for the outline width.
/// Falls back to full extent if projection fails.
fn compute_outline_scissor(
    draw_lists: &[std::rc::Rc<crate::renderer::types::DrawList>],
    frame_uniforms: &crate::renderer::FrameUniforms,
    extent: vk::Extent2D,
) -> vk::Rect2D {
    let view = &frame_uniforms.view_matrix;
    let proj = &frame_uniforms.proj_matrix;

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    let w = extent.width as f32;
    let h = extent.height as f32;

    for draw_list in draw_lists {
        for draw_call in draw_list.iter() {
            let Some(m) = draw_call.instances.first().map(|i| i.model_matrix) else {
                continue;
            };

            for &(dx, dy, dz) in &[
                (-1.0, -1.0, -1.0),
                (1.0, -1.0, -1.0),
                (-1.0, 1.0, -1.0),
                (-1.0, -1.0, 1.0),
                (1.0, 1.0, -1.0),
                (1.0, -1.0, 1.0),
                (-1.0, 1.0, 1.0),
                (1.0, 1.0, 1.0),
            ] {
                // Transform corner through the full model matrix (M * corner).
                // The model matrix includes rotation, scale, and translation.
                let wx = m[0] * dx + m[4] * dy + m[8] * dz + m[12];
                let wy = m[1] * dx + m[5] * dy + m[9] * dz + m[13];
                let wz = m[2] * dx + m[6] * dy + m[10] * dz + m[14];

                // proj * view * world_pos (combined into one step)
                let vx = view[0] * wx + view[4] * wy + view[8] * wz + view[12];
                let vy = view[1] * wx + view[5] * wy + view[9] * wz + view[13];
                let vz = view[2] * wx + view[6] * wy + view[10] * wz + view[14];
                let vw = view[3] * wx + view[7] * wy + view[11] * wz + view[15];

                let clip_x = proj[0] * vx + proj[4] * vy + proj[8] * vz + proj[12] * vw;
                let clip_y = proj[1] * vx + proj[5] * vy + proj[9] * vz + proj[13] * vw;
                let clip_w = proj[3] * vx + proj[7] * vy + proj[11] * vz + proj[15] * vw;

                if clip_w <= 1e-6 {
                    continue;
                }

                let ndc_x = clip_x / clip_w;
                let ndc_y = clip_y / clip_w;

                let screen_x = (ndc_x * 0.5 + 0.5) * w;
                let screen_y = (ndc_y * 0.5 + 0.5) * h;

                min_x = min_x.min(screen_x);
                min_y = min_y.min(screen_y);
                max_x = max_x.max(screen_x);
                max_y = max_y.max(screen_y);
            }
        }
    }

    if min_x > max_x || min_y > max_y {
        return vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };
    }

    let padding = compute_outline_width(h) * h * 0.5 + 8.0;
    min_x = (min_x - padding).max(0.0);
    min_y = (min_y - padding).max(0.0);
    max_x = (max_x + padding).min(w);
    max_y = (max_y + padding).min(h);

    vk::Rect2D {
        offset: vk::Offset2D {
            x: min_x as i32,
            y: min_y as i32,
        },
        extent: vk::Extent2D {
            width: (max_x - min_x).max(1.0) as u32,
            height: (max_y - min_y).max(1.0) as u32,
        },
    }
}
