//! Deferred retirement of native GPU resources.
//!
//! A resource destroyed or replaced mid-session may still be referenced by
//! submissions that have not completed yet. Freeing it immediately is a
//! use-after-free on the GPU, so retired resources enter this queue tagged
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
use super::skeleton_buffer::SkeletonBuffer;
use super::texture::Texture;
use crate::renderer::registry::AnyPipeline;

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

    /// Native handle identifying this buffer in diagnostics.
    fn handle(&self) -> vk::Buffer {
        self.buffer
    }

    /// Device-memory bytes held by this buffer's allocation.
    fn allocation_bytes(&self) -> u64 {
        self.allocation.as_ref().map_or(0, |a| a.size())
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

/// A descriptor set layout destroyed with a material; raw handles need their
/// owning device to be destroyed, so the context is carried along.
pub(crate) struct RetiredDescriptorSetLayout {
    layout: Option<vk::DescriptorSetLayout>,
    context: Rc<VulkanContext>,
}

impl RetiredDescriptorSetLayout {
    pub(crate) fn new(layout: vk::DescriptorSetLayout, context: Rc<VulkanContext>) -> Self {
        Self {
            layout: Some(layout),
            context,
        }
    }

    /// Native handle identifying this layout in diagnostics.
    fn handle(&self) -> vk::DescriptorSetLayout {
        self.layout.unwrap_or(vk::DescriptorSetLayout::null())
    }
}

impl Drop for RetiredDescriptorSetLayout {
    fn drop(&mut self) {
        if let Some(layout) = self.layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
    }
}

/// A native GPU resource awaiting deferred destruction.
///
/// Every variant owns its resource: dropping it after the retirement age is
/// reached is the whole free protocol. `BindlessSlot` is the exception — a
/// slot index is freed by handing it back to
/// [`BindlessTextureManager::release_texture_slot`](super::bindless_texture::BindlessTextureManager::release_texture_slot),
/// so drains return expired slots to the caller instead of dropping them.
pub(crate) enum RetiredResource {
    /// Replaced or destroyed buffer (dynamic mesh growth, UI auto-grow,
    /// mesh destroy).
    Buffer(RetiredBuffer),
    /// Destroyed texture. Dropping the `Rc` frees the image, view, sampler,
    /// and allocation once the last reference is gone.
    Texture(Rc<Texture>),
    /// Replaced or destroyed graphics/compute pipeline (material destroy,
    /// hot reload, descriptor-layout invalidation). Boxed: pipelines are the
    /// largest variant by far.
    Pipeline(Box<AnyPipeline>),
    /// Standalone material descriptor set layout.
    DescriptorSetLayout(RetiredDescriptorSetLayout),
    /// Destroyed skeleton joint-matrix storage buffer.
    SkeletonBuffer(SkeletonBuffer),
    /// Bindless slot withheld from the free list while in-flight submissions
    /// can still resolve the old texture through it.
    BindlessSlot(u32),
}

impl RetiredResource {
    fn kind(&self) -> RetirementKind {
        match self {
            RetiredResource::Buffer(_) => RetirementKind::Buffer,
            RetiredResource::Texture(_) => RetirementKind::Texture,
            RetiredResource::Pipeline(_) => RetirementKind::Pipeline,
            RetiredResource::DescriptorSetLayout(_) => RetirementKind::DescriptorSetLayout,
            RetiredResource::SkeletonBuffer(_) => RetirementKind::SkeletonBuffer,
            RetiredResource::BindlessSlot(_) => RetirementKind::BindlessSlot,
        }
    }

    /// Native handle identity for retirement diagnostics.
    fn describe(&self) -> String {
        match self {
            RetiredResource::Buffer(buffer) => format!("buffer {:?}", buffer.handle()),
            RetiredResource::Texture(texture) => {
                format!("texture view {:?}", texture.image_view().vk())
            }
            RetiredResource::Pipeline(pipeline) => format!("pipeline {:?}", pipeline.vk_pipeline()),
            RetiredResource::DescriptorSetLayout(layout) => {
                format!("descriptor set layout {:?}", layout.handle())
            }
            RetiredResource::SkeletonBuffer(buffer) => {
                format!("skeleton buffer {:?}", buffer.buffer())
            }
            RetiredResource::BindlessSlot(slot) => format!("bindless slot {slot}"),
        }
    }

    /// Device-memory bytes held by this entry, where knowable.
    fn approximate_bytes(&self) -> Option<u64> {
        match self {
            RetiredResource::Buffer(buffer) => Some(buffer.allocation_bytes()),
            RetiredResource::SkeletonBuffer(buffer) => Some(buffer.size()),
            _ => None,
        }
    }
}

impl From<RetiredBuffer> for RetiredResource {
    fn from(buffer: RetiredBuffer) -> Self {
        RetiredResource::Buffer(buffer)
    }
}

impl From<Rc<Texture>> for RetiredResource {
    fn from(texture: Rc<Texture>) -> Self {
        RetiredResource::Texture(texture)
    }
}

impl From<AnyPipeline> for RetiredResource {
    fn from(pipeline: AnyPipeline) -> Self {
        RetiredResource::Pipeline(Box::new(pipeline))
    }
}

impl From<RetiredDescriptorSetLayout> for RetiredResource {
    fn from(layout: RetiredDescriptorSetLayout) -> Self {
        RetiredResource::DescriptorSetLayout(layout)
    }
}

impl From<SkeletonBuffer> for RetiredResource {
    fn from(buffer: SkeletonBuffer) -> Self {
        RetiredResource::SkeletonBuffer(buffer)
    }
}

/// Resource categories tracked in [`RetirementSnapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetirementKind {
    Buffer,
    Texture,
    Pipeline,
    DescriptorSetLayout,
    SkeletonBuffer,
    BindlessSlot,
}

