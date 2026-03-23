use std::collections::HashMap;

use super::error::RenderGraphError;
use super::frame_graph::{BACKBUFFER_NAME, FrameGraph};
use super::pass::PassDesc;
use super::passes::ViewportRect;
use super::resource::{GraphResourceHandle, ResourceState};
use super::transient_texture::TransientTexture;
use crate::handle::PipelineHandle;
use crate::renderer::VulkanRenderer;
use crate::renderer::types::{DrawCall, DrawList, UIDrawList};
use crate::vulkan::commandbuffer::CommandBuffer;
use crate::vulkan::vertex_attribute::AttributeType;
use ash::vk;
use gpu_allocator::vulkan::Allocation;

/// Frame context for submitting work to passes.
///
/// Passed to the closure in [`VulkanRenderer::render()`]. Provides a simple
/// API for submitting draw lists to named passes.
pub struct Frame<'a> {
    /// Reference to the frame graph.
    graph: &'a FrameGraph,

    /// Reference to the Vulkan renderer.
    /// Mutable reference allows access to per-frame resources like UI buffers.
    renderer: &'a mut VulkanRenderer,

    /// Current swapchain image index being rendered to.
    image_index: u32,

    /// Pending pass execution data.
    pending: HashMap<usize, PassExecutionData>,

    /// Current state of transient resources (name -> state).
    resource_states: HashMap<String, ResourceState>,

    /// Per-frame temporary buffers (allocated during this frame, cleaned up after GPU completion).
    temporary_buffers: Vec<(vk::Buffer, Allocation)>,

    /// Whether the global depth buffer has been written by a previous pass this frame.
    /// Used to insert a synchronization barrier between depth prepass and geometry pass.
    depth_buffer_written: bool,
}

/// Data for a single pass execution.
#[derive(Default, Clone)]
struct PassExecutionData {
    /// Draw lists to render in this pass.
    draw_lists: Vec<DrawList>,

    /// UI draw lists to render in this pass.
    ui_draw_lists: Vec<UIDrawList>,

    /// Whether dispatch was requested.
    dispatch: Option<(u32, u32, u32)>,

    /// Custom uniform data.
    uniform_data: Vec<u8>,
}

impl<'a> Frame<'a> {
    /// Create a new frame context.
    pub(crate) fn new(
        graph: &'a FrameGraph,
        renderer: &'a mut VulkanRenderer,
        image_index: u32,
        _frame_idx: usize,
    ) -> Self {
        // Initialize all transient resources as Undefined
        let resource_states: HashMap<String, ResourceState> = graph
            .transient_resources
            .iter()
            .map(|desc| (desc.name.clone(), ResourceState::Undefined))
            .collect();

        Self {
            graph,
            renderer,
            image_index,
            pending: HashMap::new(),
            resource_states,
            temporary_buffers: Vec::new(),
            depth_buffer_written: false,
        }
    }

    /// Get the current frame index from the renderer.
    /// This is the authoritative source for which frame's resources to use.
    fn current_frame(&self) -> usize {
        self.renderer.current_frame()
    }

