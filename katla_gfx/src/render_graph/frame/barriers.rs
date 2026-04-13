use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::frame_graph::BACKBUFFER_NAME;
use crate::render_graph::resource::ResourceState;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl<'a> Frame<'a> {
    /// Insert barriers for a pass.
    ///
    /// Computes required resource states based on pass reads/writes and
    /// inserts layout transitions as needed.
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

        for write_name in &pass.writes {
            // Skip backbuffer - it's managed by the swapchain
            if write_name == BACKBUFFER_NAME {
                continue;
            }

            let Some(transient) = self
                .graph
                .transient_texture(write_name, self.current_frame())
            else {
                continue;
            };

            let is_depth = transient.format == vk::Format::D32_SFLOAT;

            let current_state = self
                .resource_states
                .get(write_name)
                .copied()
                .unwrap_or(ResourceState::Undefined);

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

                // Get the ACTUAL GPU layout from the transient texture
                // This persists across frames via RefCell
                let old_layout = transient.current_layout();

                log::debug!(
                    "[Barrier] Pass '{}' write '{}': {:?} -> {:?}",
                    pass.name,
                    write_name,
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

                // Update tracked state AND GPU layout (persist to TransientTexture for next frame)
                self.resource_states
                    .insert(write_name.clone(), required_state);
                transient.set_layout(required_layout);
            }
        }

        for read_name in &pass.reads {
            // Skip backbuffer - not read by shaders
            if read_name == BACKBUFFER_NAME {
                continue;
            }

            // Skip resources that are also written by this pass — the write barrier
            // handles the layout transition to COLOR_ATTACHMENT_OPTIMAL, and the
            // pass reads the resource as an input attachment or via subpass self-dependency.
            if pass.writes.contains(read_name) {
                continue;
            }

            let Some(transient) = self
                .graph
                .transient_texture(read_name, self.current_frame())
            else {
                continue;
            };

            log::debug!(
                "[BARRIER] Pass '{}' reading transient texture '{}': current_layout={:?}, format={:?}",
                pass.name,
                read_name,
                transient.current_layout(),
                transient.format
            );

            let current_state = self
                .resource_states
                .get(read_name)
                .copied()
                .unwrap_or(ResourceState::Undefined);

            let required_state = ResourceState::ShaderRead;

            if current_state != required_state {
                let required_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;

                // Get the ACTUAL GPU layout from the transient texture
                // This persists across frames via RefCell
                let old_layout = transient.current_layout();

                log::debug!(
                    "[BARRIER] Pass '{}' transitioning '{}' from {:?} to {:?}",
                    pass.name,
                    read_name,
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

                // Update tracked state AND GPU layout (persist to TransientTexture for next frame)
                self.resource_states
                    .insert(read_name.clone(), required_state);
                transient.set_layout(required_layout);
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

        for write_name in &current_pass.writes {
            // Skip backbuffer
            if write_name == BACKBUFFER_NAME {
                continue;
            }

            let Some(transient) = self
                .graph
                .transient_texture(write_name, self.current_frame())
            else {
                continue;
            };

            // Find the next pass that accesses this resource
            let next_access = self.graph.passes[pass_index + 1..]
                .iter()
                .find(|pass| pass.reads.contains(write_name) || pass.writes.contains(write_name));

            // Only transition to SHADER_READ_ONLY if the next access is a read.
            // If the next access is a write, the pre-barrier will handle it.
            let next_is_read = match next_access {
                Some(pass) => pass.reads.contains(write_name) && !pass.writes.contains(write_name),
                None => true, // No more accesses, can transition for potential future sampling
            };

            if !next_is_read {
                continue;
            }

            let current_state = self
                .resource_states
                .get(write_name)
                .copied()
                .unwrap_or(ResourceState::ColorAttachment);

            let needs_transition = current_state == ResourceState::ColorAttachment
                || current_state == ResourceState::Undefined
                || current_state == ResourceState::DepthStencilAttachment;

            if needs_transition {
                let old_layout = transient.current_layout();

                log::debug!(
                    "[PostBarrier] Pass '{}' -> next read '{}': {:?} -> SHADER_READ_ONLY",
                    current_pass.name,
                    write_name,
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

                self.resource_states
                    .insert(write_name.clone(), ResourceState::ShaderRead);
                transient.set_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
            }
        }

        Ok(())
    }
}
