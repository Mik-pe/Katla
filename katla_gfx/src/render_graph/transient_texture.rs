use std::cell::Cell;
use std::rc::Rc;

use crate::render_graph::resource::{ResourceState, TransientTextureOps};
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
    current_layout: Cell<vk::ImageLayout>,
    /// Current resource state (semantic usage) for barrier decision-making.
    state: Cell<ResourceState>,
}

impl TransientTextureOps for TransientTexture {
    fn state(&self) -> ResourceState {
        self.state.get()
    }

    fn set_state(&self, new_state: ResourceState) {
        self.state.set(new_state);
    }
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
            current_layout: Cell::new(vk::ImageLayout::UNDEFINED),
            state: Cell::new(ResourceState::Undefined),
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

    /// Get the current resource state (delegates to TransientTextureOps).
    pub fn state(&self) -> ResourceState {
        <Self as TransientTextureOps>::state(self)
    }

    /// Update the tracked resource state after a transition (delegates to TransientTextureOps).
    pub(crate) fn set_state(&self, new_state: ResourceState) {
        <Self as TransientTextureOps>::set_state(self, new_state);
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