    /// Submit a draw list to a pass.
    pub fn submit(&mut self, pass: &str, draw_list: &DrawList) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        self.pending
            .entry(index)
            .or_default()
            .draw_lists
            .push(draw_list.clone());
        self
    }

    /// Submit a UI draw list to a pass.
    pub fn submit_ui(&mut self, pass: &str, ui_draw_list: &UIDrawList) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        let cmd_count = ui_draw_list.commands.len();
        self.pending
            .entry(index)
            .or_default()
            .ui_draw_lists
            .push(ui_draw_list.clone());

        log::trace!(
            "submit_ui: pass='{}', index={}, commands={}, pending UI lists now={}",
            pass,
            index,
            cmd_count,
            self.pending[&index].ui_draw_lists.len()
        );

        self
    }

    /// Dispatch compute workgroups for a pass.
    pub fn dispatch(&mut self, pass: &str, x: u32, y: u32, z: u32) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        self.pending.entry(index).or_default().dispatch = Some((x, y, z));
        self
    }

    /// Push uniform data for a pass.
    pub fn push_uniform(&mut self, pass: &str, data: &[u8]) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        self.pending
            .entry(index)
            .or_default()
            .uniform_data
            .extend_from_slice(data);
        self
    }

    /// Execute all passes in order.
    pub(super) fn execute_passes(&mut self) -> Result<(), RenderGraphError> {
        // Reset per-frame particle state
        // SAFETY: We have exclusive access during frame execution
        let graph_ptr = self.graph as *const FrameGraph as *mut FrameGraph;
        unsafe {
            (*graph_ptr).particle_emit_ran = false;
        }

        // Use storage_manager.current_frame() consistently for all frame resource selection
        let frame_idx = self.current_frame();
        log::trace!(
            "=== execute_passes: frame_idx={}, {} passes to execute ===",
            frame_idx,
            self.graph.passes.len()
        );
        for (idx, pass) in self.graph.passes.iter().enumerate() {
            log::trace!(
                "  Pass {}: '{}' (type={:?})",
                idx,
                pass.name,
                pass.pass_type
            );
        }

        // Clone the command buffer to avoid borrowing issues
        let cmd = self.renderer.frame_context.command_buffers[frame_idx].clone();

        // === PHASE 1: Execute compute dispatches (BEFORE any render passes) ===
        // Vulkan doesn't allow compute dispatches inside a render pass, so we must
        // execute all particle simulation compute shaders before beginning any rendering.
        // NOTE: Particle compute is now handled by the render graph via ComputePass.
        // The particle_compute pass executes before all graphics passes automatically.

        // === PHASE 2: Execute graphics passes ===
        for (index, pass) in self.graph.passes.iter().enumerate() {
            let data = self.pending.remove(&index).unwrap_or_default();

            if pass.name == "ui" {
                log::trace!(
                    "UI pass execution: index={}, frame_idx={}, ui_draw_lists={}, commands={}",
                    index,
                    self.current_frame(),
                    data.ui_draw_lists.len(),
                    data.ui_draw_lists
                        .iter()
                        .map(|l| l.commands.len())
                        .sum::<usize>()
                );
            }

            log::trace!(
                "Executing pass '{}' (index {}): pipeline={:?}, draw_lists={}, writes={:?}",
                pass.name,
                index,
                pass.pipeline,
                data.draw_lists.len(),
                pass.writes
            );

            // Track which writes happened this frame (for debugging black screen issues)
            if !pass.writes.is_empty() {
                log::trace!("Pass '{}' writes to: {:?}", pass.name, pass.writes);
            }

            // CRITICAL: Track backbuffer state BEFORE pass execution
            // This allows subsequent passes that write to backbuffer to use LOAD instead of CLEAR
            // For example: compositing pass writes to backbuffer, then UI pass should LOAD that content
            if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
                log::trace!(
                    "Pass '{}' will write to backbuffer, tracking state BEFORE execution",
                    pass.name
                );
                log::trace!(
                    "Current resource_states: {:?}",
                    self.resource_states.keys().collect::<Vec<_>>()
                );
                self.resource_states
                    .insert(BACKBUFFER_NAME.to_string(), ResourceState::ColorAttachment);
                log::trace!(
                    "After tracking, resource_states: {:?}",
                    self.resource_states.keys().collect::<Vec<_>>()
                );
            }

            // Insert pre-pass barriers
            self.insert_barriers(&cmd, index)?;

            // Execute pass based on type
            match pass.pass_type {
                super::pass::PassType::Graphics => {
                    // Check if this is a shadow pass (no material, no pipeline, no color writes,
                    // but has depth writes via transient resources)
                    if pass.material.is_none()
                        && pass.pipeline.is_none()
                        && pass.writes.iter().any(|w| {
                            self.graph
                                .transient_texture(w, self.current_frame())
                                .map(|t| t.format == vk::Format::D32_SFLOAT)
                                .unwrap_or(false)
                        })
                    {
                        log::trace!("'{}' -> shadow pass (no-op, clearing atlas)", pass.name);
                        self.execute_shadow_pass(&cmd, pass)?;
                    }
                    // Check if this is a depth prepass (no material, no pipeline, no color writes,
                    // no transient depth writes — uses global depth buffer)
                    else if pass.material.is_none()
                        && pass.pipeline.is_none()
                        && pass.writes.is_empty()
                        && pass.uses_depth
                    {
                        log::trace!("'{}' -> depth prepass", pass.name);
                        self.execute_depth_prepass(&cmd, pass, data)?;
                    }
                    // Check if this is a compositing pass (has material AND compositing_viewports)
                    else if let Some(material_handle) = pass.material {
                        if pass.compositing_viewports.is_some() && data.draw_lists.is_empty() {
                            log::trace!("'{}' -> compositing pass", pass.name);
                            self.execute_compositing_pass(&cmd, pass, material_handle)?;
                        } else {
                            // Pass has material but is NOT compositing (e.g., UI pass)
                            // Fall through to graphics pass execution
                            log::trace!(
                                "'{}' -> graphics pass with material (draw_lists={}, ui_draw_lists={})",
                                pass.name,
                                data.draw_lists.len(),
                                data.ui_draw_lists.len()
                            );
                            self.execute_graphics_pass(&cmd, pass, data)?;
                        }
                    }
                    // Check if this is a fullscreen pass (has pipeline, no draw lists)
                    else if pass.pipeline.is_some() && data.draw_lists.is_empty() {
                        log::trace!("'{}' -> fullscreen pass", pass.name);
                        if let Some(pipeline) = pass.pipeline {
                            self.execute_fullscreen_pass(&cmd, pass, pipeline)?;
                        }
                    } else {
                        log::trace!(
                            "'{}' -> graphics pass (draw_lists={}, ui_draw_lists={})",
                            pass.name,
                            data.draw_lists.len(),
                            data.ui_draw_lists.len()
                        );
                        self.execute_graphics_pass(&cmd, pass, data)?;
                    }
                }
                super::pass::PassType::Compute => {
                    // Compute pass (e.g., particle simulation)
                    log::trace!("'{}' -> compute pass", pass.name);
                    if let Some(pipeline) = pass.pipeline {
                        self.execute_compute_pass(&cmd, pass, pipeline)?;
                    } else {
                        log::warn!("Compute pass '{}' has no pipeline", pass.name);
                    }
                }
            }

            // Insert post-pass barriers for transient textures that will be read by subsequent passes
            // This ensures proper synchronization between write and read operations
            self.insert_post_pass_barriers(&cmd, index)?;

            // Track depth buffer writes for synchronization with subsequent passes
            if pass.uses_depth {
                self.depth_buffer_written = true;
            }

            // Render particles after geometry pass (before tonemap, so they get tonemapped
            // and depth-tested against the scene geometry)
            if pass.name == "geometry" {
                if let Some(ref particle_system) = self.renderer.particle_system {
                    let alive_count = particle_system.alive_count();

                    if alive_count > 0 {
                        if let Some(hdr_texture) = self
                            .graph
                            .transient_textures
                            .get(frame_idx)
                            .and_then(|m| m.get("hdr_color"))
                        {
                            if let Err(e) = self.render_particles_to_texture(&cmd, hdr_texture) {
                                log::error!("Failed to render particles: {}", e);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Insert barriers for a pass.
    ///
    /// Computes required resource states based on pass reads/writes and
    /// inserts layout transitions as needed.
    fn insert_barriers(
        &mut self,
        cmd: &CommandBuffer,
        pass_index: usize,
    ) -> Result<(), RenderGraphError> {
        use crate::barrier::ImageBarrier;

        let Some(pass) = self.graph.pass(pass_index) else {
            return Ok(());
        };

        log::trace!(
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
                log::trace!(
                    "[BARRIER] Depth render-pass sync before '{}' (previous pass wrote depth)",
                    pass.name
                );
                ImageBarrier::depth_render_pass_sync(&cmd_vk, device, depth_texture.image.vk());
            }
        }

        // Process writes first (color attachments)
        for write_name in &pass.writes {
            // Skip backbuffer - it's managed by the swapchain
            if write_name == BACKBUFFER_NAME {
                continue;
            }

            // Check if this is a transient texture
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

                log::trace!(
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

        // Process reads (shader resources)
        for read_name in &pass.reads {
            // Skip backbuffer - not read by shaders
            if read_name == BACKBUFFER_NAME {
                continue;
            }

            // Check if this is a transient texture
            let Some(transient) = self
                .graph
                .transient_texture(read_name, self.current_frame())
            else {
                continue;
            };

            log::trace!(
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

                log::trace!(
                    "[BARRIER] Pass '{}' transitioning '{}' from {:?} to {:?}",
                    pass.name,
                    read_name,
                    old_layout,
                    required_layout
                );

                // Transition using the actual tracked old_layout
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
    fn insert_post_pass_barriers(
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

            // Check if this is a transient texture
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

                log::trace!(
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

    /// Execute a graphics pass with dynamic rendering.
    fn execute_graphics_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        log::trace!(
            "🎨 [GRAPHICS] PASS '{}' with frame_idx={}, draw_lists={}, ui_draw_lists={}",
            pass.name,
            self.current_frame(),
            data.draw_lists.len(),
            data.ui_draw_lists.len()
        );

        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Determine color attachment:
        // 1. If pass writes to "backbuffer", use swapchain directly
        // 2. If pass writes to a transient texture, use that (frame-indexed)
        // 3. Use load_op from pass.color_attachments if available, otherwise default to CLEAR
        //    For backbuffer: use LOAD if a previous pass already wrote to it
        let color_attachment = if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
            // Explicit backbuffer write - use swapchain
            let swapchain_view =
                self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();

            // Check if a previous pass already wrote to the backbuffer
            let backbuffer_written = self.resource_states.contains_key(BACKBUFFER_NAME);
            let load_op = if backbuffer_written {
                log::trace!(
                    "✅ PASS '{}': Using LOAD for backbuffer (previous pass wrote to it)",
                    pass.name
                );
                vk::AttachmentLoadOp::LOAD
            } else {
                log::warn!(
                    "⚠️  PASS '{}': Using CLEAR for backbuffer (first write) - WILL OVERWRITE PREVIOUS CONTENT!",
                    pass.name
                );
                vk::AttachmentLoadOp::CLEAR
            };

            vk::RenderingAttachmentInfo::default()
                .image_view(swapchain_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(load_op)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.1, 0.1, 0.1, 1.0],
                    },
                })
        } else if let Some(color_name) = pass.writes.first() {
            // Check if this is a transient texture
            if let Some(transient) = self
                .graph
                .transient_texture(color_name, self.current_frame())
            {
                // Check if pass specified load/store ops for this attachment
                let (load_op, store_op, clear_value) = pass
                    .color_attachments
                    .iter()
                    .find(|(name, ..)| name == color_name)
                    .map(|(_, _, load_op, store_op, clear_value)| {
                        (
                            match load_op {
                                crate::render_pass::LoadOp::Load => vk::AttachmentLoadOp::LOAD,
                                crate::render_pass::LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
                                crate::render_pass::LoadOp::DontCare => {
                                    vk::AttachmentLoadOp::NONE_EXT
                                }
                            },
                            match store_op {
                                crate::render_pass::StoreOp::Store => vk::AttachmentStoreOp::STORE,
                                crate::render_pass::StoreOp::DontCare => {
                                    vk::AttachmentStoreOp::NONE_EXT
                                }
                            },
                            match clear_value {
                                crate::render_pass::ClearValue::Color(c) => {
                                    vk::ClearColorValue { float32: *c }
                                }
                                _ => vk::ClearColorValue {
                                    float32: [0.0, 0.0, 0.0, 1.0],
                                },
                            },
                        )
                    })
                    .unwrap_or((
                        vk::AttachmentLoadOp::CLEAR,
                        vk::AttachmentStoreOp::STORE,
                        vk::ClearColorValue {
                            float32: [0.1, 0.1, 0.1, 1.0],
                        },
                    ));

                vk::RenderingAttachmentInfo::default()
                    .image_view(transient.image_view.vk())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(load_op)
                    .store_op(store_op)
                    .clear_value(vk::ClearValue { color: clear_value })
            } else {
                return Err(RenderGraphError::ResourceNotFound(format!(
                    "Color target '{}' not found. Use 'backbuffer' for swapchain or create a transient resource.",
                    color_name
                )));
            }
        } else {
            return Err(RenderGraphError::InvalidConfiguration(
                "Pass has no color outputs. Use .write_color() for transient textures or declare output explicitly".to_string()
            ));
        };

        // Depth attachment (only for passes that use depth testing)
        // Use per-frame depth buffer to prevent data races when multiple frames
        // execute concurrently on the GPU (e.g., MAILBOX present mode).
        let depth_attachment = if pass.uses_depth {
            let frame_idx = self.current_frame();
            let depth_view = self
                .renderer
                .frame_context
                .depth_render_textures
                .get(frame_idx)
                .map(|t| t.image_view.vk())
                .expect("depth_render_textures must have an entry for current frame");

            let (load_op, store_op, clear_depth) = pass
                .depth_attachment
                .map(|(lo, so, cv)| {
                    let depth_val = match cv {
                        crate::render_pass::ClearValue::DepthStencil { depth, .. } => depth,
                        _ => 0.0,
                    };
                    (lo.into(), so.into(), depth_val)
                })
                .unwrap_or((
                    vk::AttachmentLoadOp::CLEAR,
                    vk::AttachmentStoreOp::STORE,
                    0.0,
                ));

            Some(
                vk::RenderingAttachmentInfo::default()
                    .image_view(depth_view)
                    .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                    .load_op(load_op)
                    .store_op(store_op)
                    .clear_value(vk::ClearValue {
                        depth_stencil: vk::ClearDepthStencilValue {
                            depth: clear_depth,
                            stencil: 0,
                        },
                    }),
            )
        } else {
            None
        };

        // Begin dynamic rendering
        cmd.begin_rendering(
            &[color_attachment],
            depth_attachment.as_ref(),
            None,
            render_area,
            1,
        );

        // Set viewport and scissor
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

        // Execute draw lists
        for draw_list in &data.draw_lists {
            self.execute_draw_list(cmd, draw_list)?;
        }

        // Execute UI draw lists
        for ui_draw_list in &data.ui_draw_lists {
            self.execute_ui_draw_list(cmd, pass, ui_draw_list)?;
        }

        // End rendering
        cmd.end_rendering();

        Ok(())
    }

    /// Render particles to a texture using the particle system.
    ///
    /// This starts a new render pass targeting the specified texture.
    fn render_particles_to_texture(
        &mut self,
        cmd: &CommandBuffer,
        texture: &TransientTexture,
    ) -> Result<(), RenderGraphError> {
        let _frame_idx = self.current_frame();
        use ash::vk;

        let particle_system = self.renderer.particle_system.as_ref().ok_or_else(|| {
            RenderGraphError::InvalidConfiguration("Particle system not initialized".to_string())
        })?;

        // Check if there are any particles to render
        let alive_count = particle_system.alive_count();
        if alive_count == 0 {
            return Ok(()); // No particles to render
        }

        // Transition hdr_color to COLOR_ATTACHMENT_OPTIMAL for particle rendering
        // (it may be in SHADER_READ_ONLY_OPTIMAL from the geometry post-barrier)
        let current_layout = texture.current_layout();
        if current_layout != vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL {
            let barrier = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_READ)
                .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(
                    vk::AccessFlags2::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                )
                .old_layout(current_layout)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .image(texture.image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            let dependency_info =
                vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));
            unsafe {
                self.renderer
                    .context
                    .device
                    .cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dependency_info);
            }
            texture.set_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        }

        // Create render pass begin info
        let color_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(texture.image_view.vk())
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD) // Load existing HDR output (sky + geometry)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.0; 4] },
            });

        // Depth attachment for depth testing against scene geometry
        let frame_idx = self.current_frame();
        let depth_view = self
            .renderer
            .frame_context
            .depth_render_textures
            .get(frame_idx)
            .map(|t| t.image_view.vk())
            .expect("depth_render_textures must have an entry for current frame");
        let depth_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(depth_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD) // Keep geometry depth
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        let rendering_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: texture.extent,
            })
            .layer_count(1)
            .color_attachments(std::slice::from_ref(&color_attachment))
            .depth_attachment(&depth_attachment);

        unsafe {
            self.renderer
                .context
                .device
                .cmd_begin_rendering(cmd.vk_command_buffer(), &rendering_info);
        }

        // Set viewport and scissor
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: texture.extent.width as f32,
            height: texture.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: texture.extent,
        };

        unsafe {
            self.renderer.context.device.cmd_set_viewport(
                cmd.vk_command_buffer(),
                0,
                std::slice::from_ref(&viewport),
            );
            self.renderer.context.device.cmd_set_scissor(
                cmd.vk_command_buffer(),
                0,
                std::slice::from_ref(&scissor),
            );
        }

        // Render particles using the particle system
        // Get storage descriptor set first to avoid borrow conflicts
        let storage_descriptor_set = if self.renderer.particle_system.is_some() {
            Some(self.renderer.storage_descriptor_sets[self.renderer.current_frame()].vk_set())
        } else {
            None
        };

        // Get current frame index before mutable borrow
        let current_frame = self.current_frame();

        if let Some(ref mut particle_system) = self.renderer.particle_system {
            if let Some(pipeline_handle) = particle_system.render_pipeline_handle() {
                // Update render descriptor set to point to the correct frame's alive list
                // (the one simulate just wrote survivors to)
                if let Err(e) = particle_system.update_render_descriptor_binding(current_frame) {
                    log::warn!("Failed to update particle render descriptor binding: {}", e);
                }

                // Get the pipeline from the registry
                let pipeline_asset = self
                    .renderer
                    .asset_registry
                    .get_pipeline(pipeline_handle)
                    .ok_or_else(|| {
                        RenderGraphError::InvalidConfiguration(format!(
                            "Particle pipeline {:?} not found in registry",
                            pipeline_handle
                        ))
                    })?;

                let vk_pipeline = pipeline_asset.vk_pipeline();
                let vk_layout = pipeline_asset.vk_layout();

                // Get the storage descriptor set (Set 1) from renderer for FrameUniforms
                let storage_ds = storage_descriptor_set.ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(
                        "Storage descriptor set not available".to_string(),
                    )
                })?;

                // Call particle system render method
                particle_system
                    .render(
                        cmd.vk_command_buffer(),
                        vk::RenderPass::null(), // Using dynamic rendering, not needed
                        vk_pipeline,
                        vk_layout,
                        storage_ds,
                        current_frame,
                    )
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!("Particle render failed: {}", e))
                    })?;

                log::trace!("Drew {} particles successfully", alive_count);
            } else {
                log::warn!("Particle render pipeline not created, skipping particle rendering");
            }
        } else {
            log::warn!("Particle system not available, skipping particle rendering");
        }

        // End render pass
        unsafe {
            self.renderer
                .context
                .device
                .cmd_end_rendering(cmd.vk_command_buffer());
        }

        // Transition texture back to shader read-only for UI sampling
        let old_layout = texture.current_layout();
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
            .old_layout(old_layout)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(texture.image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let dependency_info =
            vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));

        unsafe {
            self.renderer
                .context
                .device
                .cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dependency_info);
        }

        texture.set_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        Ok(())
    }
    /// Execute a draw list.
    fn execute_draw_list(
        &mut self,
        cmd: &CommandBuffer,
        draw_list: &DrawList,
    ) -> Result<(), RenderGraphError> {
        log::trace!(
            "execute_draw_list: {} draw calls to execute",
            draw_list.draws.len()
        );

        // Execute regular draw calls
        for draw_call in &draw_list.draws {
            log::trace!(
                "Executing draw call: mesh={:?}, material={:?}",
                draw_call.mesh,
                draw_call.material
            );
            self.execute_draw_call(cmd, draw_call)?;
        }

        log::trace!(
            "execute_draw_list: completed {} draw calls",
            draw_list.draws.len()
        );

        Ok(())
    }

    /// Execute a UI draw list.
    fn execute_ui_draw_list(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        ui_draw_list: &UIDrawList,
    ) -> Result<(), RenderGraphError> {
        // Early exit if empty
        if ui_draw_list.is_empty() {
            return Ok(());
        }

        // Get the UI material from the pass
        let material_handle = pass.material.ok_or(RenderGraphError::InvalidConfiguration(
            "UI pass has no material specified. Use .material() on UIPass.".to_string(),
        ))?;

        // Get material asset from registry
        let material = self
            .renderer
            .asset_registry
            .get_material(material_handle)
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;

        // Get pipeline handle from material
        let pipeline_handle = material
            .pipeline
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;
        let (pipeline, pipeline_layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Get or create per-frame UI buffers and upload data
        let frame_idx = self.renderer.current_frame();
        let (vertex_buffer, index_buffer) =
            self.get_or_update_ui_buffers(frame_idx, ui_draw_list)?;

        // Bind vertex and index buffers
        cmd.bind_vertex_buffer(vertex_buffer.0, 0);
        cmd.bind_index_buffer(index_buffer, 0, vk::IndexType::UINT32);

        // Get swapchain extent for scissor (physical pixels)
        let extent = self.renderer.frame_context.swapchain.get_extent();

        // Check font atlas is available
        if self.renderer.ui_renderer.font_atlas_handle().is_none() {
            return Err(RenderGraphError::InvalidConfiguration(
                "UI font atlas not initialized".to_string(),
            ));
        }

        // Bind UI descriptor sets (sampler, uniforms, bindless textures)
        // Use screen_size from draw list (logical pixels, matches vertex coordinates)
        // Bind set 0 once (sampler, uniforms don't change per frame)
        // Bind set 1 once (bindless texture array, shared with 3D materials)
        self.bind_ui_descriptor_sets(
            cmd,
            pipeline_handle,
            pipeline_layout,
            ui_draw_list.screen_size,
        )?;

        // Execute each draw command with scissor clipping
        for draw_cmd in &ui_draw_list.commands {
            // Set scissor for clipping (if specified)
            // clip_rect is in logical pixels, convert to physical pixels for Vulkan scissor
            if let Some([x, y, width, height]) = draw_cmd.clip_rect {
                let scale = ui_draw_list.scale_factor;
                let scissor = crate::sync::Rect2D::new(
                    (x * scale).max(0.0) as i32,
                    (y * scale).max(0.0) as i32,
                    (width * scale).max(0.0) as u32,
                    (height * scale).max(0.0) as u32,
                );
                cmd.set_scissor(&[scissor]);
            } else {
                // No clipping - reset to full screen
                cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
                    extent.width,
                    extent.height,
                )]);
            }

            // Draw indexed
            unsafe {
                self.renderer.context.device.cmd_draw_indexed(
                    cmd.vk_command_buffer(),
                    draw_cmd.index_count,
                    1,
                    draw_cmd.index_offset,
                    0,
                    0,
                );
            }
        }

        // Reset scissor to full screen for next pass
        cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
            extent.width,
            extent.height,
        )]);

        Ok(())
    }

    /// Update per-frame UI vertex and index buffers with new data.
    ///
    /// This reuses buffers across frames to avoid memory leaks. Buffers are resized
    /// if needed to accommodate larger data.
    fn get_or_update_ui_buffers(
        &mut self,
        frame_idx: usize,
        ui_draw_list: &UIDrawList,
    ) -> Result<((vk::Buffer, u32), vk::Buffer), RenderGraphError> {
        let vertex_bytes = bytemuck::cast_slice(&ui_draw_list.vertices);
        let index_bytes = bytemuck::cast_slice(&ui_draw_list.indices);

        // Access UI resources through UIRenderer
        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

        // Update vertex buffer
        let vb = &mut ui_resources.vertex_buffers[frame_idx];
        vb.upload_data(vertex_bytes);
        let vb_handle = (vb.object(), vb.count());

        // Update index buffer
        let ib = &mut ui_resources.index_buffers[frame_idx];
        ib.upload_data(index_bytes);
        let ib_handle = ib.object();

        Ok((vb_handle, ib_handle))
    }

    /// Bind UI descriptor sets (Set 0: font atlas, sampler, uniforms).
    fn bind_ui_descriptor_sets(
        &mut self,
        cmd: &CommandBuffer,
        pipeline_handle: PipelineHandle,
        pipeline_layout: vk::PipelineLayout,
        screen_size: [f32; 2],
    ) -> Result<(), RenderGraphError> {
        // Get the pipeline to access its descriptor set layouts (separate borrow to avoid conflicts)
        let descriptor_set_layout = {
            let pipeline = self
                .renderer
                .asset_registry
                .get_pipeline(pipeline_handle)
                .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

            let descriptor_set_layouts = pipeline.descriptor_set_layouts();
            if descriptor_set_layouts.is_empty() {
                return Err(RenderGraphError::InvalidConfiguration(
                    "UI pipeline has no descriptor set layouts".to_string(),
                ));
            }

            descriptor_set_layouts[0]
        };

        // Now we can mutate the renderer state
        let frame_idx = self.renderer.current_frame();
        let descriptor_set =
            self.get_or_create_ui_descriptor_set(frame_idx, descriptor_set_layout, screen_size)?;

        // Bind descriptor set 0 (sampler, uniforms)
        cmd.bind_descriptor_sets(pipeline_layout, 0, &[descriptor_set], &[]);

        // Bind descriptor set 1 (bindless texture array - shared with 3D materials)
        let bindless_descriptor_set = self.renderer.bindless_manager.descriptor_set();
        cmd.bind_descriptor_sets(pipeline_layout, 1, &[bindless_descriptor_set.vk()], &[]);

        Ok(())
    }

    /// Get or create per-frame UI descriptor set.
    fn get_or_create_ui_descriptor_set(
        &mut self,
        frame_idx: usize,
        layout: vk::DescriptorSetLayout,
        screen_size: [f32; 2],
    ) -> Result<vk::DescriptorSet, RenderGraphError> {
        // Check if we already have a descriptor set for this frame with the same layout
        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

        // Ensure we have storage for this frame
        while ui_resources.descriptor_sets.len() <= frame_idx {
            ui_resources.descriptor_sets.push(None);
        }

        // Check if we already have a descriptor set for this frame
        let descriptor_set_handle = ui_resources.descriptor_sets[frame_idx]
            .as_ref()
            .map(|ds| ds.vk());

        let _ = ui_resources; // Release borrow before calling update

        if let Some(ds_handle) = descriptor_set_handle {
            // Update uniform buffer with new screen size
            self.update_ui_descriptor_set(ds_handle, screen_size)?;
            return Ok(ds_handle);
        }

        // Create new descriptor set pool and descriptor set
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1),
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        let descriptor_pool = unsafe {
            self.renderer
                .context
                .device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Failed to create UI descriptor pool: {:?}",
                        e
                    ))
                })?
        };

        let layouts = [layout];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe {
            self.renderer
                .context
                .device
                .allocate_descriptor_sets(&allocate_info)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Failed to allocate UI descriptor set: {:?}",
                        e
                    ))
                })?
        };

        let descriptor_set = descriptor_sets[0];

        // Wrap in DescriptorSet for automatic cleanup (owns pool and layout)
        let descriptor_set_wrapper = crate::vulkan::descriptor_set::DescriptorSet::from_raw(
            descriptor_set,
            descriptor_pool,
            None, // Layout is owned by Pipeline, not by the descriptor set
            self.renderer.context.device.clone(),
        );

        // Store descriptor set (owns pool, automatic cleanup)
        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();
        if frame_idx < ui_resources.descriptor_sets.len() {
            ui_resources.descriptor_sets[frame_idx] = Some(descriptor_set_wrapper);
        }
        let _ = ui_resources;

        // Update descriptor set with resources
        self.update_ui_descriptor_set(descriptor_set, screen_size)?;

        Ok(descriptor_set)
    }

    /// Update UI descriptor set with sampler and uniforms.
    fn update_ui_descriptor_set(
        &mut self,
        descriptor_set: vk::DescriptorSet,
        screen_size: [f32; 2],
    ) -> Result<(), RenderGraphError> {
        // Get shared sampler from bindless manager
        let sampler = self.renderer.bindless_manager.shared_sampler();

        // Create or update uniform buffer for screen size
        let uniform_data = [screen_size[0], screen_size[1], 0.0, 0.0];
        let uniform_bytes = bytemuck::cast_slice(&uniform_data);

        // Access UI resources through RefCell
        let uniform_buffer = {
            let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

            // Create or reuse uniform buffer
            if ui_resources.uniform_buffer.is_none() {
                let uniform_buffer_info = vk::BufferCreateInfo::default()
                    .size(uniform_bytes.len() as vk::DeviceSize)
                    .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE);

                let (uniform_buffer, uniform_allocation) = self.renderer.context.allocate_buffer(
                    &uniform_buffer_info,
                    gpu_allocator::MemoryLocation::CpuToGpu,
                );
                ui_resources.uniform_buffer = Some((uniform_buffer, uniform_allocation));
            }

            // Get uniform buffer handle (vk::Buffer is Copy)
            ui_resources.uniform_buffer.as_ref().unwrap().0
        };

        // Now get the allocation for mapping
        let uniform_ptr = {
            let allocation = &self
                .renderer
                .ui_renderer
                .ui_resources_mut()
                .uniform_buffer
                .as_ref()
                .unwrap()
                .1;
            self.renderer.context.map_buffer(allocation)
        };

        // Update uniform data
        unsafe {
            std::ptr::copy_nonoverlapping(uniform_bytes.as_ptr(), uniform_ptr, uniform_bytes.len());
        }

        // Prepare descriptor writes
        let buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(uniform_buffer)
            .offset(0)
            .range(uniform_bytes.len() as vk::DeviceSize);

        let image_info = vk::DescriptorImageInfo::default()
            .sampler(sampler.vk())
            .image_view(vk::ImageView::null()) // Null for sampler-only write
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        let writes = [
            // Binding 1: sampler
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .image_info(std::slice::from_ref(&image_info)),
            // Binding 3: screen size uniform
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .buffer_info(std::slice::from_ref(&buffer_info)),
        ];

        unsafe {
            self.renderer
                .context
                .device
                .update_descriptor_sets(&writes, &[]);
        }

        Ok(())
    }

    /// Execute a single draw call.
    fn execute_draw_call(
        &mut self,
        cmd: &CommandBuffer,
        draw_call: &DrawCall,
    ) -> Result<(), RenderGraphError> {
        // Extract mesh and material info upfront to avoid borrow conflicts
        let mesh_handle = draw_call.mesh;
        let material_handle = draw_call.material;

        let (needs_recompile, material_format) = {
            let material = self
                .renderer
                .asset_registry
                .get_material(material_handle)
                .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;
            (!material.fully_compiled, material.color_format)
        };

        // Recompile material if invalidated (e.g., after descriptor layout change during resize)
        if needs_recompile {
            self.renderer
                .ensure_material_compiled(material_handle, material_format)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Material recompilation failed: {}",
                        e
                    ))
                })?;
        }

        // Get mesh from registry
        let mesh = self
            .renderer
            .asset_registry
            .get_mesh(mesh_handle)
            .ok_or(RenderGraphError::InvalidMeshHandle(mesh_handle))?;

        // Get material from registry (may have been recompiled above)
        let material = self
            .renderer
            .asset_registry
            .get_material(material_handle)
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;

        // Clone pipeline_handle to avoid holding borrow across bind_descriptor_sets
        let pipeline_handle = material
            .pipeline
            .ok_or(RenderGraphError::InvalidMaterialHandle(draw_call.material))?;

        // Get pipeline handles from registry
        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Bind vertex buffers (SOA: position(0), normal(1), tangent(2), uv(3))
        let pos_buf = mesh
            .get_attribute_buffer(AttributeType::Position)
            .map(|vb| vb.object())
            .unwrap_or(vk::Buffer::null());
        let norm_buf = mesh
            .get_attribute_buffer(AttributeType::Normal)
            .map(|vb| vb.object())
            .unwrap_or(vk::Buffer::null());
        let tang_buf = mesh
            .get_attribute_buffer(AttributeType::Tangent)
            .map(|vb| vb.object())
            .unwrap_or(vk::Buffer::null());
        let uv_buf = mesh
            .get_attribute_buffer(AttributeType::TexCoord0)
            .map(|vb| vb.object())
            .unwrap_or(vk::Buffer::null());
        cmd.bind_vertex_buffers_at_locations(&[
            (0, pos_buf),
            (1, norm_buf),
            (2, tang_buf),
            (3, uv_buf),
        ]);

        // Bind index buffer
        if let Some(ib) = &mesh.index_buffer {
            cmd.bind_index_buffer(ib.object(), 0, vk::IndexType::UINT32);
        }

        // Extract needed data before borrows end
        let index_count = mesh.index_buffer.as_ref().map(|ib| ib.count()).unwrap_or(0);

        // Material borrow ends here, allowing &mut self call below
        let _ = material;

        // Bind descriptor sets
        self.bind_descriptor_sets(cmd, layout, draw_call)?;

        // Draw indexed (instance_index is used instead of push constants)
        cmd.draw_indexed(index_count, 1, 0, 0, draw_call.instance_index);

        Ok(())
    }

    /// Bind descriptor sets for a draw call.
    ///
    /// Descriptor set layout:
    /// - Set 0: Storage uniforms (frame_data + objects array) - always bound
    /// - Set 1: Bindless textures - always bound for current materials
    /// - Set 2: Skeleton joint matrices - bound only for skinned mesh draws
    /// - Set 3: Light culling data - bound when light culling is active
    fn bind_descriptor_sets(
        &mut self,
        cmd: &CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        draw_call: &DrawCall,
    ) -> Result<(), RenderGraphError> {
        // Set 0: Storage uniforms (frame_data + objects array) - use per-frame descriptor set
        let storage_ds =
            self.renderer.storage_descriptor_sets[self.renderer.current_frame()].vk_set();
        cmd.bind_descriptor_sets(pipeline_layout, 0, &[storage_ds], &[]);

        // Set 1: Bindless textures (all current materials use bindless)
        let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(pipeline_layout, 1, &[bindless_ds], &[]);

        // Set 2: Skeleton joint matrices (only when draw_call has skeleton)
        if !draw_call.skeleton.is_none() {
            let skeleton_ds = self
                .renderer
                .get_skeleton_descriptor(draw_call.skeleton)
                .ok_or(RenderGraphError::InvalidSkeletonHandle(draw_call.skeleton))?;
            cmd.bind_descriptor_sets(pipeline_layout, 2, &[skeleton_ds.vk_set()], &[]);
        }

        // Set 3: Forward+ light culling data (push descriptors when active)
        if let Some(lc) = self.renderer.light_culling_buffers()
            && let Err(e) = lc.push_fragment_descriptors(cmd.vk_command_buffer(), pipeline_layout)
        {
            log::warn!("Failed to push light culling fragment descriptors: {}", e);
        }

        // Set 4: Shadow data (regular descriptor set when shadow system is active)
        self.renderer
            .bind_shadow_descriptors(cmd.vk_command_buffer(), pipeline_layout);

        Ok(())
    }

    /// Execute a fullscreen pass (draws a fullscreen triangle).
    fn execute_fullscreen_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        pipeline_handle: crate::handle::PipelineHandle,
    ) -> Result<(), RenderGraphError> {
        let current_frame = self.current_frame();
        log::trace!(
            "[FULLSCREEN] Pass '{}' execution: frame_idx={}, writes={:?}, reads={:?}",
            pass.name,
            current_frame,
            pass.writes,
            pass.reads
        );

        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Determine color attachment:
        // 1. If pass writes to "backbuffer", use swapchain directly
        // 2. If pass writes to a transient texture, use that (frame-indexed)
        // 3. Use load_op from pass.color_attachments if available, otherwise default to CLEAR
        let color_attachment = if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
            // Explicit backbuffer write - use swapchain
            let swapchain_view =
                self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();
            vk::RenderingAttachmentInfo::default()
                .image_view(swapchain_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.1, 0.1, 0.1, 1.0],
                    },
                })
        } else if let Some(color_name) = pass.writes.first() {
            // Check if this is a transient texture (fullscreen pass like tonemap)
            let frame_idx = self.current_frame();
            if let Some(transient) = self.graph.transient_texture(color_name, frame_idx) {
                log::trace!(
                    "[FULLSCREEN] Pass '{}' writing to transient texture '{}' at frame_idx={}, format={:?}, extent={}x{}",
                    pass.name,
                    color_name,
                    frame_idx,
                    transient.format,
                    transient.extent.width,
                    transient.extent.height
                );

                // Check if pass specified load/store ops for this attachment
                let (load_op, store_op, clear_value) = pass
                    .color_attachments
                    .iter()
                    .find(|(name, ..)| name == color_name)
                    .map(|(_, _, load_op, store_op, clear_value)| {
                        (
                            match load_op {
                                crate::render_pass::LoadOp::Load => vk::AttachmentLoadOp::LOAD,
                                crate::render_pass::LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
                                crate::render_pass::LoadOp::DontCare => {
                                    vk::AttachmentLoadOp::NONE_EXT
                                }
                            },
                            match store_op {
                                crate::render_pass::StoreOp::Store => vk::AttachmentStoreOp::STORE,
                                crate::render_pass::StoreOp::DontCare => {
                                    vk::AttachmentStoreOp::NONE_EXT
                                }
                            },
                            match clear_value {
                                crate::render_pass::ClearValue::Color(c) => {
                                    vk::ClearColorValue { float32: *c }
                                }
                                _ => vk::ClearColorValue {
                                    float32: [0.0, 0.0, 0.0, 1.0],
                                },
                            },
                        )
                    })
                    .unwrap_or((
                        vk::AttachmentLoadOp::CLEAR,
                        vk::AttachmentStoreOp::STORE,
                        vk::ClearColorValue {
                            float32: [0.1, 0.1, 0.1, 1.0],
                        },
                    ));

                vk::RenderingAttachmentInfo::default()
                    .image_view(transient.image_view.vk())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(load_op)
                    .store_op(store_op)
                    .clear_value(vk::ClearValue { color: clear_value })
            } else {
                return Err(RenderGraphError::ResourceNotFound(format!(
                    "Color target '{}' not found. Use 'backbuffer' for swapchain or create a transient resource.",
                    color_name
                )));
            }
        } else {
            return Err(RenderGraphError::InvalidConfiguration(
                "Pass has no color outputs. Use 'backbuffer' for swapchain or create a transient resource.".to_string()
            ));
        };

        // Begin dynamic rendering
        cmd.begin_rendering(
            &[color_attachment],
            None, // No depth attachment for fullscreen passes
            None,
            render_area,
            1,
        );

        // Set viewport and scissor
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

        // Get pipeline from registry
        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Bind descriptor sets (storage uniforms + bindless textures) - use per-frame descriptor set
        let storage_ds =
            self.renderer.storage_descriptor_sets[self.renderer.current_frame()].vk_set();
        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

        let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(layout, 1, &[bindless_ds], &[]);

        // Skip fullscreen draw for tonemap passes with no HDR input (e.g., background clear pass).
        // The render pass clear color already provides the desired output.
        // Non-tonemap fullscreen passes (e.g., sky) always draw.
        let skip_draw = pass
            .tonemap_params
            .as_ref()
            .is_some_and(|p| p.hdr_texture_index.is_none());

        if !skip_draw {
            cmd.draw_array(3, 1, 0, 0);
        }

        // End rendering
        cmd.end_rendering();

        Ok(())
    }

    /// Execute a compute pass (GPU compute work).
    ///
    /// Compute passes perform general-purpose GPU computation without rendering to attachments.
    /// Used for particle simulation, physics, and other compute-intensive tasks.
    ///
    /// # Compute-Specific Behavior
    ///
    /// 1. **Bind compute pipeline**: Set pipeline for compute work
    /// 2. **Bind descriptor sets**: Set 0 (static buffers) + Set 1 (push descriptors if needed)
    /// 3. **Dispatch compute shader**: Execute with specified workgroup count
    fn execute_compute_pass(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        pipeline_handle: crate::handle::PipelineHandle,
    ) -> Result<(), RenderGraphError> {
        let current_frame = self.current_frame();
        log::trace!(
            "[COMPUTE] Pass '{}' execution: frame_idx={}, pipeline={:?}",
            pass.name,
            current_frame,
            pipeline_handle
        );

        let device = &self.renderer.context.device;

        // Get compute pipeline from registry
        let compute_pipeline = self
            .renderer
            .asset_registry
            .get_pipeline(pipeline_handle)
            .ok_or_else(|| {
                RenderGraphError::PipelineNotSet(format!(
                    "Pipeline {:?} not found",
                    pipeline_handle
                ))
            })?;

        let vk_pipeline = compute_pipeline.vk_pipeline();

        // Bind compute pipeline
        unsafe {
            device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::COMPUTE,
                vk_pipeline,
            );
        }

        // Get current frame index before any mutable borrows
        let current_frame = self.current_frame();

        // Bind descriptor sets if particle system is active
        // Note: Particle system manages its own descriptor sets
        if let Some(ref mut particle_system) = self.renderer.particle_system
            && pass.name.contains("particle")
        {
            log::trace!("Executing particle compute pass '{}'", pass.name);

            // Use pre-calculated workgroup count from frame graph
            // These were calculated in renderer.rs based on current particle state
            let workgroup_count = if pass.name.contains("emit") {
                self.graph.particle_emit_workgroup_count
            } else if pass.name.contains("simulate") {
                self.graph.particle_simulate_workgroup_count
            } else {
                log::warn!(
                    "Unknown particle compute pass '{}', using default workgroup count",
                    pass.name
                );
                1
            };

            // Before recording dispatch
            if workgroup_count == 0 {
                log::debug!(
                    "Skipping particle compute pass '{}' - workgroup_count is 0",
                    pass.name
                );
                return Ok(()); // Skip dispatch
            }

            // Record the appropriate dispatch based on pass name
            if pass.name.contains("emit") {
                // Update compute descriptor bindings for EMIT pass
                // CRITICAL: Emit needs binding 2 to point to alive[frame_index]
                // so that newly emitted particles are appended where simulate will read them
                particle_system
                    .update_compute_descriptor_binding(current_frame)
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Failed to update particle compute descriptor binding: {}",
                            e
                        ))
                    })?;
                particle_system
                    .record_emit_dispatch(
                        cmd.vk_command_buffer(),
                        &self.renderer.asset_registry,
                        workgroup_count,
                        current_frame,
                    )
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Particle emit dispatch failed: {}",
                            e
                        ))
                    })?;
                // Mark that emit ran this frame so simulate knows not to
                // overwrite emit_count.
                let graph_ptr = self.graph as *const FrameGraph as *mut FrameGraph;
                unsafe {
                    (*graph_ptr).particle_emit_ran = true;
                }
            } else if pass.name.contains("simulate") {
                // Update compute descriptor bindings for SIMULATE pass
                // CRITICAL: Simulate needs binding 3 to point to alive[(frame+1)%2]
                // so that survivors are written to the region render will read from
                particle_system
                    .update_compute_descriptor_binding(current_frame)
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Failed to update particle compute descriptor binding: {}",
                            e
                        ))
                    })?;

                // Reset counters before simulate.
                // When emit was skipped, alive_count and emit_count must be reset
                // here since emit didn't do it.
                let emit_ran = self.graph.particle_emit_ran;
                particle_system.reset_simulate_counters(
                    cmd.vk_command_buffer(),
                    emit_ran,
                    current_frame,
                );

                particle_system
                    .record_simulate_dispatch(
                        cmd.vk_command_buffer(),
                        &self.renderer.asset_registry,
                        workgroup_count,
                        current_frame,
                    )
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Particle simulate dispatch failed: {}",
                            e
                        ))
                    })?;

                // No swap needed — simulate writes survivors to alive[(frame+1)%2] via
                // descriptor offset flip in update_compute_descriptor_binding. The render
                // pass reads from the same region via update_render_descriptor_binding.

                // Record particle debug readback if requested this frame
                // SAFETY: We need to access the graph's debug readback flag through the Frame's graph reference
                // This is safe because we're in the middle of frame execution and have exclusive access
                let graph_ptr = self.graph as *const FrameGraph as *mut FrameGraph;
                unsafe {
                    if (*graph_ptr).particle_debug_readback {
                        log::info!("Recording particle debug readback after simulate pass");
                        particle_system
                            .record_debug_readback(cmd.vk_command_buffer(), current_frame)
                            .map_err(|e| {
                                RenderGraphError::VulkanError(format!(
                                    "Particle debug readback failed: {}",
                                    e
                                ))
                            })?;
                        // Reset flag after recording
                        (*graph_ptr).particle_debug_readback = false;
                    }
                }
            }

            return Ok(());
        }

        // Generic compute dispatch for non-particle compute passes
        // TODO: Calculate workgroup count based on work size
        unsafe {
            device.cmd_dispatch(cmd.vk_command_buffer(), 64, 1, 1);
        }

        log::trace!("Compute pass '{}' executed successfully", pass.name);
        Ok(())
    }

    /// Execute a shadow pass.
    ///
    /// Phase 1: Clears the shadow atlas depth to 1.0 (far plane).
    /// Future phases will render actual shadow depth from the light's perspective.
    fn execute_shadow_pass(
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

        // Find the shadow atlas depth texture
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

        // Begin rendering with depth-only attachment
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

        // Get shadow pipeline from renderer
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
            // Set single viewport for this cascade
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

            // Set shadow params for this cascade (cascade_index + bias)
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

        // Mark the shadow atlas as written for barrier tracking
        if let Some(write_name) = pass.writes.first() {
            self.resource_states
                .insert(write_name.clone(), ResourceState::DepthStencilAttachment);
        }

        Ok(())
    }

    /// Execute a depth prepass — renders only depth from the camera's perspective.
    ///
    /// The depth buffer is reused by the subsequent geometry pass via `LoadOp::Load`.
    /// This enables early-Z rejection and reduces overdraw in the PBR pass.
    fn execute_depth_prepass(
        &mut self,
        cmd: &CommandBuffer,
        _pass: &PassDesc,
        data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        let frame_idx = self.current_frame();
        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        log::trace!(
            "[DEPTH_PREPASS] frame_idx={}, draw_lists={}",
            frame_idx,
            data.draw_lists.len()
        );

        // No color attachment — depth only
        let depth_view = self
            .renderer
            .frame_context
            .depth_render_textures
            .get(frame_idx)
            .map(|t| t.image_view.vk())
            .expect("depth_render_textures must have an entry for current frame");

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

        cmd.begin_rendering(&[], Some(&depth_attachment), None, render_area, 1);

        let depth_pipeline_handle = self.renderer.depth_prepass_pipeline().ok_or(
            RenderGraphError::InvalidConfiguration(
                "Depth prepass pipeline not initialized".to_string(),
            ),
        )?;

        let (pipeline, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_vk_handles(depth_pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(
                depth_pipeline_handle,
            ))?;

        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

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

        // Draw all geometry (depth only — same draw lists as geometry pass)
        let skinned_pipeline_handle = self.renderer.depth_prepass_skinned_pipeline();

        let (skinned_pipeline, skinned_layout) = if let Some(handle) = skinned_pipeline_handle {
            self.renderer
                .asset_registry
                .get_pipeline_vk_handles(handle)
                .map(|(p, l)| (Some(p), Some(l)))
                .unwrap_or((None, None))
        } else {
            (None, None)
        };

        let mut current_pipeline_is_skinned = false;

        for draw_list in &data.draw_lists {
            for draw_call in draw_list.iter() {
                let is_skinned = !draw_call.skeleton.is_none();

                // Skip skinned meshes if the skinned pipeline is not available
                if is_skinned && skinned_pipeline.is_none() {
                    continue;
                }

                // Bind the correct pipeline when switching between skinned and non-skinned
                if is_skinned != current_pipeline_is_skinned {
                    if is_skinned {
                        unsafe {
                            self.renderer.context.device.cmd_bind_pipeline(
                                cmd.vk_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                skinned_pipeline.unwrap(),
                            );
                        }

                        let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
                        cmd.bind_descriptor_sets(skinned_layout.unwrap(), 0, &[storage_ds], &[]);
                    } else {
                        unsafe {
                            self.renderer.context.device.cmd_bind_pipeline(
                                cmd.vk_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline,
                            );
                        }

                        let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
                        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);
                    }
                    current_pipeline_is_skinned = is_skinned;
                }

                // Bind skeleton descriptor set for skinned meshes (Set 2)
                if is_skinned {
                    let skeleton_ds = self
                        .renderer
                        .get_skeleton_descriptor(draw_call.skeleton)
                        .ok_or(RenderGraphError::InvalidSkeletonHandle(draw_call.skeleton))?;
                    cmd.bind_descriptor_sets(
                        skinned_layout.unwrap(),
                        2,
                        &[skeleton_ds.vk_set()],
                        &[],
                    );
                }

                let mesh = self
                    .renderer
                    .asset_registry
                    .get_mesh(draw_call.mesh)
                    .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?;

                // Depth prepass: bind SOA attribute buffers based on mesh type
                let pos_buf = mesh
                    .get_attribute_buffer(AttributeType::Position)
                    .map(|vb| vb.object())
                    .unwrap_or(vk::Buffer::null());
                if is_skinned {
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
                } else {
                    cmd.bind_vertex_buffers_at_locations(&[(0, pos_buf)]);
                }

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

        cmd.end_rendering();

        Ok(())
    }

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
    fn execute_compositing_pass(
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

        log::trace!(
            "[COMPOSITING] Pass '{}' execution: frame_idx={}, viewport_count={}, writes={:?}",
            pass.name,
            current_frame,
            viewports.len(),
            pass.writes
        );

        let extent = self.renderer.frame_context.swapchain.get_extent();

        // Update compositing uniforms (viewport rectangles and screen size)
        // We use objects[0] for fullscreen/post-processing passes (similar to tonemap)
        let viewport_count = viewports.len() as u32;
        let screen_size = [extent.width as f32, extent.height as f32];

        // Get viewport texture bindless index
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

        // Encode viewport count, screen size, and bindless index in objects[0]
        // base_color.rg = screen_size (width, height)
        // base_color.a = viewport bindless texture index
        // material_params.x = viewport count
        self.renderer.storage_manager.update_object_bindless(
            current_frame,
            0,          // Slot 0 for fullscreen passes
            &[0.0; 16], // Identity matrix (unused)
            &[
                screen_size[0],               // base_color.r = screen width
                screen_size[1],               // base_color.g = screen height
                0.0,                          // base_color.b = unused
                viewport_bindless_idx as f32, // base_color.a = viewport bindless index
            ],
            viewport_count as f32, // material_params.x = viewport count
            0.0,                   // material_params.y = unused
            0.0,                   // material_params.z = unused
            0.0,                   // material_params.w = unused
            [0, 0, 0, 0],          // texture_indices = unused
        );

        // TODO: Pass viewport rectangles via proper uniform buffer
        // For now, the shader uses a simple hardcoded split-screen layout
        // This will be enhanced in a follow-up to support arbitrary viewport rectangles

        // Create or update compositing descriptor set with viewport textures
        let compositing_desc_set =
            self.get_or_create_compositing_descriptor_set(viewports, current_frame)?;

        // Get swapchain extent for rendering
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Determine color attachment (backbuffer or transient texture)
        let color_attachment = if pass.writes.contains(&BACKBUFFER_NAME.to_string()) {
            // Write to backbuffer
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
            // Write to transient texture
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

        // Begin dynamic rendering
        cmd.begin_rendering(
            &[color_attachment],
            None, // No depth attachment for compositing
            None,
            render_area,
            1,
        );

        // Set viewport and scissor
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

        // Get material and pipeline from registry
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
            .get_pipeline_vk_handles(pipeline_handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Bind descriptor sets
        // Set 0: Storage uniforms (frame_data + objects array)
        let storage_ds = self.renderer.storage_descriptor_sets[current_frame].vk_set();
        cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

        // Set 1: Bindless textures (shared with all materials)
        let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(layout, 1, &[bindless_ds], &[]);

        // Set 2: Compositing descriptor set (viewport texture array)
        cmd.bind_descriptor_sets(layout, 2, &[compositing_desc_set], &[]);

        // Draw fullscreen triangle (3 vertices)
        cmd.draw_array(3, 1, 0, 0);

        // End rendering
        cmd.end_rendering();

        Ok(())
    }

    /// Get or create compositing descriptor set for current frame.
    ///
    /// Creates or updates a descriptor set with the viewport texture array.
    /// The descriptor set is cached per-frame and updated when viewport textures change.
    fn get_or_create_compositing_descriptor_set(
        &mut self,
        viewports: &[(GraphResourceHandle, ViewportRect)],
        frame_idx: usize,
    ) -> Result<vk::DescriptorSet, RenderGraphError> {
        use crate::render_graph::descriptor_sets::CompositingDescriptorSet;
        use std::rc::Rc;

        // Collect viewport texture image views
        let mut texture_views = Vec::with_capacity(viewports.len());
        for (handle, _rect) in viewports {
            // Find the texture resource name from the handle
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

            log::trace!(
                "[COMPOSITING] Looking up viewport texture: '{}' (handle={})",
                resource_name,
                handle.index()
            );

            // Get the transient texture
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

            log::trace!(
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

impl<'a> Drop for Frame<'a> {
    fn drop(&mut self) {
        // Clean up temporary buffers created during this frame
        for (buffer, allocation) in self.temporary_buffers.drain(..) {
            unsafe {
                self.renderer.context.device.destroy_buffer(buffer, None);
            }
            self.renderer
                .context
                .allocator
                .borrow_mut()
                .free(allocation)
                .ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PassExecutionData;

    #[test]
    fn test_pass_execution_data_default() {
        let data = PassExecutionData::default();
        assert!(data.draw_lists.is_empty());
        assert!(data.dispatch.is_none());
        assert!(data.uniform_data.is_empty());
    }
}
