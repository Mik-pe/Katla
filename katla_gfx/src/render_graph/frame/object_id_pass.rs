use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::draw_helpers::{
    DescriptorConfig, DrawParams, draw_meshes_with_skinning,
};
use crate::render_graph::frame::{Frame, PassExecutionData};
use crate::render_graph::pass::PassDesc;
use crate::renderer::VulkanRenderer;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
    /// Execute the object-ID picking pass.
    ///
    /// Renders each mesh with a flat color encoding its instance index + 1
    /// into the R32Uint transient texture. Uses depth testing with LoadOp::Load
    /// to reuse the depth buffer from the depth prepass.
    pub(super) fn execute_object_id_pass(
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

        log::debug!(
            "[OBJECT_ID] frame_idx={}, draw_lists={}",
            frame_idx,
            data.draw_lists.len()
        );

        // Color attachment: R32Uint transient texture, ops from the pass
        // declaration (cleared to 0 = no object).
        let color_attachments = self.resolve_color_attachments(pass)?;
        if color_attachments.is_empty() {
            return Err(RenderGraphError::InvalidConfiguration(
                "Object-ID pass has no color outputs".to_string(),
            ));
        }

        // Depth attachment: reuse the depth prepass result via the declared
        // depth ops (depth-only view, no stencil attachment).
        let depth_view = self
            .renderer
            .frame_context
            .depth_render_textures
            .get(frame_idx)
            .map(|t| t.image_view.vk())
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(format!(
                    "depth_render_textures missing entry for frame {}",
                    frame_idx
                ))
            })?;

        let ops = pass.depth_attachment.ok_or_else(|| {
            RenderGraphError::InvalidConfiguration(format!(
                "object-ID pass '{}' declares no depth ops",
                pass.name
            ))
        })?;
        let clear = match ops.depth.clear_value {
            crate::render_pass::ClearValue::DepthStencil { depth, stencil } => {
                vk::ClearDepthStencilValue { depth, stencil }
            }
            _ => vk::ClearDepthStencilValue {
                depth: 0.0,
                stencil: 0,
            },
        };
        let depth_attachment = super::depth_attachment_info(depth_view, &ops.depth, clear);

        cmd.begin_rendering(
            &color_attachments,
            Some(&depth_attachment),
            None,
            render_area,
            1,
        );

        let object_id_pipeline_handle = self.renderer.depth_prepass_pipeline().ok_or(
            RenderGraphError::InvalidConfiguration(
                "Object-ID pass requires the depth-prepass pipeline".to_string(),
            ),
        )?;

        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_handles(object_id_pipeline_handle)?;

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

        let (skinned_pipeline, skinned_layout) =
            if let Some(handle) = self.renderer.depth_prepass_skinned_pipeline() {
                self.renderer
                    .asset_registry
                    .get_pipeline_vk_handles(handle)
                    .map(|(pipeline, layout)| (Some(pipeline), Some(layout)))
                    .unwrap_or((None, None))
            } else {
                (None, None)
            };

        let (billboard_pipeline, billboard_layout) =
            if let Some(handle) = self.renderer.depth_prepass_billboard_pipeline() {
                self.renderer
                    .asset_registry
                    .get_pipeline_vk_handles(handle)
                    .map(|(pipeline, layout)| (Some(pipeline), Some(layout)))
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
                skinned_extra_sets: Vec::new(),
            },
            billboard_pipeline,
            billboard_layout,
            exclude_billboards: false,
        })?;

        cmd.end_rendering();

        Ok(())
    }
}
