use std::rc::Rc;

use crate::vulkan::context::VulkanContext;
use ash::vk;
use gpu_allocator::vulkan::Allocation;

use super::resource::BufferDesc;

/// Vulkan buffer allocation owned by a frame graph.
pub struct VulkanGraphBuffer {
    context: Rc<VulkanContext>,
    pub(crate) buffer: vk::Buffer,
    allocation: Option<Allocation>,
    pub(crate) desc: BufferDesc,
}

impl VulkanGraphBuffer {
    pub(crate) fn new(
        context: Rc<VulkanContext>,
        buffer: vk::Buffer,
        allocation: Allocation,
        desc: BufferDesc,
    ) -> Self {
        Self {
            context,
            buffer,
            allocation: Some(allocation),
            desc,
        }
    }

    pub(crate) fn size(&self) -> u64 {
        self.desc.size
    }

    pub fn vk_buffer(&self) -> vk::Buffer {
        self.buffer
    }
}

impl Drop for VulkanGraphBuffer {
    fn drop(&mut self) {
        if let Some(allocation) = self.allocation.take() {
            self.context.free_buffer(self.buffer, allocation);
        }
    }
}
