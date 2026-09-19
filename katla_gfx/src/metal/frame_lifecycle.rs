//! Frame-scoped lifecycle for the Metal backend.
//!
//! [`acquire_frame`] waits for the previous submission to complete and acquires
//! the next drawable (unless a headless drawable is already set). The returned
//! [`FrameToken`] owns the drawable and the submission slot until
//! [`GpuRenderer::present`](crate::renderer::gpu_renderer::GpuRenderer::present)
//! consumes it (drawable released, frame slot advanced) or the frame is aborted
//! (drawable released without presenting, slot untouched).

use objc2_metal::{MTLCommandBuffer, MTLCommandBufferStatus};

use crate::error::{GpuExecutionFailure, RendererError};
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

    /// Abort any still-open frame, either explicitly (caller aborts) or
    /// implicitly (a new frame is acquired while this one is still open).
    /// Releases a drawable this frame acquired from the surface; a headless
    /// drawable installed by `set_headless_drawable` is never touched.
    pub(crate) fn frame_clear(&mut self) {
        if self.active_frame.take().is_some() {
            log::debug!(
                "Metal frame aborted without present (slot {})",
                self.frame_index
            );
        }
        if self.frame_owns_drawable {
            self.current_drawable_texture = None;
            self.drawable_texture_view = None;
            self.frame_owns_drawable = false;
        }
        self.frame_poisoned = None;
    }

    /// Wait for the previous frame's GPU work to complete.
    ///
    /// Called by `acquire_frame` before any CPU writes to per-frame resources.
    pub(crate) fn wait_for_frame_impl(&mut self) -> Result<(), RendererError> {
        if let Some(cmd_buffer) = self.last_command_buffer.take() {
            cmd_buffer.waitUntilCompleted();
            self.texture_uploads.retire_completed();

            let status = cmd_buffer.status();
            if status != MTLCommandBufferStatus::Completed {
                let label = cmd_buffer
                    .label()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "<unlabeled>".to_string());
                let error = cmd_buffer.error();
                let code = error.as_ref().map(|value| value.code() as i64);
                let domain = error.as_ref().map(|value| value.domain().to_string());
                let description = error
                    .as_ref()
                    .map(|value| value.localizedDescription().to_string());
                let encoders = error
                    .as_ref()
                    .map(|value| {
                        super::diagnostics::extract_encoder_diagnostics(value)
                            .into_iter()
                            .map(super::diagnostics::GpuEncoderDiagnostics::into_error_payload)
                            .collect()
                    })
                    .unwrap_or_default();

                return Err(RendererError::GpuExecutionFailed(Box::new(
                    GpuExecutionFailure {
                        backend: "Metal",
                        label,
                        status: format!("{status:?}"),
                        code,
                        domain,
                        description,
                        encoders,
                    },
                )));
            }
        }
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

        self.record_frame_compute();

        if let Err(e) = self.execute_metal_passes(frame, pending, frame_graph, frame_idx) {
            self.frame_poisoned = Some(format!("{e:?}"));
            return Err(e);
        }

        Ok(())
    }

    /// Present implementation: the passes encoded by `render` presented their
    /// drawable during command-buffer commit, so present releases the frame's
    /// drawable reference and advances the frame slot. Consumes the frame.
    pub(crate) fn present_frame(&mut self, frame: FrameToken) -> Result<(), RendererError> {
        self.frame_check(&frame)?;
        if let Some(reason) = self.frame_poisoned.take() {
            self.active_frame = None;
            return Err(RendererError::InvalidOperation(format!(
                "frame is poisoned by a previous render failure: {reason}"
            )));
        }
        self.active_frame = None;
        // The passes encoded by `render` consumed (and presented) the drawable.
        self.frame_owns_drawable = false;

        // In headless mode, keep the drawable texture for readback.
        // It will be cleaned up by take_headless_texture() or destroy().
        // Just clear the view reference so the next frame gets a fresh view.
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
    renderer.frame_clear();
    renderer.wait_for_frame_impl()?;

    // If a headless drawable is already set, skip acquiring from the surface
    if renderer.current_drawable_texture.is_some() {
        let slot = renderer.frame_index();
        let token = FrameToken::new(slot, renderer.frame_generation);
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
    ) -> Result<(), RendererError> {
        self.frame_check(frame)?;
        if let Some(reason) = &self.frame_poisoned {
            return Err(RendererError::InvalidOperation(format!(
                "frame is poisoned by a previous render failure and cannot render again: {reason}"
            )));
        }
        self.bindless_manager.flush_argument_buffer();
        match self.render_frame(frame, plan, pending) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.frame_poisoned = Some(format!("{e:?}"));
                Err(e)
            }
        }
    }
}
