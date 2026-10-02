use std::rc::Rc;

use crate::vulkan::context::VulkanContext;
use ash::vk;
use gpu_allocator::vulkan::Allocation;

use super::resource::BufferDesc;

/// Vulkan buffer allocation owned by a frame graph.
pub struct VulkanGraphBuffer {
    context: Rc<VulkanContext>,
    pub(crate) buffer: vk::Buffer,
    pub(crate) offset: u64,
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
            offset: 0,
            allocation: Some(allocation),
            desc,
        }
    }

    pub(crate) fn borrowed(
        context: Rc<VulkanContext>,
        buffer: vk::Buffer,
        offset: u64,
        desc: BufferDesc,
    ) -> Self {
        Self {
            context,
            buffer,
            offset,
            allocation: None,
            desc,
        }
    }

    pub(crate) fn size(&self) -> u64 {
        self.desc.size
    }

    #[cfg(all(test, not(target_os = "macos")))]
    pub(crate) fn read_completed(&self) -> Result<Vec<u8>, crate::RendererError> {
        let allocation = self.allocation.as_ref().ok_or_else(|| {
            crate::RendererError::InvalidOperation(
                "Readback requires an owned mapped graph buffer".into(),
            )
        })?;
        self.context
            .invalidate_mapped_memory(allocation, 0, self.desc.size)?;
        let pointer = self.context.map_buffer(allocation)?;
        Ok(unsafe { std::slice::from_raw_parts(pointer, self.desc.size as usize) }.to_vec())
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
