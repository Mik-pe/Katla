use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::draw_helpers::{
    DescriptorConfig, DrawParams, draw_meshes_with_skinning,
};
use crate::render_graph::frame::{Frame, PassExecutionData};
use crate::render_graph::pass::PassDesc;
use crate::vulkan::commandbuffer::CommandBuffer;
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

        log::debug!(
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

        let (depth_view, stencil_attachment) =
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
            .get_pipeline_handles(depth_pipeline_handle)?;

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

        let (skinned_pipeline, skinned_layout) =
            if let Some(handle) = self.renderer.depth_prepass_skinned_pipeline() {
                self.renderer
                    .asset_registry
                    .get_pipeline_vk_handles(handle)
                    .map(|(p, l)| (Some(p), Some(l)))
                    .unwrap_or((None, None))
            } else {
                (None, None)
            };

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
                extra_sets: Vec::new(),
            },
        })?;

        cmd.end_rendering();

        Ok(())
    }
}
