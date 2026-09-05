use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::frame::draw_helpers::{
    DescriptorConfig, DrawParams, draw_meshes_with_skinning,
};
use crate::render_graph::pass::PassDesc;
use crate::render_graph::resource::ResourceState;
use crate::renderer::VulkanRenderer;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
    /// Execute a shadow pass.
    ///
    /// All cascades share one atlas render pass and bind separate parameter buffers.
    pub(super) fn execute_shadow_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        data: crate::render_graph::frame::PassExecutionData,
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
            .find_map(|&id| self.graph.transient_texture_by_id(id, frame_idx))
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
            .get_pipeline_handles(shadow_pipeline_handle)?;

        let (skinned_pipeline, skinned_layout) =
            if let Some(skinned_handle) = self.renderer.shadow_pipeline_skinned() {
                self.renderer
                    .asset_registry
                    .get_pipeline_handles(skinned_handle)
                    .map(|(p, l)| (Some(p), Some(l)))?
            } else {
                (None, None)
            };

        let num_cascades: u32 = self.renderer.shadow_cascade_count();
        let depth_bias = self.renderer.shadow_cascade_depth_bias();

        // Build viewports and scissors dynamically based on cascade count
        // Layout: 2x2 grid in the shadow atlas texture
        //   [Cascade 2] [Cascade 3]
        //   [Cascade 0] [Cascade 1]
        let viewports: Vec<vk::Viewport> = (0..num_cascades)
            .map(|i| {
                let col = i % 2;
                let row = 1 - (i / 2);
                vk::Viewport {
                    x: (col * half_w) as f32,
                    y: (row * half_h) as f32,
                    width: half_w as f32,
                    height: half_h as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }
            })
            .collect();

        let scissors: Vec<vk::Rect2D> = (0..num_cascades)
            .map(|i| {
                let col = i % 2;
                let row = 1 - (i / 2);
                vk::Rect2D {
                    offset: vk::Offset2D {
                        x: (col * half_w) as i32,
                        y: (row * half_h) as i32,
                    },
                    extent: vk::Extent2D {
                        width: half_w,
                        height: half_h,
                    },
                }
            })
            .collect();

        cmd.begin_rendering(&[], Some(&depth_attachment), None, render_area, 1);

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

            let cascade_ds = self
                .renderer
                .shadow_cascade_descriptor_set(cascade_idx as usize)
                .ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(
                        "Missing shadow cascade descriptor".into(),
                    )
                })?;
            let extra_sets = vec![
                (1, self.renderer.empty_descriptor_set(frame_idx)),
                (2, cascade_ds),
            ];
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
                    skinned_extra_sets: Vec::new(),
                },
                billboard_pipeline: None,
                billboard_layout: None,
                exclude_billboards: true,
            })?;
        }

        cmd.end_rendering();

        if let Some(&write_id) = pass.writes.first()
            && let Some(transient) = self.graph.transient_texture_by_id(write_id, frame_idx)
        {
            transient.set_state(ResourceState::DepthStencilAttachment);
        }

        Ok(())
    }
}
