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
        let extent = self.color_target_extent(pass);
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        log::debug!(
            "[DEPTH_PREPASS] frame_idx={}, draw_lists={}",
            frame_idx,
            data.draw_lists.len()
        );

        // Color attachments (R32Uint object-ID textures cleared to 0) come
        // from the pass declaration.
        let color_attachments = self.resolve_color_attachments(pass)?;

        // Depth + stencil attachments come from the declared depth ops.
        let (depth_attachment, stencil_attachment) = self.resolve_frame_depth_attachments(pass)?;
        let (Some(depth_attachment), stencil_attachment) = (depth_attachment, stencil_attachment)
        else {
            return Err(RenderGraphError::InvalidConfiguration(
                "Depth prepass must write depth".to_string(),
            ));
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

        let (billboard_pipeline, billboard_layout) =
            if let Some(handle) = self.renderer.depth_prepass_billboard_pipeline() {
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
