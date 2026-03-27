mod barriers;
mod compositing;
mod depth_prepass;
mod draw_calls;
mod graphics_pass;
mod outline_pass;
mod particle_rendering;
mod shadow_pass;
mod ui_rendering;

use std::collections::HashMap;

use super::error::RenderGraphError;
use super::frame_graph::{BACKBUFFER_NAME, FrameGraph};
use super::pass::PassDesc;
use super::resource::ResourceState;
use crate::renderer::VulkanRenderer;
use crate::renderer::types::{DrawList, UIDrawList};
use ash::vk;
use gpu_allocator::vulkan::Allocation;

/// Frame context for submitting work to passes.
///
/// Passed to the closure in [`VulkanRenderer::render()`]. Provides a simple
/// API for submitting draw lists to named passes.
pub struct Frame<'a> {
    pub(super) graph: &'a FrameGraph,
    pub(super) renderer: &'a mut VulkanRenderer,
    pub(super) image_index: u32,
    pub(super) pending: HashMap<usize, PassExecutionData>,
    pub(super) resource_states: HashMap<String, ResourceState>,
    pub(super) temporary_buffers: Vec<(vk::Buffer, Allocation)>,
    pub(super) depth_buffer_written: bool,
    /// Whether the particle emit compute pass ran this frame.
    pub particle_emit_ran: bool,
    /// Whether to trigger particle debug readback this frame.
    pub particle_debug_readback: bool,
}

