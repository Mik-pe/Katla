use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::ImageSubresourceRange;
use crate::sync::VkImageView;
use crate::vulkan::context::VulkanContext;
use ash::vk;
use gpu_allocator::vulkan::Allocation;

struct ImageLayoutSnapshot {
    tracker: Rc<RefCell<ImageLayoutTracker>>,
    current: Rc<Cell<vk::ImageLayout>>,
    before: ImageLayoutTracker,
    before_current: vk::ImageLayout,
}

/// Reverses encoded image state changes when their command buffer is abandoned.
#[derive(Default)]
pub(crate) struct ImageLayoutJournal {
    snapshots: Vec<ImageLayoutSnapshot>,
    identities: std::collections::BTreeSet<usize>,
}

impl ImageLayoutJournal {
    pub(crate) fn record(&mut self, texture: &TransientTexture) {
        self.record_tracker(&texture.layouts, &texture.current_layout);
    }

    fn record_tracker(
        &mut self,
        tracker: &Rc<RefCell<ImageLayoutTracker>>,
        current: &Rc<Cell<vk::ImageLayout>>,
    ) {
        if self.identities.insert(Rc::as_ptr(tracker) as usize) {
            self.snapshots.push(ImageLayoutSnapshot {
                tracker: tracker.clone(),
                current: current.clone(),
                before: tracker.borrow().clone(),
                before_current: current.get(),
            });
        }
    }

    pub(crate) fn rollback(&mut self) {
        for snapshot in self.snapshots.drain(..) {
            *snapshot.tracker.borrow_mut() = snapshot.before;
            snapshot.current.set(snapshot.before_current);
        }
        self.identities.clear();
    }

    pub(crate) fn commit(&mut self) {
        self.snapshots.clear();
        self.identities.clear();
    }
}

/// Device memory shared by every transient texture aliased into one
/// physical allocation slot.
///
/// Member images bind this memory at offset zero and are created with
/// `ALIAS`, so their contents are undefined whenever execution crosses
/// from one member's live interval into another's — the same discard
/// semantics a fresh allocation has. The memory is freed when the last
/// member texture is destroyed.
pub(crate) struct VkSlotMemory {
    context: Rc<VulkanContext>,
    memory: vk::DeviceMemory,
    bytes: u64,
    lazily_allocated: bool,
    frame_slot: usize,
    allocation_slot: u32,
}

impl VkSlotMemory {
    pub(crate) fn new(
        context: Rc<VulkanContext>,
        memory: vk::DeviceMemory,
        bytes: u64,
        lazily_allocated: bool,
        frame_slot: usize,
        allocation_slot: u32,
    ) -> Self {
        Self {
            context,
            memory,
            bytes,
            lazily_allocated,
            frame_slot,
            allocation_slot,
        }
    }

    pub(crate) fn frame_slot(&self) -> usize {
        self.frame_slot
    }
    pub(crate) fn allocation_slot(&self) -> u32 {
        self.allocation_slot
    }

