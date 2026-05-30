use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::resource::ResourceState;
use crate::renderer::VulkanRenderer;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
    /// Insert barriers for a pass using cached barrier info.
    ///
    /// Uses pre-computed resource lists from the barrier cache to avoid
    /// re-scanning pass dependencies every frame. Layout transitions are
    /// only issued when the current texture state differs from the target.
    pub(super) fn insert_barriers(
        &mut self,
        cmd: &CommandBuffer,
        pass_index: usize,
    ) -> Result<(), RenderGraphError> {
        use crate::barrier::ImageBarrier;

        let Some(pass) = self.graph.pass(pass_index) else {
            return Ok(());
        };

        let cache = self.graph.barrier_cache(pass_index);

        log::debug!(
            "[BARRIER] Pre-pass barriers for '{}': reads={:?}, writes={:?}",
            pass.name,
            pass.reads,
            pass.writes
        );

        let cmd_vk = cmd.vk_command_buffer();
        let device = &self.renderer.context.device;

        if cache.is_some_and(|c| c.needs_depth_sync) && self.depth_buffer_written {
            let frame_idx = self.current_frame();
            if let Some(depth_texture) = self
                .renderer
                .frame_context
                .depth_render_textures
                .get(frame_idx)
            {
                log::debug!(
                    "[BARRIER] Depth render-pass sync before '{}' (previous pass wrote depth)",
                    pass.name
                );
                ImageBarrier::depth_render_pass_sync(&cmd_vk, device, depth_texture.image.vk());
            }
        }

        let write_resources = cache
            .map(|c| c.pre_write_resources.as_slice())
            .unwrap_or(&[]);
        let read_resources = cache
            .map(|c| c.pre_read_resources.as_slice())
            .unwrap_or(&[]);

        for &write_id in write_resources {
            let Some(transient) = self
                .graph
                .transient_texture_by_id(write_id, self.current_frame())
            else {
                continue;
            };

            let is_depth = transient.format == vk::Format::D32_SFLOAT;

            let current_state = transient.state();

            let required_state = if is_depth {
                ResourceState::DepthStencilAttachment
            } else {
                ResourceState::ColorAttachment
            };

            if current_state != required_state {
                let required_layout = if is_depth {
                    vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
                } else {
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                };

                let old_layout = transient.current_layout();

                log::debug!(
                    "[Barrier] Pass '{}' write '{}': {:?} -> {:?}",
                    pass.name,
                    self.graph.resource_name(write_id).unwrap_or("?"),
                    old_layout,
                    required_layout
                );

                if is_depth {
                    ImageBarrier::transition_with_range(
                        &cmd_vk,
                        device,
                        transient.image,
                        old_layout,
                        required_layout,
                        vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::DEPTH,
                            base_mip_level: 0,
                            level_count: vk::REMAINING_MIP_LEVELS,
                            base_array_layer: 0,
                            layer_count: vk::REMAINING_ARRAY_LAYERS,
                        },
                    );
                } else {
                    ImageBarrier::transition(
                        &cmd_vk,
                        device,
                        transient.image,
                        old_layout,
                        required_layout,
                    );
                }

                transient.set_layout(required_layout);
                transient.set_state(required_state);
            }
        }

        for &read_id in read_resources {
            let Some(transient) = self
                .graph
                .transient_texture_by_id(read_id, self.current_frame())
            else {
                continue;
            };

            log::debug!(
                "[BARRIER] Pass '{}' reading transient texture '{}': current_layout={:?}, format={:?}",
                pass.name,
                self.graph.resource_name(read_id).unwrap_or("?"),
                transient.current_layout(),
                transient.format
            );

            let current_state = transient.state();

            let required_state = ResourceState::ShaderRead;

            if current_state != required_state {
                let required_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;

                let old_layout = transient.current_layout();

                log::debug!(
                    "[BARRIER] Pass '{}' transitioning '{}' from {:?} to {:?}",
                    pass.name,
                    self.graph.resource_name(read_id).unwrap_or("?"),
                    old_layout,
                    required_layout
                );

                let is_depth = transient.format == vk::Format::D32_SFLOAT;
                if is_depth {
                    ImageBarrier::transition_with_range(
                        &cmd_vk,
                        device,
                        transient.image,
                        old_layout,
                        required_layout,
                        vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::DEPTH,
                            base_mip_level: 0,
                            level_count: vk::REMAINING_MIP_LEVELS,
                            base_array_layer: 0,
                            layer_count: vk::REMAINING_ARRAY_LAYERS,
                        },
                    );
                } else {
                    ImageBarrier::transition(
                        &cmd_vk,
                        device,
                        transient.image,
                        old_layout,
                        required_layout,
                    );
                }

                transient.set_layout(required_layout);
                transient.set_state(required_state);
            }
        }

        Ok(())
    }

    /// Insert post-pass barriers using cached next-access info.
    ///
    /// Uses pre-computed resource lists from the barrier cache to avoid
    /// scanning the execution order for the next accessing pass every frame.
    pub(super) fn insert_post_pass_barriers(
        &mut self,
        cmd: &CommandBuffer,
        pass_index: usize,
    ) -> Result<(), RenderGraphError> {
        use crate::barrier::ImageBarrier;

        let Some(current_pass) = self.graph.pass(pass_index) else {
            return Ok(());
        };

        let Some(cache) = self.graph.barrier_cache(pass_index) else {
            return Ok(());
        };

        let cmd_vk = cmd.vk_command_buffer();
        let device = &self.renderer.context.device;

        for &write_id in &cache.post_write_to_read_resources {
            let Some(transient) = self
                .graph
                .transient_texture_by_id(write_id, self.current_frame())
            else {
                continue;
            };

            let current_state = transient.state();

            let needs_transition = current_state == ResourceState::ColorAttachment
                || current_state == ResourceState::Undefined
                || current_state == ResourceState::DepthStencilAttachment;

            if needs_transition {
                let old_layout = transient.current_layout();

                log::debug!(
                    "[PostBarrier] Pass '{}' -> next read '{}': {:?} -> SHADER_READ_ONLY",
                    current_pass.name,
                    self.graph.resource_name(write_id).unwrap_or("?"),
                    old_layout
                );

                let is_depth = transient.format == vk::Format::D32_SFLOAT;
                if is_depth {
                    ImageBarrier::transition_with_range(
                        &cmd_vk,
                        device,
                        transient.image,
                        old_layout,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::DEPTH,
                            base_mip_level: 0,
                            level_count: vk::REMAINING_MIP_LEVELS,
                            base_array_layer: 0,
                            layer_count: vk::REMAINING_ARRAY_LAYERS,
                        },
                    );
                } else {
                    ImageBarrier::transition(
                        &cmd_vk,
                        device,
                        transient.image,
                        old_layout,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    );
                }

                transient.set_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
                transient.set_state(ResourceState::ShaderRead);
            }
        }

        Ok(())
    }
}
