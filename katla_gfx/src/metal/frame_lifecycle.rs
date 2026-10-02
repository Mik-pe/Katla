//! Three bounded frame slots own allocators and all mutable upload storage.
//! Acquiring a busy slot waits for that slot's feedback; submitting never waits.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::MTL4CommandAllocator;
use std::time::{Duration, Instant};

use crate::error::RendererError;
use crate::renderer::frame_scope::{FrameAcquisition, FrameToken};
use crate::texture::ImageFormat;

use super::metal_renderer::MetalRenderer;

/// Marker methods implementing the frame-scoped contract on Metal.
impl MetalRenderer {
    pub(crate) fn frame_check(&self, frame: &FrameToken) -> Result<(), RendererError> {
        match &self.active_frame {
            Some(active) if active == frame => Ok(()),
            Some(_) => Err(RendererError::InvalidOperation(
                "stale frame token: the frame was already finished or superseded".into(),
            )),
            None => Err(RendererError::InvalidOperation(
                "no frame is currently acquired; call acquire_frame first".into(),
            )),
        }
    }

    pub(crate) fn frame_write_check(&self, frame: &FrameToken) -> Result<(), RendererError> {
        self.frame_check(frame)?;
        if self.pending_frame.is_some() {
            return Err(RendererError::InvalidOperation(
                "Frame-local writes must precede render".into(),
            ));
        }
        Ok(())
    }

    /// Abort any still-open frame, either explicitly (caller aborts) or
    /// implicitly (a new frame is acquired while this one is still open).
    /// Releases a drawable this frame acquired from the surface; a headless
    /// drawable installed by `set_headless_drawable` is never touched.
    pub(crate) fn frame_clear(&mut self) {
        let abandoned = self.pending_frame.take().is_some();
        self.pending_buffer_accesses.borrow_mut().clear();
        if abandoned {
            let slot = self.frame_index();
            self.frame_slots[slot].timestamps.reset();
        }
        self.texture_uploads
            .cancel_unsubmitted(self.frame_slots[self.frame_index()].generation);
        if self.active_frame.take().is_some() {
            log::debug!(
                "Metal frame aborted without present (slot {})",
                self.frame_index
            );
        }
        if self.frame_owns_drawable {
            self.current_drawable_texture = None;
            self.drawable_texture_view = None;
            self.context.surface.discard_drawable();
            self.frame_owns_drawable = false;
        }
        self.frame_poisoned = None;
    }

    /// Drain all submissions for an explicit synchronous tool or rebuild.
    #[cfg(test)]
    pub(crate) fn wait_for_frame_impl(&mut self) -> Result<(), RendererError> {
        for slot in 0..super::metal_renderer::FRAMES_IN_FLIGHT {
            self.wait_for_slot(slot)?;
        }
        self.buffer_history.borrow_mut().clear();
        self.buffer_history_retirement.borrow_mut().clear();
        Ok(())
    }

    pub(crate) fn wait_for_slot(&mut self, slot: usize) -> Result<(), RendererError> {
        let started = Instant::now();
        if let Some(submission) = self.frame_slots[slot].submission.take() {
            let feedback = match submission.wait_until_completed() {
                Ok(feedback) => feedback,
                Err(error) => {
                    self.texture_uploads
                        .retire_failed_submission(self.frame_slots[slot].generation);
                    return Err(error);
                }
            };
            self.frame_metrics.gpu_frame_time =
                Duration::from_secs_f64((feedback.gpu_end - feedback.gpu_start).max(0.0));
            self.texture_uploads
                .retire_completed(self.frame_slots[slot].generation);
        }
        self.buffer_history_retirement
            .borrow_mut()
            .retire_completed(&mut self.buffer_history.borrow_mut());
        if let Some(queries) = &self.timestamp_queries {
            queries.cache_completed(&self.frame_slots[slot].timestamps);
        }
        self.frame_slots[slot].timestamps.reset();
        self.frame_slots[slot].allocator.reset();
        self.frame_metrics.slot_wait = started.elapsed();
        self.frame_metrics.in_flight = self
            .frame_slots
            .iter()
            .filter(|slot| {
                slot.submission
                    .as_ref()
                    .is_some_and(|submission| !submission.completion.is_complete())
            })
            .count();
        Ok(())
    }

