#[cfg(feature = "vulkan")]
mod barriers;
#[cfg(feature = "vulkan")]
mod compositing;
#[cfg(feature = "vulkan")]
mod depth_prepass;
#[cfg(feature = "vulkan")]
mod draw_calls;
#[cfg(feature = "vulkan")]
mod draw_helpers;
#[cfg(feature = "vulkan")]
mod graphics_pass;
#[cfg(feature = "vulkan")]
mod outline_pass;
#[cfg(feature = "vulkan")]
mod parallel_geometry;
#[cfg(feature = "vulkan")]
mod parallel_shadow;
#[cfg(feature = "vulkan")]
mod particle_rendering;
#[cfg(feature = "vulkan")]
mod shadow_pass;
#[cfg(feature = "vulkan")]
mod ui_rendering;

use std::collections::HashMap;
use std::rc::Rc;

use super::backend::RenderGraphBackend;
use super::error::RenderGraphError;
use super::frame_graph::FrameGraph;
use super::handles::PassId;
use super::pass::PassDesc;
use crate::renderer::types::{DrawList, UIDrawList};

/// Frame context for submitting work to passes.
///
/// Passed to the closure in `FrameGraph::execute()`. Provides a simple
/// API for submitting draw lists to named passes.
pub struct Frame<'a, B: RenderGraphBackend> {
    pub(super) graph: &'a FrameGraph<B>,
    pub(super) renderer: &'a mut B,
    pub(super) image_index: u32,
    pub(super) pending: HashMap<usize, PassExecutionData>,
    /// Whether the backbuffer has been written to this frame.
    pub(super) backbuffer_written: bool,
    pub(super) depth_buffer_written: bool,
    /// Whether the particle emit compute pass ran this frame.
    pub particle_emit_ran: bool,
}

/// Data for a single pass execution.
#[derive(Default, Clone)]
pub(crate) struct PassExecutionData {
    pub(crate) draw_lists: Vec<Rc<DrawList>>,

    pub(crate) ui_draw_lists: Vec<UIDrawList>,

    pub(crate) dispatch: Option<(u32, u32, u32)>,

    pub(crate) uniform_data: Vec<u8>,
}

impl<'a, B: RenderGraphBackend> Frame<'a, B> {
    /// Create a new frame context.
    pub(crate) fn new(
        graph: &'a FrameGraph<B>,
        renderer: &'a mut B,
        image_index: u32,
        _frame_idx: usize,
    ) -> Self {
        Self {
            graph,
            renderer,
            image_index,
            pending: HashMap::new(),
            backbuffer_written: false,
            depth_buffer_written: false,
            particle_emit_ran: false,
        }
    }

    /// Get the current frame index from the renderer.
    fn current_frame(&self) -> usize {
        self.renderer.current_frame()
    }

    /// Get mutable access to the renderer.
    pub fn renderer_mut(&mut self) -> &mut B {
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
    pub fn submit(&mut self, pass_id: PassId, draw_list: &DrawList) -> &mut Self {
        let index = pass_id.0 as usize;

        self.pending
            .entry(index)
            .or_default()
            .draw_lists
            .push(Rc::new(draw_list.clone()));
        self
    }

    /// Submit a UI draw list to a pass.
    pub fn submit_ui(&mut self, pass_id: PassId, ui_draw_list: &UIDrawList) -> &mut Self {
        let index = pass_id.0 as usize;

        let cmd_count = ui_draw_list.commands.len();
        self.pending
            .entry(index)
            .or_default()
            .ui_draw_lists
            .push(ui_draw_list.clone());

        log::debug!(
            "submit_ui: pass_id={:?}, index={}, commands={}, pending UI lists now={}",
            pass_id,
            index,
            cmd_count,
            self.pending[&index].ui_draw_lists.len()
        );

        self
    }

    /// Dispatch compute workgroups for a pass.
    pub fn dispatch(&mut self, pass_id: PassId, x: u32, y: u32, z: u32) -> &mut Self {
        let index = pass_id.0 as usize;

        self.pending.entry(index).or_default().dispatch = Some((x, y, z));
        self
    }

    /// Push uniform data for a pass.
    pub fn push_uniform(&mut self, pass_id: PassId, data: &[u8]) -> &mut Self {
        let index = pass_id.0 as usize;

        self.pending
            .entry(index)
            .or_default()
            .uniform_data
            .extend_from_slice(data);
        self
    }
}

#[cfg(feature = "vulkan")]
use crate::renderer::VulkanRenderer;

#[cfg(feature = "vulkan")]
use crate::render_graph::BACKBUFFER_NAME;

#[cfg(feature = "vulkan")]
impl<'a> Frame<'a, VulkanRenderer> {
    /// Resolve a color attachment for a pass.
    ///
    /// Handles both backbuffer and transient texture targets, including
    /// load/store/clear operation resolution from `pass.color_attachments`.
    ///
    /// Returns `None` if the pass has no color outputs.
    pub(super) fn resolve_color_attachment(
        &self,
        pass: &PassDesc,
    ) -> Result<Option<ash::vk::RenderingAttachmentInfo<'_>>, RenderGraphError> {
        use crate::render_pass::{ClearValue, LoadOp, StoreOp};

