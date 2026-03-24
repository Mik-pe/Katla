use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::pass::PassDesc;
use crate::render_graph::resource::ResourceState;
use crate::vulkan::commandbuffer::CommandBuffer;
use crate::vulkan::vertex_attribute::AttributeType;
use ash::vk;

impl<'a> Frame<'a> {
    /// Execute a shadow pass.
    ///
    /// Phase 1: Clears the shadow atlas depth to 1.0 (far plane).
    /// Future phases will render actual shadow depth from the light's perspective.
    pub(super) fn execute_shadow_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
    ) -> Result<(), RenderGraphError> {
        let frame_idx = self.current_frame();

        log::trace!(
            "[SHADOW] Pass '{}' execution: frame_idx={}, writes={:?}",
            pass.name,
            frame_idx,
            pass.writes
        );

        let shadow_atlas = pass
            .writes
            .iter()
            .find_map(|w| self.graph.transient_texture(w, frame_idx))
            .ok_or_else(|| {
                RenderGraphError::ResourceNotFound(
                    "Shadow pass has no depth texture to write to".to_string(),
                )
            })?;

        let extent = shadow_atlas.extent;
        let half_w = extent.width / 2;
        let half_h = extent.height / 2;

        let depth_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(shadow_atlas.image_view.vk())
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            });

        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        cmd.begin_rendering(&[], Some(&depth_attachment), None, render_area, 1);

        let shadow_pipeline_handle =
            self.renderer
                .shadow_pipeline()
                .ok_or(RenderGraphError::InvalidConfiguration(
                    "Shadow pipeline not initialized. Call init_shadow_pipeline() first."
                        .to_string(),
                ))?;

        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(shadow_pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(
                shadow_pipeline_handle,
            ))?;

        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Bind descriptor sets:
        // Set 0: storage uniforms (frame_data + objects) — per-frame
        // Set 2: shadow cascades — per-frame
        let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

        if let Some(cascade_ds) = self.renderer.shadow_cascade_descriptor_set() {
            cmd.bind_descriptor_sets(layout, 2, &[cascade_ds], &[]);
        }

        // Render geometry for each cascade.
        // Each cascade gets its own viewport region in the 2x2 atlas.
        // Cascade index is passed via push constants to the shadow depth shader.
        //
        // Atlas layout (4096x4096):
        //   cascade 0 (near)  -> top-left:     (0, half_h, half_w, half_h)
        //   cascade 1         -> top-right:    (half_w, half_h, half_w, half_h)
        //   cascade 2         -> bottom-left:  (0, 0, half_w, half_h)
        //   cascade 3 (far)   -> bottom-right: (half_w, 0, half_w, half_h)
        //
        // Note: Vulkan viewport Y=0 is at the TOP of the image
        let viewports = [
            vk::Viewport {
                x: 0.0,
                y: half_h as f32,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }, // cascade 0 (top-left)
            vk::Viewport {
                x: half_w as f32,
                y: half_h as f32,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }, // cascade 1 (top-right)
            vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }, // cascade 2 (bottom-left)
            vk::Viewport {
                x: half_w as f32,
                y: 0.0,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }, // cascade 3 (bottom-right)
        ];

        let scissors = [
            vk::Rect2D {
                offset: vk::Offset2D {
                    x: 0,
                    y: half_h as i32,
                },
                extent: vk::Extent2D {
                    width: half_w,
                    height: half_h,
                },
            },
            vk::Rect2D {
                offset: vk::Offset2D {
                    x: half_w as i32,
                    y: half_h as i32,
                },
                extent: vk::Extent2D {
                    width: half_w,
                    height: half_h,
                },
            },
            vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: half_w,
                    height: half_h,
                },
            },
            vk::Rect2D {
                offset: vk::Offset2D {
                    x: half_w as i32,
                    y: 0,
                },
                extent: vk::Extent2D {
                    width: half_w,
                    height: half_h,
                },
            },
        ];

        // Render geometry for each cascade.
        // Each cascade gets its own viewport region in the 2x2 atlas.
        // Cascade index is passed via push constants to the shadow depth shader.
        let data = self
            .pending
            .remove(&self.graph.pass_index(&pass.name).unwrap_or(0))
            .unwrap_or_default();

        let num_cascades: u32 = self
            .renderer
            .shadow
            .csm
            .as_ref()
            .map(|csm| csm.cascade_count() as u32)
            .unwrap_or(4);

        let depth_bias = self
            .renderer
            .shadow
            .csm
            .as_ref()
            .map(|csm| csm.params().depth_bias_slope)
            .unwrap_or(2.0);

        for cascade_idx in 0..num_cascades {
            let vp = viewports[cascade_idx as usize];
            let sc = scissors[cascade_idx as usize];

            unsafe {
                self.renderer.context.device.cmd_set_viewport(
                    cmd.vk_command_buffer(),
                    0,
                    std::slice::from_ref(&vp),
                );
                self.renderer.context.device.cmd_set_scissor(
                    cmd.vk_command_buffer(),
                    0,
                    std::slice::from_ref(&sc),
                );
            }

            self.renderer
                .set_shadow_cascade_params(cascade_idx, depth_bias);

            // --- Non-skinned meshes (regular shadow pipeline) ---
            for draw_list in &data.draw_lists {
                for draw_call in &draw_list.draws {
                    if !draw_call.skeleton.is_none() {
                        continue;
                    }

                    let mesh = self
                        .renderer
                        .asset_registry
                        .get_mesh(draw_call.mesh)
                        .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?;

                    // Shadow pipeline needs position(0) only
                    let pos_buf = mesh
                        .get_attribute_buffer(AttributeType::Position)
                        .map(|vb| vb.object())
                        .unwrap_or(vk::Buffer::null());
                    cmd.bind_vertex_buffers_at_locations(&[(0, pos_buf)]);

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

            // --- Skinned meshes (skinned shadow pipeline) ---
            if let Some(skinned_pipeline_handle) = self.renderer.shadow_pipeline_skinned() {
                let (skinned_pipeline, skinned_layout) = self
                    .renderer
                    .asset_registry
                    .get_pipeline_vk_handles(skinned_pipeline_handle)
                    .ok_or(RenderGraphError::InvalidPipelineHandle(
                        skinned_pipeline_handle,
                    ))?;

                unsafe {
                    self.renderer.context.device.cmd_bind_pipeline(
                        cmd.vk_command_buffer(),
                        vk::PipelineBindPoint::GRAPHICS,
                        skinned_pipeline,
                    );
                }

                // Re-bind descriptor sets for the skinned pipeline layout:
                // Set 0: storage uniforms (frame_data + objects)
                // Set 2: shadow cascades
                let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
                cmd.bind_descriptor_sets(skinned_layout, 0, &[storage_ds], &[]);

                if let Some(cascade_ds) = self.renderer.shadow_cascade_descriptor_set() {
                    cmd.bind_descriptor_sets(skinned_layout, 2, &[cascade_ds], &[]);
                }

                for draw_list in &data.draw_lists {
                    for draw_call in &draw_list.draws {
                        if draw_call.skeleton.is_none() {
                            continue;
                        }

                        // Bind Set 3: skeleton joint matrices for this draw call
                        let skeleton_ds = self
                            .renderer
                            .get_skeleton_descriptor(draw_call.skeleton)
                            .ok_or(RenderGraphError::InvalidSkeletonHandle(draw_call.skeleton))?;
                        cmd.bind_descriptor_sets(skinned_layout, 3, &[skeleton_ds.vk_set()], &[]);

                        let mesh = self
                            .renderer
                            .asset_registry
                            .get_mesh(draw_call.mesh)
                            .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?;

                        // Skinned shadow pipeline needs position(0) + joint_indices(4) + joint_weights(5)
                        let pos_buf = mesh
                            .get_attribute_buffer(AttributeType::Position)
                            .map(|vb| vb.object())
                            .unwrap_or(vk::Buffer::null());
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

                        if let Some(ib) = &mesh.index_buffer {
                            cmd.bind_index_buffer(ib.object(), 0, vk::IndexType::UINT32);
                        }

                        let index_count =
                            mesh.index_buffer.as_ref().map(|ib| ib.count()).unwrap_or(0);

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

                // Switch back to the regular shadow pipeline for the next cascade iteration
                unsafe {
                    self.renderer.context.device.cmd_bind_pipeline(
                        cmd.vk_command_buffer(),
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline,
                    );
                }

                let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
                cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

                if let Some(cascade_ds) = self.renderer.shadow_cascade_descriptor_set() {
                    cmd.bind_descriptor_sets(layout, 2, &[cascade_ds], &[]);
                }
            }
        }

        cmd.end_rendering();

        if let Some(write_name) = pass.writes.first() {
            self.resource_states
                .insert(write_name.clone(), ResourceState::DepthStencilAttachment);
        }

        Ok(())
    }
}
