//! Buffer Device Address (BDA) support for Vulkan 1.2+.
//!
//! This module provides types and functions for working with Buffer Device Address,
//! which allows buffers to be accessed via GPU virtual addresses instead of descriptors.
//!
//! # Benefits
//! - Reduced descriptor management overhead
//! - More flexible uniform buffer access
//! - Enables pointer-based shader algorithms
//!
//! # Usage
//! ```ignore
//! // This example requires internal access to DeviceAddressBuffer
//! // Use the high-level API instead
//! ).expect("Failed to create BDA buffer");
//!
//! // Get the device address to pass to shaders via push constants
//! let address = buffer.device_address();
//!
//! // Map and write data
//! buffer.write(&[1.0f32, 2.0, 3.0, 4.0]);
//! ```

use ash::vk;
use gpu_allocator::MemoryLocation;
use std::rc::Rc;

use super::context::VulkanContext;

/// A buffer that can be accessed via its GPU device address.
///
/// This buffer type enables Buffer Device Address (BDA) functionality,
/// allowing shaders to access buffer contents via pointer addresses rather
/// than descriptor bindings.
pub struct DeviceAddressBuffer {
    /// The underlying Vulkan buffer handle.
    pub(crate) buffer: vk::Buffer,
    /// Memory allocation for this buffer.
    pub(crate) allocation: gpu_allocator::vulkan::Allocation,
    /// Size of the buffer in bytes.
    pub size: u64,
    /// Whether this buffer is persistently mapped.
    #[allow(dead_code)]
    is_persistent: bool,
    /// Persistent mapping pointer (if is_persistent is true).
    mapped_ptr: Option<*mut u8>,
    /// Vulkan context reference.
    context: Rc<VulkanContext>,
}

unsafe impl Send for DeviceAddressBuffer {}
unsafe impl Sync for DeviceAddressBuffer {}

impl DeviceAddressBuffer {
    /// Create a new persistently mapped Device Address Buffer.
    ///
    /// This is useful for frequently updated uniform data, as it avoids
    /// repeated map/unmap operations.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `size` - Size of the buffer in bytes
    ///
    /// # Returns
    /// A new persistently mapped DeviceAddressBuffer, or an error if creation fails.
    pub fn new_persistent(context: Rc<VulkanContext>, size: u64) -> Result<Self, vk::Result> {
        // Create buffer with SHADER_DEVICE_ADDRESS flag
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::UNIFORM_BUFFER
                    | vk::BufferUsageFlags::STORAGE_BUFFER,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (buffer, allocation) = context.allocate_buffer(&buffer_info, MemoryLocation::CpuToGpu);

        // Map the buffer persistently
        let mapped_ptr = unsafe {
            let ptr = context.map_buffer(&allocation);
            std::ptr::write_bytes(ptr, 0, size as usize);
            ptr
        };

        Ok(Self {
            buffer,
            allocation,
            size,
            is_persistent: true,
            mapped_ptr: Some(mapped_ptr),
            context,
        })
    }

    /// Map the buffer for CPU access.
    ///
    /// If the buffer is persistently mapped, returns the persistent mapping.
    /// Otherwise, temporarily maps the buffer.
    ///
    /// # Safety
    /// - The buffer must not already be mapped (unless persistently mapped)
    /// - The mapped memory must not be accessed after the buffer is dropped
    pub unsafe fn map(&mut self) -> &mut [u8] {
        if let Some(ptr) = self.mapped_ptr {
            unsafe { std::slice::from_raw_parts_mut(ptr, self.size as usize) }
        } else {
            let ptr = self.context.map_buffer(&self.allocation);
            unsafe { std::slice::from_raw_parts_mut(ptr, self.size as usize) }
        }
    }

    /// Check if this buffer is persistently mapped.
    #[inline]
    pub fn is_persistent(&self) -> bool {
        self.is_persistent
    }

    /// Flush a range of mapped memory to make CPU writes visible to the GPU.
    ///
    /// # Arguments
    /// * `offset` - Offset from the start of the buffer (in bytes)
    /// * `size` - Size of the range to flush (in bytes)
    pub fn flush(&self, offset: u64, size: u64) {
        self.context
            .flush_mapped_memory(&self.allocation, offset, size);
    }
}

impl Drop for DeviceAddressBuffer {
    fn drop(&mut self) {
        self.context
            .free_buffer(self.buffer, std::mem::take(&mut self.allocation));
    }
}
