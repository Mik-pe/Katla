//! Frame-scoped rendering API.
//!
//! Rendering one frame means owning one frame token. [`GpuRenderer::acquire_frame`](crate::renderer::gpu_renderer::GpuRenderer::acquire_frame)
//! returns a [`FrameAcquisition`]: either a [`FrameToken`] owning one reusable frame
//! slot (and, when windowed, one surface image), an explicitly unavailable surface,
//! or an out-of-date surface that must be recreated. Every frame-local operation —
//! uniforms, per-object data, lights, shadow cascades, graph execution — takes that
//! token, so writes cannot be issued against a frame that was never acquired, has
//! already been presented, or belongs to an abandoned acquisition.
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
    generation: u64,
}

impl FrameToken {
    /// Create a token from a slot and generation counter.
    ///
    /// Public so alternative in-crate renderers (test mocks) can implement
    /// [`GpuRenderer`](crate::renderer::gpu_renderer::GpuRenderer). Constructing
    /// a token externally is harmless: the renderer only accepts the token that
    /// equals its open frame, and the generation counter is not observable.
    pub fn new(slot: usize, generation: u64) -> Self {
        Self { slot, generation }
    }

    /// The reusable frame slot this frame owns, in `0..FRAMES_IN_FLIGHT`.
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
