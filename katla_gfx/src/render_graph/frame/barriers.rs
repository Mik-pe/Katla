use crate::render_graph::BACKBUFFER_NAME;
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::resource::ResourceState;
use crate::renderer::VulkanRenderer;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
    /// Insert barriers for a pass.
    ///
    /// Computes required resource states based on pass reads/writes and
    /// inserts layout transitions as needed. Uses `TransientTexture::state()`
    /// as the single source of truth for layout state.
    pub(super) fn insert_barriers(
        &mut self,
        cmd: &CommandBuffer,
        pass_index: usize,
    ) -> Result<(), RenderGraphError> {
        use crate::barrier::ImageBarrier;

        let Some(pass) = self.graph.pass(pass_index) else {
            return Ok(());
        };

        log::debug!(
            "[BARRIER] Pre-pass barriers for '{}': reads={:?}, writes={:?}",
            pass.name,
            pass.reads,
            pass.writes
        );

        let cmd_vk = cmd.vk_command_buffer();
        let device = &self.renderer.context.device;

        // Synchronize global depth buffer between consecutive depth-using passes.
        // When a depth prepass writes depth followed by a geometry pass that reads it,
        // an image memory barrier is required even though the layout stays the same.
        if pass.uses_depth && self.depth_buffer_written {
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

        for &write_id in &pass.writes {
            // Skip backbuffer - it's managed by the swapchain
            if self.graph.resource_name(write_id) == Some(BACKBUFFER_NAME) {
                continue;
            }

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

                // Use depth-specific subresource range for depth textures
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

        for &read_id in &pass.reads {
            // Skip backbuffer - not read by shaders
            if self.graph.resource_name(read_id) == Some(BACKBUFFER_NAME) {
                continue;
            }

            // Skip resources that are also written by this pass
            if pass.writes.contains(&read_id) {
                continue;
            }

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

    /// Insert post-pass barriers to ensure proper synchronization.
    ///
    /// This method transitions textures written by the current pass to SHADER_READ_ONLY
    /// only if the immediately next pass that accesses the resource will read it (not write it).
    /// If the next pass writes to the resource, the pre-barrier will handle the transition.
    pub(super) fn insert_post_pass_barriers(
        &mut self,
        cmd: &CommandBuffer,
        pass_index: usize,
    ) -> Result<(), RenderGraphError> {
        use crate::barrier::ImageBarrier;

        let Some(current_pass) = self.graph.pass(pass_index) else {
            return Ok(());
        };

        let cmd_vk = cmd.vk_command_buffer();
        let device = &self.renderer.context.device;

        for &write_id in &current_pass.writes {
            // Skip backbuffer
            if self.graph.resource_name(write_id) == Some(BACKBUFFER_NAME) {
                continue;
            }

            let Some(transient) = self
                .graph
                .transient_texture_by_id(write_id, self.current_frame())
            else {
                continue;
            };

            // Find the next pass in execution order that accesses this resource
            let execution_order = self.graph.execution_order();
            let current_pos = execution_order.iter().position(|&p| p == pass_index);
            let next_access = current_pos.and_then(|pos| {
                execution_order[pos + 1..]
                    .iter()
                    .find(|&&idx| {
                        let p = &self.graph.passes[idx];
                        p.reads.contains(&write_id) || p.writes.contains(&write_id)
                    })
                    .map(|&idx| &self.graph.passes[idx])
            });

            // Only transition to SHADER_READ_ONLY if the next access is a read.
            let next_is_read = match next_access {
                Some(pass) => pass.reads.contains(&write_id) && !pass.writes.contains(&write_id),
                None => true,
            };

            if !next_is_read {
                continue;
            }

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