    pub(crate) fn memory(&self) -> vk::DeviceMemory {
        self.memory
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn lazily_allocated(&self) -> bool {
        self.lazily_allocated
    }
}

impl Drop for VkSlotMemory {
    fn drop(&mut self) {
        unsafe {
            self.context.device.free_memory(self.memory, None);
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ImageLayoutTracker {
    pieces: Vec<(ImageSubresourceRange, vk::ImageLayout)>,
}

impl ImageLayoutTracker {
    pub(crate) fn ranges(
        &self,
        range: ImageSubresourceRange,
        initial: vk::ImageLayout,
    ) -> Vec<(ImageSubresourceRange, vk::ImageLayout)> {
        let mut result = Vec::new();
        let mut remainder = vec![range];
        for &(piece, layout) in &self.pieces {
            if let Some(overlap) = piece.intersection(range) {
                result.push((overlap, layout));
                remainder = remainder
                    .iter()
                    .flat_map(|range| range.subtract(piece))
                    .collect();
            }
        }
        result.extend(remainder.into_iter().map(|range| (range, initial)));
        result
    }

    pub(crate) fn set(&mut self, range: ImageSubresourceRange, layout: vk::ImageLayout) {
        self.pieces = self
            .pieces
            .iter()
            .flat_map(|&(piece, old)| {
                piece
                    .subtract(range)
                    .into_iter()
                    .map(move |piece| (piece, old))
            })
            .collect();
        self.pieces.push((range, layout));
    }
}

/// Transient texture created and managed by the frame graph.
#[derive(Clone)]
pub struct TransientTexture {
    /// Vulkan context for cleanup.
    owner: Rc<TransientTextureOwner>,
    /// Vulkan image handle.
    pub image: vk::Image,
    /// Standalone memory allocation; `None` when the texture is aliased
    /// into a physical slot owned by [`VkSlotMemory`].
    /// Shared slot memory this texture is aliased into, if any.
    slot_memory: Option<Rc<VkSlotMemory>>,
    /// Image view for rendering/sampling.
    pub image_view: VkImageView,
    /// Image format.
    pub format: vk::Format,
    /// Image extent.
    pub extent: vk::Extent2D,
    /// Bindless texture slot (if registered with bindless system).
    /// This is used to update the descriptor when the texture is recreated.
    pub(super) bindless_slot: Option<u32>,
    /// Current GPU layout - tracked to ensure correct barrier old_layout.
    current_layout: Rc<Cell<vk::ImageLayout>>,
    pub(crate) layouts: Rc<RefCell<ImageLayoutTracker>>,
}

impl TransientTexture {
    /// Create a new transient texture.
    pub(crate) fn new(
        context: Rc<VulkanContext>,
        image: vk::Image,
        allocation: Option<Allocation>,
        image_view: VkImageView,
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> Self {
        Self {
            owner: Rc::new(TransientTextureOwner {
                context,
                image,
                image_view,
                allocation,
                slot_memory: RefCell::new(None),
            }),
            image,
            slot_memory: None,
            image_view,
            format,
            extent,
            bindless_slot: None,
            current_layout: Rc::new(Cell::new(vk::ImageLayout::UNDEFINED)),
            layouts: Rc::new(RefCell::new(ImageLayoutTracker::default())),
        }
    }

    /// Alias this texture into a physical allocation slot.
    ///
    /// The texture owns a reference to the shared memory; the image itself
    /// must already be bound to it.
    pub(crate) fn set_slot_memory(&mut self, slot_memory: Rc<VkSlotMemory>) {
        *self.owner.slot_memory.borrow_mut() = Some(slot_memory.clone());
        self.slot_memory = Some(slot_memory);
    }

    /// Shared slot memory backing this texture, when aliased.
    pub(crate) fn slot_memory(&self) -> Option<&VkSlotMemory> {
        self.slot_memory.as_deref()
    }

    /// Get the current tracked GPU layout.
    pub fn current_layout(&self) -> vk::ImageLayout {
        self.current_layout.get()
    }

    pub(crate) fn set_range_layout(&self, range: ImageSubresourceRange, layout: vk::ImageLayout) {
        self.current_layout.set(layout);
        self.layouts.borrow_mut().set(range, layout);
    }

    /// Get the raw Vulkan image view handle.
    pub fn image_view_vk(&self) -> vk::ImageView {
        self.image_view.vk()
    }
}

struct TransientTextureOwner {
    context: Rc<VulkanContext>,
    image: vk::Image,
    image_view: VkImageView,
    allocation: Option<Allocation>,
    slot_memory: RefCell<Option<Rc<VkSlotMemory>>>,
}

impl Drop for TransientTextureOwner {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_image_view(self.image_view.vk(), None);
            self.context.device.destroy_image(self.image, None);
            if let Some(allocation) = self.allocation.take() {
                self.context.allocator.free(allocation, "transient texture");
            }
        }
    }
}

impl TransientTexture {
    pub(crate) fn allocation(&self) -> Option<&Allocation> {
        self.owner.allocation.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::ImageAspects;

    #[test]
    fn test_disjoint_subresources_keep_distinct_native_layouts() {
        let mip0 = ImageSubresourceRange::new(ImageAspects::COLOR, 0, 1, 0, 1);
        let mip1 = ImageSubresourceRange::new(ImageAspects::COLOR, 1, 1, 0, 1);
        let mut tracker = ImageLayoutTracker::default();
        tracker.set(mip0, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        tracker.set(mip1, vk::ImageLayout::TRANSFER_DST_OPTIMAL);
        assert_eq!(
            tracker.ranges(mip0, vk::ImageLayout::UNDEFINED),
            vec![(mip0, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)]
        );
        assert_eq!(
            tracker.ranges(mip1, vk::ImageLayout::UNDEFINED),
            vec![(mip1, vk::ImageLayout::TRANSFER_DST_OPTIMAL)]
        );
        tracker.set(mip0, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        assert_eq!(
            tracker.ranges(mip1, vk::ImageLayout::UNDEFINED),
            vec![(mip1, vk::ImageLayout::TRANSFER_DST_OPTIMAL)]
        );
    }

    #[test]
    fn test_whole_range_transition_partitions_its_source_layouts() {
        let mip0 = ImageSubresourceRange::new(ImageAspects::COLOR, 0, 1, 0, 1);
        let both = ImageSubresourceRange::new(ImageAspects::COLOR, 0, 2, 0, 1);
        let mut tracker = ImageLayoutTracker::default();
        tracker.set(mip0, vk::ImageLayout::GENERAL);
        let ranges = tracker.ranges(both, vk::ImageLayout::UNDEFINED);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0], (mip0, vk::ImageLayout::GENERAL));
        assert_eq!(ranges[1].0.base_mip_level, 1);
        assert_eq!(ranges[1].1, vk::ImageLayout::UNDEFINED);
    }
}

#[cfg(test)]
mod journal_tests {
    use super::*;

    #[test]
    fn test_aborted_range_transitions_restore_the_actual_layout() {
        let tracker = Rc::new(RefCell::new(ImageLayoutTracker::default()));
        let current = Rc::new(Cell::new(vk::ImageLayout::UNDEFINED));
        let range = ImageSubresourceRange::WHOLE_COLOR;
        let mut journal = ImageLayoutJournal::default();
        journal.record_tracker(&tracker, &current);
        tracker.borrow_mut().set(range, vk::ImageLayout::GENERAL);
        current.set(vk::ImageLayout::GENERAL);
        journal.record_tracker(&tracker, &current);
        tracker
            .borrow_mut()
            .set(range, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        journal.rollback();
        assert_eq!(
            tracker.borrow().ranges(range, vk::ImageLayout::UNDEFINED)[0].1,
            vk::ImageLayout::UNDEFINED
        );
        assert_eq!(current.get(), vk::ImageLayout::UNDEFINED);
        journal.record_tracker(&tracker, &current);
        tracker.borrow_mut().set(range, vk::ImageLayout::GENERAL);
        current.set(vk::ImageLayout::GENERAL);
        journal.commit();
        journal.rollback();
        assert_eq!(
            tracker.borrow().ranges(range, vk::ImageLayout::UNDEFINED)[0].1,
            vk::ImageLayout::GENERAL
        );
    }
}
