//! Buffer descriptor utilities.
//!
//! Provides types for working with uniform and storage buffer descriptors.
//!
//! For creating descriptor sets, use [`crate::vulkan::DescriptorSetBuilder`].

use ash::vk;
use std::rc::Rc;

use super::VulkanContext;

/// Trait for types that can provide buffer binding info.
///
/// Implement this for your buffer types to enable easy descriptor creation.
/// The new [`crate::vulkan::BufferSource`] trait is the preferred way to work
/// with buffers in descriptor sets, but this trait is kept for backward compatibility.
pub(crate) trait BufferDescriptorSource {
    /// Get the Vulkan buffer handle.
    fn buffer(&self) -> crate::sync::VkBuffer;
}

// Implement BufferDescriptorSource for DeviceAddressBuffer
impl BufferDescriptorSource for crate::vulkan::bda::DeviceAddressBuffer {
    fn buffer(&self) -> crate::sync::VkBuffer {
        crate::sync::VkBuffer::new(self.buffer)
    }
}

// Implement BufferDescriptorSource for SkeletonBuffer
impl BufferDescriptorSource for crate::vulkan::skeleton_buffer::SkeletonBuffer {
    fn buffer(&self) -> crate::sync::VkBuffer {
        crate::sync::VkBuffer::new(self.buffer())
    }
}

/// Type-safe uniform buffer for shader uniforms.
///
/// Provides a simple API for creating and updating uniform buffers.
/// Automatically handles memory allocation and mapping.
///
/// # Example
///
/// ```ignore
/// // Create a uniform buffer for screen size
/// let screen_uniform = UniformBuffer::<[f32; 4]>::new(context)?;
///
/// // Update the data
/// screen_uniform.write(&[width, height, 0.0, 0.0]);
///
/// // Use with descriptor set builder
/// let desc_set = DescriptorSetBuilder::new(&context)
///     .uniform_buffer(0, &screen_uniform)
///     .build(layout)?;
/// ```
pub struct UniformBuffer<T: Copy> {
    buffer: vk::Buffer,
    allocation: gpu_allocator::vulkan::Allocation,
    _marker: std::marker::PhantomData<T>,
    context: Rc<VulkanContext>,
}

impl<T: Copy> UniformBuffer<T> {
    /// Create a new uniform buffer with space for type T.
    ///
    /// The buffer is allocated with CpuToGpu memory, which guarantees
    /// a persistent mapped pointer for efficient writes via [`write`](Self::write).
    pub fn new(context: Rc<VulkanContext>) -> Result<Self, vk::Result> {
        let size = std::mem::size_of::<T>() as vk::DeviceSize;

        let buffer_info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .size(size);

        let (buffer, allocation) =
            context.allocate_buffer(&buffer_info, gpu_allocator::MemoryLocation::CpuToGpu);

        Ok(Self {
            buffer,
            allocation,
            _marker: std::marker::PhantomData,
            context,
        })
    }

    /// Write data to the uniform buffer.
    ///
    /// # Panics
    ///
    /// Panics if the allocation is not mapped. This should never happen
    /// for buffers allocated with `CpuToGpu` memory location.
    pub fn write(&self, data: &T) {
        let ptr = self
            .allocation
            .mapped_ptr()
            .expect("UniformBuffer: allocation should be mapped (CpuToGpu guarantees this)")
            .cast::<T>()
            .as_ptr();
        unsafe {
            std::ptr::write_unaligned(ptr, *data);
        }
    }

    /// Get the buffer size in bytes.
    pub fn size(&self) -> vk::DeviceSize {
        std::mem::size_of::<T>() as vk::DeviceSize
    }

    /// Get the Vulkan buffer handle.
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer
    }
}

impl<T: Copy> BufferDescriptorSource for UniformBuffer<T> {
    fn buffer(&self) -> crate::sync::VkBuffer {
        crate::sync::VkBuffer::new(self.buffer)
    }
}

impl<T: Copy> Drop for UniformBuffer<T> {
    fn drop(&mut self) {
        self.context.free_buffer(
            crate::sync::VkBuffer::new(self.buffer),
            std::mem::take(&mut self.allocation),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uniform_buffer_trait() {
        // Just verify the trait exists and compiles
        fn assert_buffer_source<T: BufferDescriptorSource>(_: &T) {}
        // UniformBuffer implements BufferDescriptorSource
    }
}