impl RetirementKind {
    fn label(self) -> &'static str {
        match self {
            RetirementKind::Buffer => "buffers",
            RetirementKind::Texture => "textures",
            RetirementKind::Pipeline => "pipelines",
            RetirementKind::DescriptorSetLayout => "descriptor-set-layouts",
            RetirementKind::SkeletonBuffer => "skeleton-buffers",
            RetirementKind::BindlessSlot => "bindless-slots",
        }
    }
}

/// Diagnostics snapshot of a retirement queue.
///
/// `oldest_retired_at` is the frame counter of the oldest pending entry; a
/// pending entry whose age (`current_frame - oldest_retired_at`) far exceeds
/// the frames-in-flight count indicates retirement draining has stopped
/// (e.g. rendering ceased) rather than a leak.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetirementSnapshot {
    /// Pending replaced/destroyed vertex, index, and UI buffers.
    pub buffers: usize,
    /// Pending destroyed textures.
    pub textures: usize,
    /// Pending replaced/destroyed pipelines.
    pub pipelines: usize,
    /// Pending standalone descriptor set layouts.
    pub descriptor_set_layouts: usize,
    /// Pending skeleton joint-matrix buffers.
    pub skeleton_buffers: usize,
    /// Pending bindless slot releases.
    pub bindless_slots: usize,
    /// Device-memory bytes held by pending entries that know their size
    /// (buffers and skeleton buffers; textures and pipelines do not).
    pub pending_bytes: u64,
    /// Frame counter at which the oldest pending entry was retired.
    pub oldest_retired_at: Option<u64>,
}

impl RetirementSnapshot {
    /// Total number of pending retirement entries.
    pub fn total(&self) -> usize {
        self.buffers
            + self.textures
            + self.pipelines
            + self.descriptor_set_layouts
            + self.skeleton_buffers
            + self.bindless_slots
    }

    /// Human-readable per-kind summary for logs and diagnostics.
    pub fn summary(&self) -> String {
        let mut out = format!("pending retirements: {}", self.total());
        for (count, kind) in [
            (self.buffers, RetirementKind::Buffer),
            (self.textures, RetirementKind::Texture),
            (self.pipelines, RetirementKind::Pipeline),
            (
                self.descriptor_set_layouts,
                RetirementKind::DescriptorSetLayout,
            ),
            (self.skeleton_buffers, RetirementKind::SkeletonBuffer),
            (self.bindless_slots, RetirementKind::BindlessSlot),
        ] {
            if count > 0 {
                out.push_str(&format!(" {}={}", kind.label(), count));
            }
        }
        if self.pending_bytes > 0 {
            out.push_str(&format!(" bytes={}", self.pending_bytes));
        }
        if let Some(retired_at) = self.oldest_retired_at {
            out.push_str(&format!(" oldest=frame:{retired_at}"));
        }
        out
    }
}

/// Queue of retired resources waiting for their last user to complete.
pub(crate) struct RetirementQueue {
    entries: Vec<(u64, RetiredResource)>,
}

impl RetirementQueue {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Queue a resource retired at frame counter `retired_at`.
    pub(crate) fn push(&mut self, resource: RetiredResource, retired_at: u64) {
        self.entries.push((retired_at, resource));
    }

    /// Free every entry whose pre-retirement submissions have all completed.
    ///
    /// Call after waiting on the current frame slot's fence (e.g. in
    /// `wait_for_frame`), passing the current monotonic frame counter.
    /// Expired bindless slots are returned instead of freed: only the
    /// owning `BindlessTextureManager` can put a slot back on its free list.
    pub(crate) fn drain_completed(
        &mut self,
        current_frame: u64,
        frames_in_flight: usize,
    ) -> Vec<u32> {
        let mut expired_slots = Vec::new();
        let mut index = 0;
        while index < self.entries.len() {
            if retirement_expired(self.entries[index].0, current_frame, frames_in_flight) {
                let (_, entry) = self.entries.swap_remove(index);
                log::debug!("retiring {}", entry.describe());
                match entry {
                    RetiredResource::BindlessSlot(slot) => expired_slots.push(slot),
                    expired => drop(expired),
                }
            } else {
                index += 1;
            }
        }
        expired_slots
    }

