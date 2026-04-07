use ash::vk;
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme},
};

use crate::error::RendererError;

use super::VulkanContext;

impl VulkanContext {
    pub fn allocate_buffer(
        &self,
        buffer_info: &vk::BufferCreateInfo,
        location: MemoryLocation,
    ) -> Result<(vk::Buffer, Allocation), RendererError> {
        let buffer = unsafe { self.device.create_buffer(buffer_info, None) }
            .map_err(|e| RendererError::VulkanError("Failed to create buffer".into(), e))?;
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let allocation_info = AllocationCreateDesc {
            name: "Buffer Allocation",
            requirements,
            location,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        };

        let mut allocator = self.allocator.borrow_mut();
        let allocation = allocator
            .allocate(&allocation_info)
            .map_err(|e| RendererError::from_allocation_error("buffer", e))?;

        unsafe {
            self.device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .map_err(|e| {
                    RendererError::VulkanError("Failed to bind buffer memory".into(), e)
                })?;
        }
        Ok((buffer, allocation))
    }

    /// Free a buffer and its allocation.
    pub(crate) fn free_buffer(&self, buffer: vk::Buffer, allocation: Allocation) {
        let mut allocator = self.allocator.borrow_mut();
        let _ = allocator.free(allocation);
        unsafe { self.device.destroy_buffer(buffer, None) };
    }

    /// Map a buffer allocation to host memory.
    /// Currently maps the entire buffer; partial mapping could be added as an optimization.
    pub fn map_buffer(&self, allocation: &Allocation) -> Result<*mut u8, RendererError> {
        allocation
            .mapped_ptr()
            .map(|ptr| ptr.cast().as_ptr())
            .ok_or_else(|| RendererError::InvalidOperation("Buffer is not mapped".to_string()))
    }

    /// Flush mapped memory ranges to make CPU writes visible to the GPU.
    ///
    /// This is required for non-coherent memory types. For coherent memory, this is a no-op.
    ///
    /// The offset and size are automatically aligned to `non_coherent_atom_size` as required
    /// by the Vulkan specification. The actual flushed range may be slightly larger than
    /// requested to ensure proper alignment.
    pub fn flush_mapped_memory(
        &self,
        allocation: &Allocation,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
    ) -> Result<(), RendererError> {
        let base_memory_offset = allocation.offset() + offset;

        let aligned_memory_offset = base_memory_offset & !(self.non_coherent_atom_size - 1);

        let end = base_memory_offset + size;

        let aligned_size = if size == vk::WHOLE_SIZE {
            vk::WHOLE_SIZE
        } else {
            let size_needed = end - aligned_memory_offset;
            (size_needed + self.non_coherent_atom_size - 1) & !(self.non_coherent_atom_size - 1)
        };

        unsafe {
            let memory = allocation.memory();
            let flush_range = vk::MappedMemoryRange::default()
                .memory(memory)
                .offset(aligned_memory_offset)
                .size(aligned_size);

            self.device
                .flush_mapped_memory_ranges(&[flush_range])
                .map_err(|e| {
                    RendererError::VulkanError("Failed to flush mapped memory".into(), e)
                })?;
        }
        Ok(())
    }

    /// Invalidate mapped memory ranges to make GPU writes visible to CPU reads.
    ///
    /// Must be called after a GPU write (e.g., compute shader atomic operations,
    /// vkCmdFillBuffer, vkCmdCopyBuffer) before reading the mapped memory on CPU.
    ///
    /// The offset and size are automatically aligned to `non_coherent_atom_size` as required
    /// by the Vulkan specification.
    pub fn invalidate_mapped_memory(
        &self,
        allocation: &Allocation,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
    ) -> Result<(), RendererError> {
        let base_memory_offset = allocation.offset() + offset;
        let aligned_memory_offset = base_memory_offset & !(self.non_coherent_atom_size - 1);

        let aligned_size = if size == vk::WHOLE_SIZE {
            vk::WHOLE_SIZE
        } else {
            let end = base_memory_offset + size;
            let size_needed = end - aligned_memory_offset;
            (size_needed + self.non_coherent_atom_size - 1) & !(self.non_coherent_atom_size - 1)
        };

        unsafe {
            let memory = allocation.memory();
            let range = vk::MappedMemoryRange::default()
                .memory(memory)
                .offset(aligned_memory_offset)
                .size(aligned_size);

            self.device
                .invalidate_mapped_memory_ranges(&[range])
                .map_err(|e| {
                    RendererError::VulkanError("Failed to invalidate mapped memory".into(), e)
                })?;
        }
        Ok(())
    }

    pub fn create_image(
        &self,
        image_create_info: vk::ImageCreateInfo,
        location: MemoryLocation,
    ) -> Result<(vk::Image, Allocation), RendererError> {
        let image = unsafe { self.device.create_image(&image_create_info, None) }
            .map_err(|e| RendererError::VulkanError("Failed to create image".into(), e))?;
        let requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let allocation_info = AllocationCreateDesc {
            name: "Image Allocation",
            requirements,
            location,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        };

        let mut allocator = self.allocator.borrow_mut();
        let allocation = allocator
            .allocate(&allocation_info)
            .map_err(|e| RendererError::from_allocation_error("image", e))?;

        unsafe {
            self.device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .map_err(|e| {
                    RendererError::VulkanError("Failed to bind image memory".into(), e)
                })?;
        }
        Ok((image, allocation))
    }

    /// Free an image and its allocation.
    /// Uses wrapper type to avoid exposing vk::Image in public API.
    pub(crate) fn free_image(&self, image: crate::sync::VkImage, allocation: Allocation) {
        let mut allocator = self.allocator.borrow_mut();
        let _ = allocator.free(allocation);
        unsafe {
            self.device.destroy_image(image.vk(), None);
        }
    }
}
