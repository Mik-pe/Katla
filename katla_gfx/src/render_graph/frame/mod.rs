mod barriers;
pub(crate) use barriers::state_layout;
mod compute_commands;
mod draw_calls;
mod graphics_bindings;
mod graphics_pass;
mod native_capture;
mod ui_rendering;

use std::collections::HashMap;
use std::rc::Rc;

use super::backend::RenderGraphBackend;
use super::error::RenderGraphError;
use super::frame_graph::FrameGraph;
use super::handles::PassId;
use super::pass::PassDesc;
use crate::renderer::types::{DrawList, PreparedDrawCounts, PreparedDraws, UIDrawList};

/// Frame context for submitting work to passes.
///
/// Passed to the closure in `FrameGraph::execute()`. Provides a simple
/// API for submitting draw lists to named passes.
pub struct Frame<'a, B: RenderGraphBackend> {
    pub(super) graph: &'a FrameGraph<B>,
    pub(super) renderer: &'a mut B,
    pub(super) image_index: u32,
    pub(super) pending: HashMap<usize, PassExecutionData>,
    pub(super) imported_image_states: HashMap<
        super::handles::ResourceId,
        Vec<(super::ImageSubresourceRange, super::ImageSyncState)>,
    >,
    /// What this frame's backend actually encoded, in encode order.
    ///
    /// Populated by the backend as it creates encoders; the graph-level
    /// dispatch records pass identity and declared attachment contract, so a
    /// trace can be compared with the compiled plan.
    pub(super) execution_trace: super::trace::ResourceExecutionTrace,
    /// Whether to record [`Self::execution_trace`]. Off unless a caller asks,
    /// so the steady-state path pays nothing.
    pub(super) trace_enabled: bool,
}

/// Data for a single pass execution.
#[derive(Default, Clone)]
pub(crate) struct PassExecutionData {
    pub(crate) draw_lists: Vec<Rc<DrawList>>,

    pub(crate) ui_draw_lists: Vec<UIDrawList>,

    pub(crate) dispatch: Option<(u32, u32, u32)>,

    pub(crate) uniform_data: Vec<u8>,
}