/// Data for a single pass execution.
#[derive(Default, Clone)]
pub(super) struct PassExecutionData {
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
            particle_emit_ran: false,
            particle_debug_readback: graph.params.particle_debug_readback,
        }
    }

    /// Get the current frame index from the renderer.
    /// This is the authoritative source for which frame's resources to use.
    fn current_frame(&self) -> usize {
        self.renderer.current_frame()
    }

    /// Get mutable access to the renderer.
    pub fn renderer_mut(&mut self) -> &mut VulkanRenderer {
        self.renderer
    }

    /// Get the particle emit workgroup count for this frame.
    pub fn particle_emit_workgroup_count(&self) -> u32 {
        self.graph.params.particle_emit_workgroup_count
    }

    /// Get the particle simulate workgroup count for this frame.
    pub fn particle_simulate_workgroup_count(&self) -> u32 {
        self.graph.params.particle_simulate_workgroup_count
    }

    /// Get the animation skeleton count for this frame.
    pub fn animation_skeleton_count(&self) -> u32 {
        self.graph.params.animation_skeleton_count
    }

    /// Get the skeleton copy commands for this frame.
    pub fn skeleton_copy_commands(&self) -> &[(u32, u32, u32)] {
        &self.graph.params.skeleton_copy_commands
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

    /// Resolve a color attachment for a pass.
    ///
    /// Handles both backbuffer and transient texture targets, including
    /// load/store/clear operation resolution from `pass.color_attachments`.
    ///
    /// Returns `None` if the pass has no color outputs.
    pub(super) fn resolve_color_attachment(
        &self,
        pass: &PassDesc,
    ) -> Result<Option<vk::RenderingAttachmentInfo<'_>>, RenderGraphError> {
        use super::frame_graph::BACKBUFFER_NAME;
        use crate::render_pass::{ClearValue, LoadOp, StoreOp};

        if pass.writes_to(BACKBUFFER_NAME) {
            let swapchain_view =
                self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();

            let backbuffer_written = self.resource_states.contains_key(BACKBUFFER_NAME);
            let load_op = if backbuffer_written {
                vk::AttachmentLoadOp::LOAD
            } else {
                vk::AttachmentLoadOp::CLEAR
            };

            return Ok(Some(
                vk::RenderingAttachmentInfo::default()
                    .image_view(swapchain_view)
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(load_op)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.1, 0.1, 0.1, 1.0],
                        },
                    }),
            ));
        }

        let color_name = match pass.writes.first() {
            Some(name) => name,
            None => return Ok(None),
        };

        let transient = self
            .graph
            .transient_texture(color_name, self.current_frame())
            .ok_or_else(|| {
                RenderGraphError::ResourceNotFound(format!(
                    "Color target '{}' not found. Use 'backbuffer' for swapchain or create a transient resource.",
                    color_name
                ))
            })?;

        let (load_op, store_op, clear_value) = pass
            .color_attachments
            .iter()
            .find(|(name, ..)| name == color_name)
            .map(|(_, _, load_op, store_op, clear_value)| {
                (
                    match load_op {
                        LoadOp::Load => vk::AttachmentLoadOp::LOAD,
                        LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
                        LoadOp::DontCare => vk::AttachmentLoadOp::NONE_EXT,
                    },
                    match store_op {
                        StoreOp::Store => vk::AttachmentStoreOp::STORE,
                        StoreOp::DontCare => vk::AttachmentStoreOp::NONE_EXT,
                    },
                    match clear_value {
                        ClearValue::Color(c) => vk::ClearColorValue { float32: *c },
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

        Ok(Some(
            vk::RenderingAttachmentInfo::default()
                .image_view(transient.image_view.vk())
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(load_op)
                .store_op(store_op)
                .clear_value(vk::ClearValue { color: clear_value }),
        ))
    }

    /// Execute all passes in order.
    pub(super) fn execute_passes(&mut self) -> Result<(), RenderGraphError> {
        self.particle_emit_ran = false;

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

        // === Execute all passes in topologically sorted order ===
        // The execution plan from the graph compiler ensures dependencies
        // execute before the passes that read from them.
        let execution_order = self.graph.execution_order();

        for index in execution_order {
            let pass = &self.graph.passes[index];
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
            if pass.writes_to(BACKBUFFER_NAME) {
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

            self.insert_barriers(&cmd, index)?;

            match pass.pass_type {
                super::pass::PassType::Graphics => {
                    match pass.kind {
                        Some(super::pass::PassKind::Shadow) => {
                            log::trace!("'{}' -> shadow pass", pass.name);
                            self.execute_shadow_pass(&cmd, pass)?;
                        }
                        Some(super::pass::PassKind::DepthPrepass) => {
                            log::trace!("'{}' -> depth prepass", pass.name);
                            self.execute_depth_prepass(&cmd, pass, data)?;
                        }
                        Some(super::pass::PassKind::Outline) => {
                            log::trace!("'{}' -> outline pass", pass.name);
                            self.execute_outline_pass(&cmd, pass, data)?;
                        }
                        Some(super::pass::PassKind::StencilIndicator) => {
                            log::trace!("'{}' -> stencil indicator pass", pass.name);
                            self.execute_stencil_indicator_pass(&cmd, pass, data)?;
                        }
                        Some(super::pass::PassKind::Compositing) => {
                            if let Some(material_handle) = pass.material {
                                log::trace!("'{}' -> compositing pass", pass.name);
                                self.execute_compositing_pass(&cmd, pass, material_handle)?;
                            } else {
                                log::warn!("Compositing pass '{}' has no material", pass.name);
                            }
                        }
                        Some(super::pass::PassKind::Fullscreen) => {
                            log::trace!("'{}' -> fullscreen pass", pass.name);
                            if let Some(pipeline) = pass.pipeline {
                                self.execute_fullscreen_pass(&cmd, pass, pipeline)?;
                            }
                        }
                        Some(super::pass::PassKind::Geometry) | None => {
                            // Geometry pass or pass without explicit kind — use heuristics
                            if let Some(material_handle) = pass.material {
                                if pass.compositing_viewports.is_some()
                                    && data.draw_lists.is_empty()
                                {
                                    log::trace!("'{}' -> compositing pass (heuristic)", pass.name);
                                    self.execute_compositing_pass(&cmd, pass, material_handle)?;
                                } else {
                                    log::trace!(
                                        "'{}' -> graphics pass with material (draw_lists={}, ui_draw_lists={})",
                                        pass.name,
                                        data.draw_lists.len(),
                                        data.ui_draw_lists.len()
                                    );
                                    self.execute_graphics_pass(&cmd, pass, data)?;
                                }
                            } else if pass.pipeline.is_some() && data.draw_lists.is_empty() {
                                log::trace!("'{}' -> fullscreen pass (heuristic)", pass.name);
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
                    }
                }
                super::pass::PassType::Compute => {
                    // Compute pass (e.g., particle simulation, light culling)
                    log::trace!("'{}' -> compute pass", pass.name);
                    if let Some(ref compute_fn) = pass.compute_fn {
                        // Custom compute dispatch via closure (e.g., light culling)
                        // The closure handles pipeline binding and dispatch internally
                        compute_fn(
                            self,
                            &cmd,
                            pass.pipeline
                                .unwrap_or(crate::handle::PipelineHandle::default()),
                        )?;
                    } else if let Some(pipeline) = pass.pipeline {
                        self.execute_compute_pass(&cmd, pass, pipeline)?;
                    } else {
                        log::warn!(
                            "Compute pass '{}' has no pipeline and no compute_fn",
                            pass.name
                        );
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
            if pass.name == "geometry"
                && let Some(ref particle_system) = self.renderer.particle_system
                && particle_system.alive_count() > 0
                && let Some(hdr_texture) = self
                    .graph
                    .transient_textures
                    .get(frame_idx)
                    .and_then(|m| m.get("hdr_color"))
                && let Err(e) = self.render_particles_to_texture(&cmd, hdr_texture)
            {
                log::error!("Failed to render particles: {}", e);
            }
        }

        Ok(())
    }
}

impl<'a> Drop for Frame<'a> {
    fn drop(&mut self) {
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

// PassExecutionData is a data container with no non-trivial logic to test.
// Full testing requires GPU context for dispatch/draw_list behavior.