    /// Free everything; only valid after a device-wide idle wait.
    pub(crate) fn drain_all(&mut self) -> Vec<u32> {
        let mut expired_slots = Vec::new();
        for (_, entry) in std::mem::take(&mut self.entries) {
            log::debug!("retiring {}", entry.describe());
            match entry {
                RetiredResource::BindlessSlot(slot) => expired_slots.push(slot),
                expired => drop(expired),
            }
        }
        expired_slots
    }

    /// Per-kind pending counts plus the oldest pending retirement frame.
    pub(crate) fn snapshot(&self) -> RetirementSnapshot {
        let mut out = RetirementSnapshot::default();
        for (retired_at, resource) in &self.entries {
            match resource.kind() {
                RetirementKind::Buffer => out.buffers += 1,
                RetirementKind::Texture => out.textures += 1,
                RetirementKind::Pipeline => out.pipelines += 1,
                RetirementKind::DescriptorSetLayout => out.descriptor_set_layouts += 1,
                RetirementKind::SkeletonBuffer => out.skeleton_buffers += 1,
                RetirementKind::BindlessSlot => out.bindless_slots += 1,
            }
            if let Some(bytes) = resource.approximate_bytes() {
                out.pending_bytes += bytes;
            }
            out.oldest_retired_at = Some(match out.oldest_retired_at {
                Some(oldest) => oldest.min(*retired_at),
                None => *retired_at,
            });
        }
        out
    }
}

impl Default for RetirementQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// A retirement queue bound to one frame counter value.
///
/// Replacements during a single update all retire at the same frame; this
/// handle carries that frame so retirement call sites stay one-argument.
pub(crate) struct FrameRetirements<'a> {
    queue: &'a mut RetirementQueue,
    frame: u64,
}

impl<'a> FrameRetirements<'a> {
    pub(crate) fn new(queue: &'a mut RetirementQueue, frame: u64) -> Self {
        Self { queue, frame }
    }

    /// Queue one retired resource, tagged with this handle's frame.
    pub(crate) fn retire(&mut self, resource: impl Into<RetiredResource>) {
        self.queue.push(resource.into(), self.frame);
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

    #[test]
    fn test_empty_queue_snapshot_and_drain() {
        let mut queue = RetirementQueue::new();
        assert_eq!(queue.snapshot().total(), 0);
        assert_eq!(queue.snapshot(), RetirementSnapshot::default());
        assert!(queue.drain_completed(100, 2).is_empty());
        assert!(queue.drain_all().is_empty());
    }

    #[test]
    fn test_drain_returns_expired_bindless_slots() {
        let mut queue = RetirementQueue::new();
        queue.push(RetiredResource::BindlessSlot(7), 10);
        queue.push(RetiredResource::BindlessSlot(8), 11);

        let expired = queue.drain_completed(11, 2);
        assert_eq!(expired, Vec::<u32>::new());
        assert_eq!(queue.snapshot().total(), 2);

        let expired = queue.drain_completed(12, 2);
        assert_eq!(expired, vec![7]);
        assert_eq!(queue.snapshot().total(), 1);

        let expired = queue.drain_completed(13, 2);
        assert_eq!(expired, vec![8]);
        assert_eq!(queue.snapshot().total(), 0);
    }

    #[test]
    fn test_drain_all_returns_every_bindless_slot() {
        let mut queue = RetirementQueue::new();
        queue.push(RetiredResource::BindlessSlot(7), 10);
        queue.push(RetiredResource::BindlessSlot(8), 11);
        let expired = queue.drain_all();
        assert_eq!(expired, vec![7, 8]);
        assert_eq!(queue.snapshot().total(), 0);
    }

    #[test]
    fn test_snapshot_counts_kinds_and_oldest_frame() {
        let mut queue = RetirementQueue::new();
        queue.push(RetiredResource::BindlessSlot(7), 30);
        queue.push(RetiredResource::BindlessSlot(8), 20);
        let snapshot = queue.snapshot();
        assert_eq!(snapshot.bindless_slots, 2);
        assert_eq!(snapshot.total(), 2);
        assert_eq!(snapshot.oldest_retired_at, Some(20));
        assert!(snapshot.summary().contains("bindless-slots=2"));
        assert!(snapshot.summary().contains("oldest=frame:20"));
    }
}
