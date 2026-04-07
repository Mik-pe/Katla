use std::cell::{RefCell, RefMut};
use std::mem::ManuallyDrop;

use ash::vk;
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator},
};

use crate::error::RendererError;

use super::VulkanContext;

/// Wrapper around `ManuallyDrop<RefCell<Allocator>>` that provides safe
/// allocation/deallocation methods which log warnings on borrow conflicts
/// instead of panicking or silently leaking memory.
pub struct GpuAllocator {
    inner: ManuallyDrop<RefCell<Allocator>>,
}

impl GpuAllocator {
    pub(crate) fn new(allocator: Allocator) -> Self {
        Self {
            inner: ManuallyDrop::new(RefCell::new(allocator)),
        }
    }

    /// Allocate GPU memory via the inner allocator.
    ///
    /// Returns `Err` on both allocation failure and borrow conflict.
    /// Borrow conflicts are logged as warnings — they indicate a re-entrant
    /// call (e.g. freeing memory during a Drop while the allocator is borrowed
    /// for an allocation).
    pub fn allocate(
        &self,
        desc: &AllocationCreateDesc,
        context: &str,
    ) -> Result<Allocation, RendererError> {
        let mut allocator = self.try_borrow(context)?;
        allocator
            .allocate(desc)
            .map_err(|e| RendererError::from_allocation_error(context, e))
    }

    /// Free a GPU memory allocation.
    ///
    /// Logs a warning on borrow conflict instead of panicking.
    /// Returns `Ok(())` even if the free itself fails (non-fatal).
    pub fn free(&self, allocation: Allocation, context: &str) {
        let offset = allocation.offset();
        match self.inner.try_borrow_mut() {
            Ok(mut allocator) => {
                if let Err(e) = allocator.free(allocation) {
                    log::warn!(
                        "Failed to free {} allocation at offset {:?}: {:?}",
                        context,
                        offset,
                        e
                    );
                }
            }
            Err(_) => {
                log::warn!(
                    "GpuAllocator borrow conflict during {} free — \
                     allocator is already borrowed, memory at offset {:?} may leak",
                    context,
                    offset
                );
            }
        }
    }

    /// Borrow the allocator mutably for direct operations.
    ///
    /// Logs a warning on borrow conflict and returns a `RendererError`.
    pub fn try_borrow(&self, context: &str) -> Result<RefMut<'_, Allocator>, RendererError> {
        self.inner.try_borrow_mut().map_err(|_| {
            log::warn!(
                "GpuAllocator borrow conflict during {} — \
                 allocator is already borrowed",
                context
            );
            RendererError::InvalidOperation(format!(
                "GpuAllocator borrow conflict during {}",
                context
            ))
        })
    }

    /// Borrow the allocator mutably, returning a `String` error on conflict.
    ///
    /// Used by internal helpers that propagate `String` errors.
    pub(crate) fn try_borrow_mut_string(
        &self,
        context: &str,
    ) -> Result<RefMut<'_, Allocator>, String> {
        self.inner.try_borrow_mut().map_err(|_| {
            log::warn!(
                "GpuAllocator borrow conflict during {} — \
                 allocator is already borrowed",
                context
            );
            format!("GpuAllocator borrow conflict during {}", context)
        })
    }

    /// Drop the inner allocator. Called during `VulkanContext::drop`.
    pub(crate) unsafe fn destroy(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.inner);
        }
    }
}

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

        let allocation = self.allocator.allocate(&allocation_info, "buffer")?;

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
        self.allocator.free(allocation, "buffer");
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

        let allocation = self.allocator.allocate(&allocation_info, "image")?;

        unsafe {
            self.device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .map_err(|e| RendererError::VulkanError("Failed to bind image memory".into(), e))?;
        }
        Ok((image, allocation))
    }

    /// Free an image and its allocation.
    /// Uses wrapper type to avoid exposing vk::Image in public API.
    pub(crate) fn free_image(&self, image: crate::sync::VkImage, allocation: Allocation) {
        self.allocator.free(allocation, "image");
        unsafe {
            self.device.destroy_image(image.vk(), None);
        }
    }
}