    /// Execute the frame graph for an open frame: collect draw lists, record
    /// this frame's compute work, and encode the compiled passes into the
    /// drawable. A failure poisons the frame so `present` cannot submit
    /// half-encoded work.
    pub fn render<F>(
        &mut self,
        frame: &FrameToken,
        frame_graph: &mut crate::render_graph::FrameGraph<MetalRenderer>,
        f: F,
    ) -> Result<(), RendererError>
    where
        F: FnOnce(&mut crate::render_graph::Frame<'_, MetalRenderer>),
    {
        self.frame_check(frame)?;
        if self.pending_frame.is_some() || self.frame_slots[frame.slot()].submission.is_some() {
            return Err(RendererError::InvalidOperation(
                "The acquired frame has already been submitted".into(),
            ));
        }
        if let Some(reason) = &self.frame_poisoned {
            return Err(RendererError::InvalidOperation(format!(
                "frame is poisoned by a previous render failure and cannot render again: {reason}"
            )));
        }

        let pending = match frame_graph.collect_draw_lists(self, f) {
            Ok(pending) => pending,
            Err(e) => {
                let error = RendererError::InvalidOperation(e.to_string());
                self.frame_poisoned = Some(format!("{error:?}"));
                return Err(error);
            }
        };

        let frame_idx = self.frame_index();

        self.bindless_manager.publish_snapshot()?;
        if let Some(light) = &mut self.light_culling {
            light.prepare_frame(
                &self.frame_uniforms.view_matrix,
                &self.frame_uniforms.proj_matrix,
            );
        }

        match self.execute_metal_passes(frame, pending, frame_graph, frame_idx) {
            Ok(trace) => {
                frame_graph.store_last_execution_trace(trace);
                Ok(())
            }
            Err(e) => {
                self.texture_uploads
                    .cancel_unsubmitted(self.frame_slots[self.frame_index()].generation);
                self.frame_poisoned = Some(format!("{e:?}"));
                Err(e)
            }
        }
    }

    /// Commit the recorded frame, present its drawable, and release its token.
    pub(crate) fn present_frame(&mut self, frame: FrameToken) -> Result<(), RendererError> {
        use crate::backend::command::GpuCommandBuffer;
        use objc2_metal::MTLBuffer;
        self.frame_check(&frame)?;
        if let Some(reason) = self.frame_poisoned.take() {
            self.frame_clear();
            return Err(RendererError::InvalidOperation(format!(
                "frame is poisoned by a previous render failure: {reason}"
            )));
        }
        let slot = frame.slot();
        if let Some(pending) = self.pending_frame.take() {
            pending.command.resources.check()?;
            self.context
                .surface
                .wait_for_drawable(&self.context.command_queue);
            let started = Instant::now();
            pending.command.submit(&self.context);
            self.frame_metrics.cpu_submit = started.elapsed();
            self.texture_uploads
                .mark_submitted(self.frame_slots[slot].generation);
            self.context.surface.present(&self.context.command_queue);
            self.frame_slots[slot]
                .timestamps
                .submitted(pending.command.completion.clone());
            for access in self.pending_buffer_accesses.borrow_mut().drain(..) {
                let identity = access.buffer.inner.gpuAddress();
                self.buffer_history
                    .borrow_mut()
                    .record(identity, access.offset, &access.accesses);
                self.buffer_history_retirement.borrow_mut().record(
                    identity,
                    &access.buffer,
                    slot,
                    &pending.command.completion,
                );
            }
            self.frame_slots[slot].picking_target =
                pending
                    .picking_target
                    .map(|view| super::picking::MetalPickingSource {
                        view,
                        slot,
                        generation: self.frame_slots[slot].generation,
                    });
            self.frame_slots[slot].submission = Some(pending.command);
            self.last_submitted_slot = Some(slot);
            self.defined_output_contents = pending.defined_outputs;
            self.frame_metrics.in_flight = self
                .frame_slots
                .iter()
                .filter(|slot| {
                    slot.submission
                        .as_ref()
                        .is_some_and(|submission| !submission.completion.is_complete())
                })
                .count();
            self.frame_metrics.cpu_lead = self.frame_metrics.in_flight;
        } else if self.frame_owns_drawable {
            self.context.surface.discard_drawable();
        }
        self.active_frame = None;
        if self.frame_owns_drawable {
            self.current_drawable_texture = None;
        }
        self.frame_owns_drawable = false;
        self.drawable_texture_view = None;
        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }
}

/// Wait for a free frame slot and acquire the next drawable.
///
/// A surface with no drawable available this tick (minimized/occluded) maps to
/// [`FrameAcquisition::Unavailable`] with no renderer state touched. Headless
/// renderers reuse the drawable installed by `set_headless_drawable`.
pub(crate) fn acquire_frame(
    renderer: &mut MetalRenderer,
) -> Result<FrameAcquisition, RendererError> {
    // An unfinished frame from an earlier acquisition is abandoned here. A drawable the
    // abandoned frame acquired from the surface is released with it.
    renderer.poll_material_reloads_impl();
    renderer.frame_clear();
    renderer.wait_for_slot(renderer.frame_index())?;
    let slot = renderer.frame_index();
    if let Some(light) = &mut renderer.light_culling {
        light.select_slot(slot);
    }
    if let Some(animation) = &mut renderer.animation_system {
        animation.select_slot(slot);
    }

    // If a headless drawable is already set, skip acquiring from the surface
    if renderer.current_drawable_texture.is_some() {
        let slot = renderer.frame_index();
        let token = FrameToken::new(slot, renderer.frame_generation);
        renderer.frame_slots[slot].generation = renderer.frame_generation;
        renderer.frame_generation += 1;
        renderer.active_frame = Some(token);
        return Ok(FrameAcquisition::Ready(token));
    }

    match renderer.context.surface.try_acquire_next_drawable()? {
        Some(texture) => {
            renderer.drawable_texture_view = Some(super::texture::MetalTextureView::new(
                texture.clone(),
                super::texture::MetalTexture::new(texture.clone(), ImageFormat::B8G8R8A8Srgb),
            ));
            renderer.current_drawable_texture = Some(texture);
            renderer.frame_owns_drawable = true;
            let slot = renderer.frame_index();
            let token = FrameToken::new(slot, renderer.frame_generation);
            renderer.frame_slots[slot].generation = renderer.frame_generation;
            renderer.frame_generation += 1;
            renderer.active_frame = Some(token);
            Ok(FrameAcquisition::Ready(token))
        }
        None => Ok(FrameAcquisition::Unavailable),
    }
}

/// Abort implementation: release the acquired drawable without presenting and
/// leave the frame slot untouched, so the next acquire reuses it normally.
pub(crate) fn abort_frame(renderer: &mut MetalRenderer, frame: FrameToken) {
    if renderer.frame_check(&frame).is_ok() {
        renderer.frame_clear();
    }
}

#[cfg(test)]
impl MetalRenderer {
    /// Direct plan execution for tests that build `PassExecutionData` manually
    /// instead of through a frame graph. A failure poisons the open frame.
    pub(crate) fn render_frame_manual(
        &mut self,
        frame: &FrameToken,
        plan: &super::execution_plan::MetalExecutionPlan,
        pending: std::collections::HashMap<usize, crate::render_graph::PassExecutionData>,
        graph: &crate::render_graph::FrameGraph<Self>,
    ) -> Result<(), RendererError> {
        self.frame_check(frame)?;
        if self.pending_frame.is_some() || self.frame_slots[frame.slot()].submission.is_some() {
            return Err(RendererError::InvalidOperation(
                "The acquired frame has already been submitted".into(),
            ));
        }
        if let Some(reason) = &self.frame_poisoned {
            return Err(RendererError::InvalidOperation(format!(
                "frame is poisoned by a previous render failure and cannot render again: {reason}"
            )));
        }
        self.bindless_manager.publish_snapshot()?;
        match self.render_frame(frame, plan, pending, graph, false) {
            Ok(_trace) => Ok(()),
            Err(e) => {
                self.texture_uploads
                    .cancel_unsubmitted(self.frame_slots[self.frame_index()].generation);
                self.frame_poisoned = Some(format!("{e:?}"));
                Err(e)
            }
        }
    }
}

pub(crate) struct MetalPendingFrame {
    pub(crate) command: super::command_buffer::MetalCommandBuffer,
    pub(crate) defined_outputs: std::collections::HashSet<(u64, u8)>,
    pub(crate) picking_target: Option<super::texture::MetalTextureView>,
}

pub(crate) struct MetalBufferExecution {
    pub(crate) buffer: super::buffer::MetalBuffer,
    pub(crate) offset: u64,
    pub(crate) accesses: Vec<crate::render_graph::BufferAccess>,
}

pub(crate) struct MetalFrameSlot {
    pub(crate) allocator: Retained<ProtocolObject<dyn MTL4CommandAllocator>>,
    pub(crate) submission: Option<super::command_buffer::MetalCommandBuffer>,
    pub(crate) generation: u64,
    pub(crate) timestamps: super::timestamp_queries::MetalTimestampSlot,
    pub(crate) picking_target: Option<super::picking::MetalPickingSource>,
}

impl MetalFrameSlot {
    pub(crate) fn new(
        context: &super::context::MetalContext,
        slot: usize,
    ) -> Result<Self, RendererError> {
        Ok(Self {
            allocator: context.create_command_allocator(slot)?,
            submission: None,
            generation: 0,
            picking_target: None,
            timestamps: super::timestamp_queries::MetalTimestampSlot::new(&context.device, slot)?,
        })
    }
}

/// Most recently observed native frame completion and CPU submission costs.
#[derive(Clone, Debug, Default)]
pub struct MetalFrameMetrics {
    pub slot_wait: Duration,
    pub cpu_submit: Duration,
    pub gpu_frame_time: Duration,
    pub in_flight: usize,
    pub cpu_lead: usize,
}
