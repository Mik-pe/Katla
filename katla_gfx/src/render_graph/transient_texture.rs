use std::cell::Cell;
use std::rc::Rc;

use crate::sync::VkImageView;
use crate::vulkan::context::VulkanContext;
use ash::vk;
use gpu_allocator::vulkan::Allocation;

/// Transient texture created and managed by the frame graph.
pub struct TransientTexture {
    /// Vulkan context for cleanup.
    context: Rc<VulkanContext>,
    /// Vulkan image handle.
    pub image: vk::Image,
    /// Memory allocation for the image.
    pub allocation: Option<Allocation>,
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
    ///
    /// This is CRITICAL for correct synchronization. Using the wrong old_layout
    /// in a barrier causes undefined behavior, including black screens.
    ///
    /// Uses Cell for interior mutability so layout can be updated during
    /// frame execution even though Frame only has an immutable borrow of FrameGraph.
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
            image_view,
            format,
            extent,
            bindless_slot: None,
            // Images are created with UNDEFINED layout
            current_layout: Cell::new(vk::ImageLayout::UNDEFINED),
        }
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
        }
    }
}
