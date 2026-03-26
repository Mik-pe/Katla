use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::{Frame, PassExecutionData};
use crate::render_graph::pass::PassDesc;
use crate::vulkan::commandbuffer::CommandBuffer;
use crate::vulkan::vertex_attribute::AttributeType;
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
            log::trace!("[OUTLINE] No draw lists, skipping outline pass");
            return Ok(());
        }

        let frame_idx = self.current_frame();
        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        log::trace!(
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
                log::trace!(
                    "[OUTLINE] Color target '{}' not found, skipping",
                    color_name
                );
                return Ok(());
            }
        } else {
            log::trace!("[OUTLINE] No color outputs, skipping");
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
            .outline
            .stencil_mark_pipeline
            .and_then(|h| self.renderer.asset_registry.get_pipeline_vk_handles(h))
            .ok_or(RenderGraphError::InvalidConfiguration(
                "Stencil mark pipeline not initialized".to_string(),
            ))?;

        let (skinned_pipeline, skinned_layout) = self
            .renderer
            .outline
            .stencil_mark_skinned_pipeline
            .and_then(|h| {
                self.renderer
                    .asset_registry
                    .get_pipeline_vk_handles(h)
                    .map(|(p, l)| (Some(p), Some(l)))
            })
            .unwrap_or((None, None));

        self.draw_with_pipelines(cmd, data, pipeline, layout, skinned_pipeline, skinned_layout)
    }

    /// Promote stencil 1→2 where selected objects are occluded by scene geometry.
    fn execute_occlusion_mark(
        &mut self,
        cmd: &CommandBuffer,
        data: &PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        let (pipeline, layout) = self
            .renderer
            .outline
            .occlusion_mark_pipeline
            .and_then(|h| self.renderer.asset_registry.get_pipeline_vk_handles(h))
            .ok_or(RenderGraphError::InvalidConfiguration(
                "Occlusion mark pipeline not initialized".to_string(),
            ))?;

        let (skinned_pipeline, skinned_layout) = self
            .renderer
            .outline
            .occlusion_mark_skinned_pipeline
            .and_then(|h| {
                self.renderer
                    .asset_registry
                    .get_pipeline_vk_handles(h)
                    .map(|(p, l)| (Some(p), Some(l)))
            })
            .unwrap_or((None, None));

        self.draw_with_pipelines(cmd, data, pipeline, layout, skinned_pipeline, skinned_layout)
    }

    /// Render the outline shell with inverted culling where stencil != 1.
    fn execute_outline_draw(
        &mut self,
        cmd: &CommandBuffer,
        data: &PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        let (pipeline, layout) = self
            .renderer
            .outline
            .outline_draw_pipeline
            .and_then(|h| self.renderer.asset_registry.get_pipeline_vk_handles(h))
            .ok_or(RenderGraphError::InvalidConfiguration(
                "Outline draw pipeline not initialized".to_string(),
            ))?;

        let (skinned_pipeline, skinned_layout) = self
            .renderer
            .outline
            .outline_draw_skinned_pipeline
            .and_then(|h| {
                self.renderer
                    .asset_registry
                    .get_pipeline_vk_handles(h)
                    .map(|(p, l)| (Some(p), Some(l)))
            })
            .unwrap_or((None, None));

        self.draw_with_pipelines(cmd, data, pipeline, layout, skinned_pipeline, skinned_layout)
    }

    fn draw_with_pipelines(
        &mut self,
        cmd: &CommandBuffer,
        data: &PassExecutionData,
        pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        skinned_pipeline: Option<vk::Pipeline>,
        skinned_layout: Option<vk::PipelineLayout>,
    ) -> Result<(), RenderGraphError> {
        let frame_idx = self.current_frame();

        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

        let mut current_is_skinned = false;

        for draw_list in &data.draw_lists {
            for draw_call in draw_list.iter() {
                let is_skinned = !draw_call.skeleton.is_none();

                if is_skinned && skinned_pipeline.is_none() {
                    continue;
                }

                if is_skinned != current_is_skinned {
                    if is_skinned {
                        unsafe {
                            self.renderer.context.device.cmd_bind_pipeline(
                                cmd.vk_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                skinned_pipeline.unwrap(),
                            );
                        }
                        cmd.bind_descriptor_sets(skinned_layout.unwrap(), 0, &[storage_ds], &[]);
                    } else {
                        unsafe {
                            self.renderer.context.device.cmd_bind_pipeline(
                                cmd.vk_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline,
                            );
                        }
                        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);
                    }
                    current_is_skinned = is_skinned;
                }

                if is_skinned {
                    let skeleton_ds = self
                        .renderer
                        .get_skeleton_descriptor(draw_call.skeleton)
                        .ok_or(RenderGraphError::InvalidSkeletonHandle(draw_call.skeleton))?;
                    cmd.bind_descriptor_sets(
                        skinned_layout.unwrap(),
                        2,
                        &[skeleton_ds.vk_set()],
                        &[],
                    );
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
                        (4, joints_buf),
                        (5, weights_buf),
                    ]);
                } else {
                    cmd.bind_vertex_buffers_at_locations(&[(0, pos_buf)]);
                }

                if let Some(ib) = &mesh.index_buffer {
                    cmd.bind_index_buffer(ib.object(), 0, vk::IndexType::UINT32);
                }

                let index_count = mesh.index_buffer.as_ref().map(|ib| ib.count()).unwrap_or(0);

                unsafe {
                    self.renderer.context.device.cmd_draw_indexed(
                        cmd.vk_command_buffer(),
                        index_count,
                        1,
                        0,
                        0,
                        draw_call.instance_index,
                    );
                }
            }
        }

        Ok(())
    }

    /// Execute the stencil indicator pass — writes 1.0 to an R8 texture where stencil == 2.
    /// This texture is later sampled by the tonemap shader to apply the wallhack overlay tint.
    pub(super) fn execute_stencil_indicator_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        if data.draw_lists.is_empty() {
            return Ok(());
        }

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
            .outline
            .stencil_indicator_pipeline
            .and_then(|h| self.renderer.asset_registry.get_pipeline_vk_handles(h))
            .ok_or(RenderGraphError::InvalidConfiguration(
                "Stencil indicator pipeline not initialized".to_string(),
            ))?;

        let (skinned_pipeline, skinned_layout) = self
            .renderer
            .outline
            .stencil_indicator_skinned_pipeline
            .and_then(|h| {
                self.renderer
                    .asset_registry
                    .get_pipeline_vk_handles(h)
                    .map(|(p, l)| (Some(p), Some(l)))
            })
            .unwrap_or((None, None));

        self.draw_with_pipelines(cmd, &data, pipeline, layout, skinned_pipeline, skinned_layout)?;

        cmd.end_rendering();

        Ok(())
    }
}