        let backbuffer_id = self.graph.resource_id(BACKBUFFER_NAME);

        if backbuffer_id.is_some_and(|id| pass.writes_to(id)) {
            let swapchain_view =
                self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();

            let load_op = if self.backbuffer_written {
                ash::vk::AttachmentLoadOp::LOAD
            } else {
                ash::vk::AttachmentLoadOp::CLEAR
            };

            return Ok(Some(
                ash::vk::RenderingAttachmentInfo::default()
                    .image_view(swapchain_view)
                    .image_layout(ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(load_op)
                    .store_op(ash::vk::AttachmentStoreOp::STORE)
                    .clear_value(ash::vk::ClearValue {
                        color: ash::vk::ClearColorValue {
                            float32: [0.1, 0.1, 0.1, 1.0],
                        },
                    }),
            ));
        }

        let &color_id = match pass.writes.first() {
            Some(id) => id,
            None => return Ok(None),
        };

        let color_name = self.graph.resource_name(color_id).unwrap_or("?");

        let transient = self
            .graph
            .transient_texture_by_id(color_id, self.current_frame())
            .ok_or_else(|| {
                RenderGraphError::ResourceNotFound(format!(
                    "Color target '{}' not found. Use 'backbuffer' for swapchain or create a transient resource.",
                    color_name
                ))
            })?;

        let resource_already_written =
            transient.state() != super::resource::ResourceState::Undefined;

        let (load_op, store_op, clear_value) = pass
            .color_attachments
            .iter()
            .find(|(id, ..)| *id == color_id)
            .map(|(_, _, load_op, store_op, clear_value)| {
                (
                    match load_op {
                        LoadOp::Load => ash::vk::AttachmentLoadOp::LOAD,
                        LoadOp::Clear => ash::vk::AttachmentLoadOp::CLEAR,
                        LoadOp::DontCare => ash::vk::AttachmentLoadOp::NONE_EXT,
                    },
                    match store_op {
                        StoreOp::Store => ash::vk::AttachmentStoreOp::STORE,
                        StoreOp::DontCare => ash::vk::AttachmentStoreOp::NONE_EXT,
                    },
                    match clear_value {
                        ClearValue::Color(c) => ash::vk::ClearColorValue { float32: *c },
                        _ => ash::vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    },
                )
            })
            .unwrap_or_else(|| {
                let load = if resource_already_written {
                    ash::vk::AttachmentLoadOp::LOAD
                } else {
                    ash::vk::AttachmentLoadOp::CLEAR
                };
                (
                    load,
                    ash::vk::AttachmentStoreOp::STORE,
                    ash::vk::ClearColorValue {
                        float32: [0.1, 0.1, 0.1, 1.0],
                    },
                )
            });

        Ok(Some(
            ash::vk::RenderingAttachmentInfo::default()
                .image_view(transient.image_view.vk())
                .image_layout(ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(load_op)
                .store_op(store_op)
                .clear_value(ash::vk::ClearValue { color: clear_value }),
        ))
    }

    /// Execute all passes in order.
    pub(super) fn execute_passes(&mut self) -> Result<(), RenderGraphError> {
        self.particle_emit_ran = false;

        let frame_idx = self.current_frame();
        let cmd = self.renderer.frame_context.command_buffers[frame_idx].clone();
        let execution_order = self.graph.execution_order();

        for index in execution_order {
            let pass = &self.graph.passes[index];
            let data = self.pending.remove(&index).unwrap_or_default();

            let writes_backbuffer = self
                .graph
                .resource_id(BACKBUFFER_NAME)
                .is_some_and(|id| pass.writes_to(id));

            self.insert_barriers(&cmd, index)?;

            match pass.pass_type {
                super::pass::PassType::Graphics => match pass.kind {
                    Some(super::pass::PassKind::Shadow) => {
                        self.execute_shadow_pass(&cmd, pass)?;
                    }
                    Some(super::pass::PassKind::DepthPrepass) => {
                        self.execute_depth_prepass(&cmd, pass, data)?;
                    }
                    Some(super::pass::PassKind::Outline) => {
                        self.execute_outline_pass(&cmd, pass, data)?;
                    }
                    Some(super::pass::PassKind::StencilIndicator) => {
                        self.execute_stencil_indicator_pass(&cmd, pass, data)?;
                    }
                    Some(super::pass::PassKind::Compositing) => {
                        if let Some(material_handle) = pass.material {
                            self.execute_compositing_pass(&cmd, pass, material_handle)?;
                        } else {
                            log::warn!("Compositing pass '{}' has no material", pass.name);
                        }
                    }
                    Some(super::pass::PassKind::Fullscreen) => {
                        if let Some(pipeline) = pass.pipeline {
                            self.execute_fullscreen_pass(&cmd, pass, pipeline)?;
                        }
                    }
                    Some(super::pass::PassKind::Geometry) => {
                        if let Some(material_handle) = pass.material {
                            if pass.compositing_viewports.is_some() && data.draw_lists.is_empty() {
                                self.execute_compositing_pass(&cmd, pass, material_handle)?;
                            } else {
                                self.execute_graphics_pass(&cmd, pass, data)?;
                            }
                        } else if pass.pipeline.is_some() && data.draw_lists.is_empty() {
                            if let Some(pipeline) = pass.pipeline {
                                self.execute_fullscreen_pass(&cmd, pass, pipeline)?;
                            }
                        } else {
                            self.execute_graphics_pass(&cmd, pass, data)?;
                        }
                    }
                    Some(super::pass::PassKind::Particles) => {
                        self.execute_particle_pass(&cmd, pass)?;
                    }
                    None => {
                        if let Some(material_handle) = pass.material {
                            if pass.compositing_viewports.is_some() && data.draw_lists.is_empty() {
                                self.execute_compositing_pass(&cmd, pass, material_handle)?;
                            } else {
                                self.execute_graphics_pass(&cmd, pass, data)?;
                            }
                        } else if pass.pipeline.is_some() && data.draw_lists.is_empty() {
                            if let Some(pipeline) = pass.pipeline {
                                self.execute_fullscreen_pass(&cmd, pass, pipeline)?;
                            }
                        } else {
                            self.execute_graphics_pass(&cmd, pass, data)?;
                        }
                    }
                },
                super::pass::PassType::Compute => {
                    if let Some(ref compute_fn) = pass.compute_fn {
                        compute_fn(self, &cmd, pass.pipeline.unwrap_or_default())?;
                    } else if let Some(pipeline) = pass.pipeline {
                        self.execute_compute_pass(&cmd, pass, pipeline, data.dispatch.clone())?;
                    } else {
                        log::warn!(
                            "Compute pass '{}' has no pipeline and no compute_fn",
                            pass.name
                        );
                    }
                }
            }

            self.insert_post_pass_barriers(&cmd, index)?;

            if writes_backbuffer {
                self.backbuffer_written = true;
            }
            if pass.uses_depth {
                self.depth_buffer_written = true;
            }
        }

        Ok(())
    }
}