impl PassExecutionData {
    /// The pass's prepared draws: its submitted lists borrowed from frame-owned
    /// storage, addressable without rebuilding merged lists.
    pub(crate) fn prepared(&self) -> PreparedDraws<'_> {
        PreparedDraws::from_lists(&self.draw_lists)
    }

    /// Prepared draw/instance totals for this pass, for diagnostics.
    pub(crate) fn prepared_counts(&self) -> PreparedDrawCounts {
        self.prepared().counts()
    }
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
            imported_image_states: HashMap::new(),
            execution_trace: super::trace::ResourceExecutionTrace::new(),
            trace_enabled: false,
        }
    }

    /// Enable recording of the emitted encoder trace for this frame.
    pub(crate) fn enable_execution_trace(&mut self) {
        self.trace_enabled = true;
    }

    /// The encoders this frame emitted, in encode order.
    pub(crate) fn execution_trace(&self) -> &super::trace::ResourceExecutionTrace {
        &self.execution_trace
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

    /// Resolve a graph-declared buffer by name for the current frame slot.
    ///
    /// The returned backend buffer can be used by a backend-specific compute
    /// callback after its native handle is extracted. Imported renderer-owned
    /// buffers and graph-owned transient buffers share this lookup path.
    pub fn buffer(&self, name: &str) -> Option<super::backend::ResolvedGraphBuffer<'_, B>> {
        let id = self.graph.resource_id(name)?;
        self.graph
            .buffer_by_id(self.renderer, id, self.current_frame())
    }

    /// Get mutable access to the renderer.
    pub fn renderer_mut(&mut self) -> &mut B {
        self.renderer
    }

    /// Submit a draw list to a pass.
    ///
    /// The list moves into frame-owned storage for the frame's lifetime, so
    /// submitting the same list to several passes shares one reference-counted
    /// copy instead of deep-cloning per submission. Callers keep building and
    /// uploading through `DrawList`/`execute_draw_calls` unchanged.
    pub fn submit(&mut self, pass_id: PassId, draw_list: Rc<DrawList>) -> &mut Self {
        let index = pass_id.0 as usize;

        let counts = draw_list
            .draws
            .iter()
            .map(|draw| draw.instance_count().max(1))
            .sum::<u32>();
        log::debug!(
            "submit: pass_id={:?}, index={}, draws={}, instances={}",
            pass_id,
            index,
            draw_list.draws.len(),
            counts
        );

        self.pending
            .entry(index)
            .or_default()
            .draw_lists
            .push(draw_list);
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

fn next_output_contents(
    name: &str,
    defined: bool,
    ops: crate::render_pass::AttachmentOps,
) -> Result<bool, RenderGraphError> {
    use crate::render_pass::{LoadOp, StoreOp};
    if ops.load == LoadOp::Load && !defined {
        return Err(super::GraphValidationError::LoadingUninitializedOutput {
            pass: name.to_owned(),
        }
        .into());
    }
    Ok(ops.store == StoreOp::Store)
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
                    .or_else(|| {
                        self.imported_texture(*id).map(|texture| ash::vk::Extent2D {
                            width: texture.width,
                            height: texture.height,
                        })
                    })
            })
            .unwrap_or(self.renderer.frame_context.extent)
    }

    fn imported_texture(&self, id: super::ResourceId) -> Option<&crate::vulkan::texture::Texture> {
        self.graph
            .imported_images
            .get(&id)
            .and_then(|&handle| self.renderer.texture_manager.get_texture(handle))
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
            .or_else(|| self.imported_texture(id).map(|texture| texture.image_view().vk()))
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
            .or_else(|| {
                self.imported_texture(id)
                    .map(|texture| texture.format().into())
            })
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
    ) -> Result<Vec<ash::vk::RenderingAttachmentInfo<'static>>, RenderGraphError> {
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
    /// Resolves the graph's declared transient or imported depth texture and
    /// its per-aspect attachment operations.
    pub(super) fn resolve_frame_depth_attachments(
        &self,
        pass: &PassDesc,
    ) -> Result<
        (
            Option<ash::vk::RenderingAttachmentInfo<'static>>,
            Option<ash::vk::RenderingAttachmentInfo<'static>>,
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
        if let Some(id) = pass.depth_target {
            let (view, format) = self
                .graph
                .transient_texture_by_id(id, frame_idx)
                .map(|texture| (texture.image_view.vk(), texture.format))
                .or_else(|| {
                    self.imported_texture(id)
                        .map(|texture| (texture.image_view().vk(), texture.format().into()))
                })
                .ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Pass '{}' cannot resolve graph depth target {}",
                        pass.name, id.0
                    ))
                })?;
            let clear = |value| match value {
                ClearValue::DepthStencil { depth, stencil } => {
                    ash::vk::ClearDepthStencilValue { depth, stencil }
                }
                _ => ash::vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            };
            let stencil = matches!(
                format,
                ash::vk::Format::D32_SFLOAT_S8_UINT | ash::vk::Format::D24_UNORM_S8_UINT
            )
            .then(|| depth_attachment_info(view, &ops.stencil, clear(ops.stencil.clear_value)));
            return Ok((
                Some(depth_attachment_info(
                    view,
                    &ops.depth,
                    clear(ops.depth.clear_value),
                )),
                stencil,
            ));
        }
        Err(RenderGraphError::InvalidConfiguration(format!(
            "Pass '{}' uses depth without a declared graph target",
            pass.name
        )))
    }

    /// Execute all passes in order.
    pub(super) fn execute_passes(&mut self) -> Result<(), RenderGraphError> {
        let frame_idx = self.current_frame();
        let cmd = self.renderer.frame_context.command_buffers[frame_idx].clone();
        let execution_order = self.graph.execution_order();
        if self.trace_enabled {
            use super::capture::{CapturedFeedback, CapturedSubmission};
            self.execution_trace.backend.backend = "vulkan".into();
            self.execution_trace.backend.frame = Some(CapturedSubmission {
                frame_slot: frame_idx,
                generation: self.renderer.frame_generation.saturating_sub(1),
                command_allocator: frame_idx,
                feedback_identity: format!(
                    "frame_fence:{frame_idx}:{}",
                    self.renderer.frame_generation.saturating_sub(1)
                ),
                feedback: CapturedFeedback::Pending,
            });
        }
        let mut output_defined =
            self.renderer.frame_context.swapchain_image_contents[self.image_index as usize].get();
        let output_id = self.graph.resource_id(BACKBUFFER_NAME);

        for &index in &execution_order {
            let pass = &self.graph.passes[index];
            if let Some((_, ops)) = pass
                .color_attachments
                .iter()
                .find(|(id, _)| Some(*id) == output_id)
            {
                output_defined = next_output_contents(&pass.name, output_defined, *ops)?;
            }
        }
        for index in execution_order {
            let pass = &self.graph.passes[index];
            let data = self.pending.remove(&index).unwrap_or_default();

            self.insert_sync_barriers(&cmd, index)?;

            for access in &pass.buffer_accesses {
                if let Some(buffer) =
                    self.graph
                        .buffer_by_id(self.renderer, access.resource, frame_idx)
                {
                    use ash::vk::Handle;
                    self.renderer
                        .pending_graph_buffers
                        .insert(buffer.vk_buffer().as_raw());
                }
            }

            let counts = data.prepared_counts();

            match pass.pass_type {
                super::pass::PassType::Graphics => {
                    self.execute_graphics_pass(&cmd, pass, data)?;
                }
                super::pass::PassType::Compute | super::pass::PassType::Transfer => {
                    self.execute_compute_commands(&cmd, pass, data.dispatch)?;
                }
            }

            if self.trace_enabled {
                let encode_position = self.execution_trace.entries().len();
                let pass_type = pass.pass_type;
                let outcome = if self
                    .execution_trace
                    .backend
                    .encoders
                    .iter()
                    .any(|encoder| encoder.pass_index == Some(index))
                {
                    super::trace::EmittedPassOutcome::Encoded
                } else {
                    super::trace::EmittedPassOutcome::SkippedNoWork
                };
                let entry = super::trace::ResourceExecutionTraceEntry {
                    pass_index: index,
                    name: pass.name.clone(),
                    pass_type,
                    encode_position,
                    outcome,
                    color_attachment_ops: pass
                        .color_attachments
                        .iter()
                        .map(|(_, ops)| *ops)
                        .collect(),
                    depth_attachment_ops: pass.depth_attachment,
                    draw_calls: counts.draw_calls,
                    instances: counts.instances,
                    color_targets: if pass_type == super::pass::PassType::Graphics {
                        super::trace::color_target_names(&self.graph.resources, pass)
                    } else {
                        Vec::new()
                    },
                    depth_target: (pass_type == super::pass::PassType::Graphics && pass.uses_depth)
                        .then_some(pass.depth_target)
                        .flatten()
                        .and_then(|id| self.graph.resource_name(id))
                        .map(str::to_owned),
                };
                self.execution_trace.push(entry);
            }
        }

        self.insert_final_sync_barriers(&cmd)?;
        self.renderer
            .frame_context
            .pending_output_contents
            .set(Some((self.image_index as usize, output_defined)));

        Ok(())
    }
}

#[cfg(test)]
mod output_contract_tests {
    use super::*;
    use crate::render_pass::{AttachmentOps, ClearValue, StoreOp};

    #[test]
    fn test_fresh_acquired_output_rejects_load_without_initialization() {
        assert!(matches!(
            next_output_contents("overlay", false, AttachmentOps::load()),
            Err(RenderGraphError::Validation(
                super::super::GraphValidationError::LoadingUninitializedOutput { .. }
            ))
        ));
        let defined = next_output_contents(
            "clear",
            false,
            AttachmentOps::clear(ClearValue::Color([0.0; 4])),
        )
        .unwrap();
        assert!(next_output_contents("overlay", defined, AttachmentOps::load()).unwrap());
    }

    #[test]
    fn test_discarded_output_contents_cannot_be_loaded_later() {
        let mut discard = AttachmentOps::load();
        discard.store = StoreOp::DontCare;
        let defined = next_output_contents("discard", true, discard).unwrap();
        assert!(!defined);
        assert!(next_output_contents("load", defined, AttachmentOps::load()).is_err());
    }
}
