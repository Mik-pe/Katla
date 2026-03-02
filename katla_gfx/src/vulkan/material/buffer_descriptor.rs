//! Buffer descriptor utilities.
//!
//! Provides types for working with uniform and storage buffer descriptors.
//!
//! For creating descriptor sets, use [`crate::vulkan::DescriptorSetBuilder`].

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
