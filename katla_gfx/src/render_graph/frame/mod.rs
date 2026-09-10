mod barriers;
mod compositing;
mod depth_prepass;
mod draw_calls;
mod draw_helpers;
mod graphics_pass;
mod object_id_pass;
mod outline_pass;
mod parallel_geometry;
mod particle_rendering;
mod shadow_pass;
mod ui_rendering;

use std::collections::HashMap;
use std::rc::Rc;

use super::backend::RenderGraphBackend;
use super::error::RenderGraphError;
use super::frame_graph::FrameGraph;
use super::handles::PassId;
use super::pass::PassDesc;
use crate::handle::SkeletonHandle;
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
    /// Whether the backend-owned depth texture has been written this frame.
    ///
    /// Scheduling fact for barrier insertion only — attachment load/store
    /// behavior always comes from the pass's declared ops.
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
            depth_buffer_written: false,
            particle_emit_ran: false,
        }
    }

    pub(super) fn validate_submissions(&self) -> Result<(), RenderGraphError> {
        for &pass_index in self.pending.keys() {
            let pass = self
                .graph
                .pass(pass_index)
                .ok_or_else(|| RenderGraphError::PassNotFound(pass_index.to_string()))?;
            if !self.graph.is_pass_index_live(pass_index) {
                return Err(RenderGraphError::SubmissionToCulledPass(pass.name.clone()));
            }
        }
        Ok(())
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
    pub fn skeleton_copy_commands(&self) -> &[(SkeletonHandle, u32, u32)] {
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

use crate::renderer::VulkanRenderer;

use crate::render_graph::BACKBUFFER_NAME;

/// Build a depth/stencil attachment info for one aspect of a target.
///
/// The clear value union is shared between aspects; Vulkan reads `depth`
/// for the depth attachment and `stencil` for the stencil attachment.
fn depth_attachment_info(
    view: ash::vk::ImageView,
    ops: &crate::render_pass::AttachmentOps,
    clear: ash::vk::ClearDepthStencilValue,
) -> ash::vk::RenderingAttachmentInfo<'static> {
    use crate::render_pass::{LoadOp, StoreOp};

    ash::vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(ash::vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(match ops.load {
            LoadOp::Clear => ash::vk::AttachmentLoadOp::CLEAR,
            LoadOp::Load => ash::vk::AttachmentLoadOp::LOAD,
            LoadOp::DontCare => ash::vk::AttachmentLoadOp::NONE_EXT,
        })
        .store_op(match ops.store {
            StoreOp::Store => ash::vk::AttachmentStoreOp::STORE,
            StoreOp::DontCare => ash::vk::AttachmentStoreOp::NONE_EXT,
        })
        .clear_value(ash::vk::ClearValue {
            depth_stencil: clear,
        })
}

impl<'a> Frame<'a, VulkanRenderer> {
    pub(super) fn color_target_extent(&self, pass: &PassDesc) -> ash::vk::Extent2D {
        if self
            .graph
            .resource_id(BACKBUFFER_NAME)
            .is_some_and(|id| pass.writes_to(id))
        {
            return self.renderer.frame_context.extent;
        }
        pass.color_attachments
            .iter()
            .find_map(|(id, _)| {
                self.graph
                    .transient_texture_by_id(*id, self.current_frame())
                    .map(|texture| texture.extent)
            })
            .unwrap_or(self.renderer.frame_context.scene_extent)
    }

    /// Resolve the image view of a declared color target.
    ///
    /// The imported backbuffer resolves to the acquired swapchain image;
    /// everything else resolves to its graph transient texture.
    fn color_target_view(
        &self,
        id: crate::render_graph::handles::ResourceId,
    ) -> Result<ash::vk::ImageView, RenderGraphError> {
        if self.graph.resource_id(BACKBUFFER_NAME) == Some(id) {
            return Ok(self.renderer.frame_context.swapchain_image_views
                [self.image_index as usize]
                .vk());
        }

        self.graph
            .transient_texture_by_id(id, self.current_frame())
            .map(|texture| texture.image_view.vk())
            .ok_or_else(|| {
                RenderGraphError::ResourceNotFound(format!(
                    "Color target '{}' not found. Use 'backbuffer' for swapchain or create a transient resource.",
                    self.graph.resource_name(id).unwrap_or("?")
                ))
            })
    }

    /// Resolve the format of a declared color target (for clear-value typing).
    fn color_target_format(&self, id: crate::render_graph::handles::ResourceId) -> ash::vk::Format {
        self.graph
            .transient_texture_by_id(id, self.current_frame())
            .map(|texture| texture.format)
            .unwrap_or(ash::vk::Format::UNDEFINED)
    }

    /// Resolve every declared color attachment of a pass.
    ///
    /// Views come from the pass's declared targets and load/store/clear
    /// behavior comes from its declared attachment ops — nothing is inferred
    /// from renderer-local state. Returns an empty vec for passes without
    /// declared color attachments.
    pub(super) fn resolve_color_attachments(
        &self,
        pass: &PassDesc,
    ) -> Result<Vec<ash::vk::RenderingAttachmentInfo<'_>>, RenderGraphError> {
        use crate::render_pass::{ClearValue, LoadOp, StoreOp};

        let mut infos = Vec::with_capacity(pass.color_attachments.len());
        for (id, ops) in &pass.color_attachments {
            let view = self.color_target_view(*id)?;
            let format = self.color_target_format(*id);

            let clear = match ops.clear_value {
                ClearValue::Color(c) => c,
                // Validation rejects depth-stencil clear values on color
                // targets; this fallback keeps encoding deterministic.
                ClearValue::DepthStencil { .. } => [0.0, 0.0, 0.0, 1.0],
            };
            let clear_color = if format == ash::vk::Format::R32_UINT {
                // Integer targets clear through the uint32 union member.
                ash::vk::ClearColorValue {
                    uint32: [clear[0] as u32, 0, 0, 0],
                }
            } else {
                ash::vk::ClearColorValue { float32: clear }
            };

            infos.push(
                ash::vk::RenderingAttachmentInfo::default()
                    .image_view(view)
                    .image_layout(ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(match ops.load {
                        LoadOp::Clear => ash::vk::AttachmentLoadOp::CLEAR,
                        LoadOp::Load => ash::vk::AttachmentLoadOp::LOAD,
                        LoadOp::DontCare => ash::vk::AttachmentLoadOp::NONE_EXT,
                    })
                    .store_op(match ops.store {
                        StoreOp::Store => ash::vk::AttachmentStoreOp::STORE,
                        StoreOp::DontCare => ash::vk::AttachmentStoreOp::NONE_EXT,
                    })
                    .clear_value(ash::vk::ClearValue { color: clear_color }),
            );
        }
        Ok(infos)
    }

    /// Resolve the declared depth and stencil attachments for a pass.
    ///
    /// Targets the backend-owned frame depth texture; per-aspect load/store/
    /// clear behavior comes from the pass's declared depth ops. Returns
    /// `(depth, stencil)`; stencil is `None` when the frame depth has no
    /// stencil aspect.
    pub(super) fn resolve_frame_depth_attachments(
        &self,
        pass: &PassDesc,
    ) -> Result<
        (
            Option<ash::vk::RenderingAttachmentInfo<'_>>,
            Option<ash::vk::RenderingAttachmentInfo<'_>>,
        ),
        RenderGraphError,
    > {
        use crate::render_pass::ClearValue;

        if !pass.uses_depth {
            return Ok((None, None));
        }
        let Some(ops) = pass.depth_attachment else {
            return Err(RenderGraphError::InvalidConfiguration(format!(
                "graphics pass '{}' uses depth but declares no depth ops",
                pass.name
            )));
        };

        let frame_idx = self.current_frame();
        let depth_texture = self
            .renderer
            .frame_context
            .depth_render_textures
            .get(frame_idx)
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(format!(
                    "depth_render_textures missing entry for frame {}",
                    frame_idx
                ))
            })?;

        let clear = match ops.depth.clear_value {
            ClearValue::DepthStencil { depth, stencil } => {
                ash::vk::ClearDepthStencilValue { depth, stencil }
            }
            _ => ash::vk::ClearDepthStencilValue {
                depth: 0.0,
                stencil: 0,
            },
        };

        if let Some(ref ds_view) = depth_texture.depth_stencil_image_view {
            Ok((
                Some(depth_attachment_info(ds_view.vk(), &ops.depth, clear)),
                Some(depth_attachment_info(ds_view.vk(), &ops.stencil, clear)),
            ))
        } else {
            Ok((
                Some(depth_attachment_info(
                    depth_texture.image_view.vk(),
                    &ops.depth,
                    clear,
                )),
                None,
            ))
        }
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

            self.insert_barriers(&cmd, index)?;

            match pass.pass_type {
                super::pass::PassType::Graphics => match pass.kind {
                    Some(super::pass::PassKind::Shadow) => {
                        self.execute_shadow_pass(&cmd, pass, data)?;
                    }
                    Some(super::pass::PassKind::DepthPrepass) => {
                        self.execute_depth_prepass(&cmd, pass, data)?;
                    }
                    Some(super::pass::PassKind::Outline) => {
                        self.execute_outline_pass(&cmd, pass, data)?;
                    }
                    Some(super::pass::PassKind::ObjectId) => {
                        self.execute_object_id_pass(&cmd, pass, data)?;
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
                    Some(super::pass::PassKind::Ui) => {
                        self.execute_graphics_pass(&cmd, pass, data)?;
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
                        self.execute_compute_pass(&cmd, pass, pipeline, data.dispatch)?;
                    } else {
                        log::warn!(
                            "Compute pass '{}' has no pipeline and no compute_fn",
                            pass.name
                        );
                    }
                }
            }

            self.insert_post_pass_barriers(&cmd, index)?;

            if pass.uses_depth {
                self.depth_buffer_written = true;
            }
        }

        Ok(())
    }
}
