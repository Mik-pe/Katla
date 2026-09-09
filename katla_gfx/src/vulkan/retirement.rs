//! Deferred retirement of replaced native buffers.
//!
//! A buffer replaced mid-session (dynamic mesh growth) may still be read by
//! submissions that have not completed yet. Freeing it immediately is a
//! use-after-free on the GPU, so replaced buffers enter this queue tagged
//! with the frame counter at retirement and are freed only once every
//! submission from before their retirement is provably complete.
//!
//! Safety rule: frame slots are protected by one in-flight fence each, and
//! [`SwapData::wait_for_fence`](super::swapdata::SwapData::wait_for_fence)
//! is called once per frame before that slot is reused. Waiting at frame
//! counter `C` therefore completes the submission `FRAMES_IN_FLIGHT` frames
//! old. An entry retired at counter `R` (before frame `R` was submitted, so
//! referenced only by submissions `< R`) is safe to free once
//! `C - R >= FRAMES_IN_FLIGHT`.

use ash::vk;
use gpu_allocator::vulkan::Allocation;
use std::rc::Rc;

use super::context::VulkanContext;

/// A native buffer whose ownership moved out of a `BufferObject`.
///
/// Created via [`VertexBuffer::into_native_parts`] and
/// [`IndexBuffer::into_native_parts`](super::IndexBuffer::into_native_parts);
/// freeing it is the retirement queue's job.
pub(crate) struct RetiredBuffer {
    buffer: vk::Buffer,
    allocation: Option<Allocation>,
    context: Rc<VulkanContext>,
}

impl RetiredBuffer {
    pub(crate) fn new(
        buffer: vk::Buffer,
        allocation: Allocation,
        context: Rc<VulkanContext>,
    ) -> Self {
        Self {
            buffer,
            allocation: Some(allocation),
            context,
        }
    }

    fn free(&mut self) {
        if let Some(allocation) = self.allocation.take() {
            self.context.free_buffer(self.buffer, allocation);
        }
    }
}

impl Drop for RetiredBuffer {
    fn drop(&mut self) {
        // Defensive: entries should be drained after the device waits idle in
        // VulkanRenderer::destroy(). Anything left frees unconditionally — the
        // Rc on the context keeps the device alive until here.
        self.free();
    }
}

/// Queue of replaced buffers waiting for their last user to complete.
pub(crate) struct BufferRetirementQueue {
    entries: Vec<(u64, RetiredBuffer)>,
}

impl BufferRetirementQueue {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Queue a replaced buffer retired at frame counter `retired_at`.
    pub(crate) fn push(&mut self, buffer: RetiredBuffer, retired_at: u64) {
        self.entries.push((retired_at, buffer));
    }

    /// Free every entry whose pre-retirement submissions have all completed.
    ///
    /// Call after waiting on the current frame slot's fence (e.g. in
    /// `wait_for_frame`), passing the current monotonic frame counter.
    pub(crate) fn drain_completed(&mut self, current_frame: u64, frames_in_flight: usize) {
        let mut index = 0;
        while index < self.entries.len() {
            if retirement_expired(self.entries[index].0, current_frame, frames_in_flight) {
                let (_, mut entry) = self.entries.swap_remove(index);
                entry.free();
            } else {
                index += 1;
            }
        }
    }

    /// Free everything; only valid after a device-wide idle wait.
    pub(crate) fn drain_all(&mut self) {
        for (_, entry) in self.entries.iter_mut() {
            entry.free();
        }
        self.entries.clear();
    }

    /// Number of buffers still awaiting retirement (diagnostics and tests).
    pub(crate) fn pending(&self) -> usize {
        self.entries.len()
    }
}

impl Default for BufferRetirementQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// A retirement queue bound to one frame counter value.
///
/// Replacements during a single update all retire at the same frame; this
/// handle carries that frame so retirement call sites stay one-argument.
pub(crate) struct FrameRetirements<'a> {
    queue: &'a mut BufferRetirementQueue,
    frame: u64,
}

impl<'a> FrameRetirements<'a> {
    pub(crate) fn new(queue: &'a mut BufferRetirementQueue, frame: u64) -> Self {
        Self { queue, frame }
    }

    /// Queue one replaced buffer, tagged with this handle's frame.
    pub(crate) fn retire(&mut self, buffer: RetiredBuffer) {
        self.queue.push(buffer, self.frame);
    }
}

/// An entry retired at `retired_at` is expired once the frame counter has
/// advanced `frames_in_flight` frames past its retirement.
fn retirement_expired(retired_at: u64, current_frame: u64, frames_in_flight: usize) -> bool {
    current_frame.saturating_sub(retired_at) >= frames_in_flight as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retirement_expires_only_after_frames_in_flight_advance() {
        assert!(!retirement_expired(10, 10, 2));
        assert!(!retirement_expired(10, 11, 2));
        assert!(retirement_expired(10, 12, 2));
        assert!(retirement_expired(10, 13, 2));
    }

    #[test]
    fn test_retirement_never_expires_on_counter_saturating_wrap() {
        // u64 cannot wrap in practice; saturating subtraction keeps an
        // entry retired near u64::MAX pending forever rather than freeing
        // it early.
        assert!(!retirement_expired(u64::MAX - 1, 0, 2));
    }
}
