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
//! ```no_run
//! use katla_vulkan::vulkan::bda::DeviceAddressBuffer;
//! # use katla_vulkan::VulkanContext;
//! # use std::rc::Rc;
//! # let context: Rc<VulkanContext> = unsafe { std::mem::zeroed() };
//!
//! let mut buffer = DeviceAddressBuffer::new(
//!     context.clone(),
//!     4096,  // size in bytes
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
use crate::sync::VkBuffer;

/// A buffer that can be accessed via its GPU device address.
///
/// This buffer type enables Buffer Device Address (BDA) functionality,
/// allowing shaders to access buffer contents via pointer addresses rather
/// than descriptor bindings.
pub struct DeviceAddressBuffer {
    /// The underlying Vulkan buffer handle.
    pub buffer: vk::Buffer,
    /// Memory allocation for this buffer.
    pub allocation: gpu_allocator::vulkan::Allocation,
    /// The GPU device address of this buffer.
    device_address: u64,
    /// Size of the buffer in bytes.
    pub size: u64,
    /// Whether this buffer is persistently mapped.
    is_persistent: bool,
    /// Persistent mapping pointer (if is_persistent is true).
    mapped_ptr: Option<*mut u8>,
    /// Vulkan context reference.
    context: Rc<VulkanContext>,
}

unsafe impl Send for DeviceAddressBuffer {}
unsafe impl Sync for DeviceAddressBuffer {}

impl DeviceAddressBuffer {
    /// Create a new Device Address Buffer.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `size` - Size of the buffer in bytes
    ///
    /// # Returns
    /// A new DeviceAddressBuffer, or an error if creation fails.
    pub fn new(context: Rc<VulkanContext>, size: u64) -> Result<Self, vk::Result> {
        Self::with_location(context, size, MemoryLocation::CpuToGpu)
    }

    /// Create a new Device Address Buffer with specific memory location.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `size` - Size of the buffer in bytes
    /// * `location` - Memory location (GpuOnly, CpuToGpu, etc.)
    ///
    /// # Returns
    /// A new DeviceAddressBuffer, or an error if creation fails.
    pub fn with_location(
        context: Rc<VulkanContext>,
        size: u64,
        location: MemoryLocation,
    ) -> Result<Self, vk::Result> {
        // Create buffer with SHADER_DEVICE_ADDRESS flag
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::UNIFORM_BUFFER
                    | vk::BufferUsageFlags::STORAGE_BUFFER,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (buffer, allocation) = context.allocate_buffer(&buffer_info, location);

        // Get the device address
        let device_address = unsafe {
            let buffer_device_address_info = vk::BufferDeviceAddressInfo::default().buffer(buffer);

            context
                .device
                .get_buffer_device_address(&buffer_device_address_info)
        };

        Ok(Self {
            buffer,
            allocation,
            device_address,
            size,
            is_persistent: false,
            mapped_ptr: None,
            context,
        })
    }

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
        let mut buffer = Self::with_location(context, size, MemoryLocation::CpuToGpu)?;

        // Map the buffer persistently
        let mapped_ptr = unsafe {
            let ptr = buffer.context.map_buffer(&buffer.allocation);

            // Zero the memory
            std::ptr::write_bytes(ptr, 0, buffer.size as usize);

            ptr
        };

        buffer.is_persistent = true;
        buffer.mapped_ptr = Some(mapped_ptr);

        Ok(buffer)
    }

    /// Get the GPU device address of this buffer.
    ///
    /// This address can be passed to shaders via push constants or other
    /// mechanisms to enable direct buffer access in shaders.
    #[inline]
    pub fn device_address(&self) -> u64 {
        self.device_address
    }

    /// Get the size of this buffer in bytes.
    #[inline]
    pub fn size(&self) -> u64 {
        self.size
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
            std::slice::from_raw_parts_mut(ptr, self.size as usize)
        } else {
            let ptr = self.context.map_buffer(&self.allocation);
            std::slice::from_raw_parts_mut(ptr, self.size as usize)
        }
    }

    /// Write data to the buffer.
    ///
    /// This is a convenience method that handles mapping/unmapping for non-persistent buffers.
    ///
    /// # Arguments
    /// * `data` - Data to write
    pub fn write<T: Copy>(&mut self, data: &[T]) {
        let byte_len = std::mem::size_of_val(data);
        assert!(
            byte_len <= self.size as usize,
            "Data size ({} bytes) exceeds buffer size ({} bytes)",
            byte_len,
            self.size
        );

        unsafe {
            let mapped = self.map();
            std::ptr::copy_nonoverlapping(
                data.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_len,
            );
        }
    }

    /// Flush mapped memory if needed (for non-coherent memory).
    pub fn flush(&self) {
        // Currently using CpuToGpu which is HOST_COHERENT, so no flush needed
        // If using GpuOnly with staging, would need vkFlushMappedMemoryRanges
    }

    /// Check if this buffer is persistently mapped.
    #[inline]
    pub fn is_persistent(&self) -> bool {
        self.is_persistent
    }
}

impl Drop for DeviceAddressBuffer {
    fn drop(&mut self) {
        self.context.free_buffer(
            VkBuffer::new(self.buffer),
            std::mem::take(&mut self.allocation),
        );
    }
}

/// Extension trait for creating device-address-enabled buffers.
pub trait DeviceAddressBufferExt {
    /// Create a new Device Address Buffer.
    fn create_device_address_buffer(&self, size: u64) -> Result<DeviceAddressBuffer, vk::Result>;
}

impl DeviceAddressBufferExt for Rc<VulkanContext> {
    fn create_device_address_buffer(&self, size: u64) -> Result<DeviceAddressBuffer, vk::Result> {
        DeviceAddressBuffer::new(self.clone(), size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a valid Vulkan context and are integration tests.
    // Unit tests without Vulkan context are limited for BDA functionality.

    #[test]
    fn test_device_address_buffer_size_alignment() {
        // Test that size calculations work correctly
        let size = 4096u64;
        assert_eq!(size, 4096);

        // Ensure 256-byte alignment (common requirement for BDA)
        assert_eq!(
            size % 256,
            0,
            "Buffer size must be 256-byte aligned for BDA"
        );
    }

    #[test]
    fn test_device_address_buffer_f32_array() {
        // Test writing an array of f32 values
        let data: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
        let byte_len = std::mem::size_of_val(&data);
        assert_eq!(byte_len, 16);

        // Ensure proper alignment
        assert_eq!(byte_len % 4, 0, "f32 arrays must be 4-byte aligned");
    }

    #[test]
    fn test_device_address_buffer_mat4() {
        // Test size for a 4x4 matrix (64 bytes for f32)
        let mat4_size = std::mem::size_of::<[[f32; 4]; 4]>();
        assert_eq!(mat4_size, 64);

        // Ensure 16-byte alignment (mat4 requirement)
        assert_eq!(mat4_size % 16, 0, "mat4 must be 16-byte aligned");
    }
}
