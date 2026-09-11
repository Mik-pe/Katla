use std::cell::Cell;
use std::rc::Rc;

use crate::sync::VkImageView;
use crate::vulkan::context::VulkanContext;
use ash::vk;
use gpu_allocator::vulkan::Allocation;

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
}

impl VkSlotMemory {
    pub(crate) fn new(
        context: Rc<VulkanContext>,
        memory: vk::DeviceMemory,
        bytes: u64,
        lazily_allocated: bool,
    ) -> Self {
        Self {
            context,
            memory,
            bytes,
            lazily_allocated,
        }
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

/// Transient texture created and managed by the frame graph.
pub struct TransientTexture {
    /// Vulkan context for cleanup.
    context: Rc<VulkanContext>,
    /// Vulkan image handle.
    pub image: vk::Image,
    /// Standalone memory allocation; `None` when the texture is aliased
    /// into a physical slot owned by [`VkSlotMemory`].
    pub allocation: Option<Allocation>,
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
    current_layout: Cell<vk::ImageLayout>,
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
            context,
            image,
            allocation,
            slot_memory: None,
            image_view,
            format,
            extent,
            bindless_slot: None,
            current_layout: Cell::new(vk::ImageLayout::UNDEFINED),
        }
    }

    /// Alias this texture into a physical allocation slot.
    ///
    /// The texture owns a reference to the shared memory; the image itself
    /// must already be bound to it.
    pub(crate) fn set_slot_memory(&mut self, slot_memory: Rc<VkSlotMemory>) {
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

    /// Update the tracked layout after a barrier transition.
    pub(crate) fn set_layout(&self, new_layout: vk::ImageLayout) {
        self.current_layout.set(new_layout);
    }

    /// Get the raw Vulkan image view handle.
    pub fn image_view_vk(&self) -> vk::ImageView {
        self.image_view.vk()
    }
}

impl Drop for TransientTexture {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_image_view(self.image_view.vk(), None);
            self.context.device.destroy_image(self.image, None);
            if let Some(allocation) = self.allocation.take() {
                self.context.allocator.free(allocation, "transient texture");
            }
            // Shared slot memory outlives every member image and is freed
            // when the last `Rc<VkSlotMemory>` drops.
        }
    }
}
