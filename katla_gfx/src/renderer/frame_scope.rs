//! Frame-scoped rendering API.
//!
//! Rendering one frame means owning one frame token. [`GpuRenderer::acquire_frame`](crate::renderer::gpu_renderer::GpuRenderer::acquire_frame)
//! returns a [`FrameAcquisition`]: either a [`FrameToken`] owning one reusable frame
//! slot (and, when windowed, one surface image), an explicitly unavailable surface,
//! or an out-of-date surface that must be recreated. Every frame-local operation —
//! buffer writes, per-object data and graph execution — takes the token, so writes cannot be issued against a frame that was never acquired, has
//! already been presented, or belongs to an abandoned acquisition. Successful
//! rendering freezes frame-local writes; present commits the pending work once.
//!
//! `acquire_frame` waits until the returned slot's previous submission has completed
//! before handing out the token, so frame-local writes can never race a slot still
//! in flight.
//!
//! Finishing a frame consumes the token through [`GpuRenderer::present`](crate::renderer::gpu_renderer::GpuRenderer::present) (submit +
//! present, one documented semantic across Vulkan and Metal). [`GpuRenderer::abort`](crate::renderer::gpu_renderer::GpuRenderer::abort)
//! abandons it: nothing is submitted or presented, the slot is left completed-safe,
//! and the next acquire reuses it normally. Acquiring again while a frame is still
//! open abandons that frame the same way — an abandoned frame can never strand a slot.

/// Proof that one frame was acquired: the only key accepted by frame-local
/// renderer methods (see [`GpuRenderer`](crate::renderer::gpu_renderer::GpuRenderer)).
///
/// Copyable by design — the renderer rejects tokens that no longer match its
/// active frame (stale, already presented, or from an abandoned acquisition)
/// with a typed error instead of trusting the caller's ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameToken {
    slot: usize,
    acquisition: u64,
}

impl FrameToken {
    /// Create a unique acquisition identity for one reusable frame slot.
    ///
    /// Custom renderers retain the returned token as their active frame and
    /// validate exact equality before accepting frame operations. A new token
    /// cannot reproduce another renderer's acquisition identity.
    pub fn new(slot: usize) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_ACQUISITION: AtomicU64 = AtomicU64::new(1);
        let acquisition = NEXT_ACQUISITION
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("frame acquisition identities exhausted");
        Self { slot, acquisition }
    }

    /// The reusable frame slot this frame owns, in `0..renderer.frame_slot_count()`.
    ///
    /// Per-frame resources indexed by slot (storage buffers, particle buffers, …)
    /// are associated with the frame through this value.
    pub fn slot(&self) -> usize {
        self.slot
    }
}

/// Outcome of acquiring one frame.
#[derive(Debug)]
pub enum FrameAcquisition {
    /// A frame slot (and, when windowed, a surface image) is owned by the
    /// returned token. Render into it, then finish with
    /// [`GpuRenderer::present`](crate::renderer::gpu_renderer::GpuRenderer::present).
    Ready(FrameToken),
    /// The surface cannot produce a frame right now (minimized, occluded, or no
    /// drawable available this tick). No renderer state was touched; retry on a
    /// later iteration.
    Unavailable,
    /// The surface is stale and must be recreated (Vulkan swapchain out of date).
    /// No renderer state was touched; call the backend's resize path and acquire again.
    OutOfDate,
}

/// Surface status after a frame's GPU submission was accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceStatus {
    /// Presentation succeeded, or the renderer has no presentation surface.
    Presented,
    /// The submission was accepted, but the surface must be recreated before acquiring again.
    RecreateRequired,
}

/// An accepted GPU submission and its subsequent surface result.
///
/// Every returned outcome means the GPU work was committed. Callers must advance
/// CPU state associated with that work before handling surface recreation or
/// propagating a presentation error. An outer error from
/// [`GpuRenderer::present`](crate::GpuRenderer::present) means no submission was accepted.
#[derive(Debug)]
pub struct PresentOutcome {
    /// Presentation status, including errors that occurred after GPU submission.
    pub surface: Result<SurfaceStatus, crate::error::RendererError>,
}

impl PresentOutcome {
    /// An accepted submission whose presentation succeeded.
    pub fn presented() -> Self {
        Self {
            surface: Ok(SurfaceStatus::Presented),
        }
    }
}
