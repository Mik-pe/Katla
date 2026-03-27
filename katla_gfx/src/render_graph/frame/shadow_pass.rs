use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::frame::draw_helpers::{
    DescriptorConfig, DrawParams, draw_meshes_with_skinning,
};
use crate::render_graph::pass::PassDesc;
use crate::render_graph::resource::ResourceState;
use crate::vulkan::commandbuffer::CommandBuffer;
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

        log::debug!(
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

        let (skinned_pipeline, skinned_layout) =
            if let Some(skinned_handle) = self.renderer.shadow_pipeline_skinned() {
                self.renderer
                    .asset_registry
                    .get_pipeline_vk_handles(skinned_handle)
                    .map(|(p, l)| (Some(p), Some(l)))
                    .ok_or(RenderGraphError::InvalidPipelineHandle(skinned_handle))?
            } else {
                (None, None)
            };

        // Viewport/scissor regions for each cascade in the 2x2 atlas
        let viewports = [
            vk::Viewport {
                x: 0.0,
                y: half_h as f32,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
            vk::Viewport {
                x: half_w as f32,
                y: half_h as f32,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
            vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
            vk::Viewport {
                x: half_w as f32,
                y: 0.0,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
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

        // Build extra descriptor sets (shadow cascades at set 2)
        let mut extra_sets = Vec::new();
        if let Some(cascade_ds) = self.renderer.shadow_cascade_descriptor_set() {
            extra_sets.push((2u32, cascade_ds));
        }

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
                    skeleton_set: 3,
                    extra_sets: extra_sets.clone(),
                },
            })?;
        }

        cmd.end_rendering();

        if let Some(write_name) = pass.writes.first() {
            self.resource_states
                .insert(write_name.clone(), ResourceState::DepthStencilAttachment);
        }

        Ok(())
    }
}
