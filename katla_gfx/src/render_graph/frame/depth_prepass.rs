use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::{Frame, PassExecutionData};
use crate::render_graph::pass::PassDesc;
use crate::vulkan::commandbuffer::CommandBuffer;
use crate::vulkan::vertex_attribute::AttributeType;
use ash::vk;

impl<'a> Frame<'a> {
    /// Execute a depth prepass — renders depth and object IDs from the camera's perspective.
    ///
    /// Outputs:
    /// - Depth buffer: reused by the geometry pass via `LoadOp::Load` (early-Z rejection)
    /// - Object-ID texture (R32Uint): instance_index + 1 for GPU-based entity picking
    pub(super) fn execute_depth_prepass(
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

        log::trace!(
            "[DEPTH_PREPASS] frame_idx={}, draw_lists={}",
            frame_idx,
            data.draw_lists.len()
        );

        // Color attachment: R32Uint object-ID texture (cleared to 0 = no object)
        let color_attachments: Vec<vk::RenderingAttachmentInfo> = pass
            .writes
            .iter()
            .filter_map(|color_name| {
                self.graph
                    .transient_texture(color_name, frame_idx)
                    .map(|transient| {
                        let (load_op, store_op, clear_value) = pass
                            .color_attachments
                            .iter()
                            .find(|(name, ..)| name == color_name)
                            .map(|(_, _, load_op, store_op, clear_value)| {
                                (
                                    match load_op {
                                        crate::render_pass::LoadOp::Load => {
                                            vk::AttachmentLoadOp::LOAD
                                        }
                                        crate::render_pass::LoadOp::Clear => {
                                            vk::AttachmentLoadOp::CLEAR
                                        }
                                        crate::render_pass::LoadOp::DontCare => {
                                            vk::AttachmentLoadOp::NONE_EXT
                                        }
                                    },
                                    match store_op {
                                        crate::render_pass::StoreOp::Store => {
                                            vk::AttachmentStoreOp::STORE
                                        }
                                        crate::render_pass::StoreOp::DontCare => {
                                            vk::AttachmentStoreOp::NONE_EXT
                                        }
                                    },
                                    match clear_value {
                                        crate::render_pass::ClearValue::Color(c) => {
                                            vk::ClearColorValue {
                                                uint32: [c[0] as u32, 0, 0, 0],
                                            }
                                        }
                                        _ => vk::ClearColorValue {
                                            uint32: [0, 0, 0, 0],
                                        },
                                    },
                                )
                            })
                            .unwrap_or((
                                vk::AttachmentLoadOp::CLEAR,
                                vk::AttachmentStoreOp::STORE,
                                vk::ClearColorValue {
                                    uint32: [0, 0, 0, 0],
                                },
                            ));

                        vk::RenderingAttachmentInfo::default()
                            .image_view(transient.image_view.vk())
                            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                            .load_op(load_op)
                            .store_op(store_op)
                            .clear_value(vk::ClearValue { color: clear_value })
                    })
            })
            .collect();

        // Depth attachment
        let depth_texture = self
            .renderer
            .frame_context
            .depth_render_textures
            .get(frame_idx)
            .expect("depth_render_textures must have an entry for current frame");

        let depth_view = depth_texture.image_view.vk();

        let depth_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(depth_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        // Provide stencil attachment when the depth format has a stencil component
        // to avoid leaving the stencil aspect in an undefined state.
        let stencil_attachment = if depth_texture.depth_stencil_image_view.is_some() {
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

        cmd.begin_rendering(
            &color_attachments,
            Some(&depth_attachment),
            stencil_attachment.as_ref(),
            render_area,
            1,
        );

        let depth_pipeline_handle = self.renderer.depth_prepass_pipeline().ok_or(
            RenderGraphError::InvalidConfiguration(
                "Depth prepass pipeline not initialized".to_string(),
            ),
        )?;

        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(depth_pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(
                depth_pipeline_handle,
            ))?;

        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

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

        // Draw all geometry (depth only — same draw lists as geometry pass)
        let skinned_pipeline_handle = self.renderer.depth_prepass_skinned_pipeline();

        let (skinned_pipeline, skinned_layout) = if let Some(handle) = skinned_pipeline_handle {
            self.renderer
                .asset_registry
                .get_pipeline_vk_handles(handle)
                .map(|(p, l)| (Some(p), Some(l)))
                .unwrap_or((None, None))
        } else {
            (None, None)
        };

        let mut current_pipeline_is_skinned = false;

        for draw_list in &data.draw_lists {
            for draw_call in draw_list.iter() {
                let is_skinned = !draw_call.skeleton.is_none();

                if is_skinned && skinned_pipeline.is_none() {
                    continue;
                }

                if is_skinned != current_pipeline_is_skinned {
                    if is_skinned {
                        unsafe {
                            self.renderer.context.device.cmd_bind_pipeline(
                                cmd.vk_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                skinned_pipeline.unwrap(),
                            );
                        }

                        let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
                        cmd.bind_descriptor_sets(skinned_layout.unwrap(), 0, &[storage_ds], &[]);
                    } else {
                        unsafe {
                            self.renderer.context.device.cmd_bind_pipeline(
                                cmd.vk_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline,
                            );
                        }

                        let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
                        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);
                    }
                    current_pipeline_is_skinned = is_skinned;
                }

                // Bind skeleton descriptor set for skinned meshes (Set 2)
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

                // Depth prepass: bind SOA attribute buffers based on mesh type
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

        cmd.end_rendering();

        Ok(())
    }
}
