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
    pub(crate) allocation: Option<Allocation>,
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

    pub(crate) fn read_range(
        &self,
        range: super::BufferByteRange,
    ) -> Result<Vec<u8>, crate::RendererError> {
        if self.desc.memory != super::BufferMemoryPolicy::Readback
            || range
                .offset
                .checked_add(range.size)
                .is_none_or(|end| end > self.desc.size)
        {
            return Err(crate::RendererError::InvalidOperation(
                "Readback requires a valid range in an explicit readback allocation".into(),
            ));
        }
        let allocation = self.allocation.as_ref().ok_or_else(|| {
            crate::RendererError::InvalidOperation("Readback allocation unavailable".into())
        })?;
        self.context
            .invalidate_mapped_memory(allocation, range.offset, range.size)?;
        let pointer = self.context.map_buffer(allocation)?;
        Ok(unsafe {
            std::slice::from_raw_parts(pointer.add(range.offset as usize), range.size as usize)
        }
        .to_vec())
    }

    pub(crate) fn write(&self, offset: u64, data: &[u8]) -> Result<(), crate::RendererError> {
        if self.desc.memory != super::BufferMemoryPolicy::CpuVisible
            || offset
                .checked_add(data.len() as u64)
                .is_none_or(|end| end > self.desc.size)
        {
            return Err(crate::RendererError::InvalidOperation(
                "Buffer write requires a CPU-visible allocation and a valid byte range".into(),
            ));
        }
        let allocation = self.allocation.as_ref().ok_or_else(|| {
            crate::RendererError::InvalidOperation("Cannot write a borrowed allocation".into())
        })?;
        let pointer = self.context.map_buffer(allocation)?;
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), pointer.add(offset as usize), data.len());
        }
        self.context
            .flush_mapped_memory(allocation, offset, data.len() as u64)?;
        Ok(())
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
