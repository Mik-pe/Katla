use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::frame_graph::BACKBUFFER_NAME;
use crate::render_graph::pass::PassDesc;
use crate::render_graph::passes::ViewportRect;
use crate::render_graph::resource::GraphResourceHandle;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl<'a> Frame<'a> {
    /// Execute a compositing pass (multi-viewport fullscreen pass).
    ///
    /// Compositing passes sample from multiple viewport textures and composite them
    /// onto the final output using viewport rectangles for positioning.
    ///
    /// # Compositing-Specific Behavior
    ///
    /// 1. **Update compositing uniforms**: Upload viewport rectangles to storage buffer
    /// 2. **Bind compositing descriptor set**: Set 2 with viewport texture array
    /// 3. **Draw fullscreen triangle**: Standard fullscreen draw with compositing shader
    pub(super) fn execute_compositing_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        material_handle: crate::handle::MaterialHandle,
    ) -> Result<(), RenderGraphError> {
        let current_frame = self.current_frame();
        let viewports =
            pass.compositing_viewports
                .as_ref()
                .ok_or(RenderGraphError::InvalidConfiguration(
                    "Compositing pass missing viewport data".to_string(),
                ))?;

        log::debug!(
            "[COMPOSITING] Pass '{}' execution: frame_idx={}, viewport_count={}, writes={:?}",
            pass.name,
            current_frame,
            viewports.len(),
            pass.writes
        );

        let extent = self.renderer.frame_context.swapchain.get_extent();

        // With per-frame transient textures, the actual index is base + frame_idx
        let viewport_bindless_idx = if let Some(base_idx) = self.graph.get_ldr_texture_base_index()
        {
            base_idx + current_frame as u32
        } else {
            log::warn!(
                "[COMPOSITING] LDR texture not registered with bindless system, using index 0"
            );
            0
        };

        // Encode viewport count, screen size, and bindless index in frame uniforms
        self.renderer.storage_manager.update_compositing_params(
            current_frame,
            [
                extent.width as f32,
                extent.height as f32,
                viewports.len() as f32,
                viewport_bindless_idx as f32,
            ],
        );

        // TODO: Pass viewport rectangles via proper uniform buffer
        // For now, the shader uses a simple hardcoded split-screen layout
        // This will be enhanced in a follow-up to support arbitrary viewport rectangles

        // Create or update compositing descriptor set with viewport textures
        let compositing_desc_set =
            self.get_or_create_compositing_descriptor_set(viewports, current_frame)?;

        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        let color_attachment = if pass.writes_to(BACKBUFFER_NAME) {
            let swapchain_view =
                self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();
            vk::RenderingAttachmentInfo::default()
                .image_view(swapchain_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
                    },
                })
        } else if let Some(color_name) = pass.writes.first() {
            let frame_idx = self.current_frame();
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
                return Err(RenderGraphError::ResourceNotFound(format!(
                    "Output target '{}' not found",
                    color_name
                )));
            }
        } else {
            return Err(RenderGraphError::InvalidConfiguration(
                "Compositing pass has no output target".to_string(),
            ));
        };

        cmd.begin_rendering(
            &[color_attachment],
            None, // No depth attachment for compositing
            None,
            render_area,
            1,
        );

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

        let material = self
            .renderer
            .asset_registry
            .get_material(material_handle)
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;

        let pipeline_handle = material
            .pipeline
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;

        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_handles(pipeline_handle)?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Set 0: Storage uniforms (frame_data + objects array)
        let storage_ds = self.renderer.storage_descriptor_sets[current_frame].vk_set();
        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

        // Set 1: Bindless textures (shared with all materials)
        let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(layout, 1, &[bindless_ds], &[]);

        // Set 2: Compositing descriptor set (viewport texture array)
        cmd.bind_descriptor_sets(layout, 2, &[compositing_desc_set], &[]);

        cmd.draw_array(3, 1, 0, 0);

        // End rendering
        cmd.end_rendering();

        Ok(())
    }

    /// Get or create compositing descriptor set for current frame.
    ///
    /// Creates or updates a descriptor set with the viewport texture array.
    /// The descriptor set is cached per-frame and updated when viewport textures change.
    pub(super) fn get_or_create_compositing_descriptor_set(
        &mut self,
        viewports: &[(GraphResourceHandle, ViewportRect)],
        frame_idx: usize,
    ) -> Result<vk::DescriptorSet, RenderGraphError> {
        use crate::render_graph::descriptor_sets::CompositingDescriptorSet;
        use std::rc::Rc;

        let mut texture_views = Vec::with_capacity(viewports.len());
        for (handle, _rect) in viewports {
            let resource_name = self
                .graph
                .resource_names
                .iter()
                .find(|&(_, h)| *h == *handle)
                .map(|(name, _)| name.clone())
                .ok_or_else(|| {
                    RenderGraphError::ResourceNotFound(format!(
                        "Viewport texture handle {} not found in resource names",
                        handle.index()
                    ))
                })?;

            log::debug!(
                "[COMPOSITING] Looking up viewport texture: '{}' (handle={})",
                resource_name,
                handle.index()
            );

            let transient = self
                .graph
                .transient_texture(&resource_name, frame_idx)
                .ok_or_else(|| {
                    log::error!(
                        "[COMPOSITING] Failed to find viewport texture '{}' for frame {}",
                        resource_name,
                        frame_idx
                    );
                    RenderGraphError::ResourceNotFound(format!(
                        "Viewport texture '{}' not found for frame {}",
                        resource_name, frame_idx
                    ))
                })?;

            log::debug!(
                "[COMPOSITING] Found viewport texture '{}': format={:?}, extent={}x{}",
                resource_name,
                transient.format,
                transient.extent.width,
                transient.extent.height
            );

            texture_views.push(transient.image_view.vk());
        }

        // Reuse existing descriptor set for this frame, or create one if needed.
        // With UPDATE_AFTER_BIND, we can safely update descriptors while
        // command buffers from the previous frame are still in-flight.
        let context = Rc::clone(&self.renderer.context);
        let mut sets = self.graph.compositing_descriptor_sets.borrow_mut();

        let vk_set = if let Some(ref mut existing) = sets[frame_idx] {
            existing.update_textures(&texture_views).map_err(|e| {
                RenderGraphError::VulkanError(format!(
                    "Failed to update compositing descriptor set: {}",
                    e
                ))
            })?;
            existing.vk_set()
        } else {
            let desc_set = Box::new(
                CompositingDescriptorSet::new(&context, &texture_views).map_err(|e| {
                    RenderGraphError::VulkanError(format!(
                        "Failed to create compositing descriptor set: {}",
                        e
                    ))
                })?,
            );
            let vk_set = desc_set.vk_set();
            sets[frame_idx] = Some(desc_set);
            vk_set
        };
        Ok(vk_set)
    }
}
