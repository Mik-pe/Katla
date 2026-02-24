//! Buffer descriptor utilities.
//!
//! Provides types for working with uniform and storage buffer descriptors.
//! For creating descriptor sets, use [`crate::vulkan::DescriptorSetBuilder`].

use ash::vk;
use std::rc::Rc;

use super::VulkanContext;

/// Info for a single buffer binding in a descriptor set.
#[derive(Clone, Copy, Debug)]
pub struct BufferBinding {
    /// The Vulkan buffer handle.
    pub buffer: vk::Buffer,
    /// Binding number in the shader.
    pub binding: u32,
    /// Offset into the buffer (in bytes).
    pub offset: vk::DeviceSize,
    /// Range/size of the binding (in bytes).
    pub range: vk::DeviceSize,
    /// Descriptor type for this binding (STORAGE_BUFFER or UNIFORM_BUFFER).
    pub descriptor_type: vk::DescriptorType,
}

/// Trait for types that can provide buffer binding info.
///
/// Implement this for your buffer types to enable easy descriptor creation.
/// The new [`crate::vulkan::BufferSource`] trait is the preferred way to work
/// with buffers in descriptor sets, but this trait is kept for backward compatibility.
pub trait BufferDescriptorSource {
    /// Get the Vulkan buffer handle.
    fn buffer(&self) -> vk::Buffer;

    /// Get the buffer size in bytes.
    fn buffer_size(&self) -> vk::DeviceSize;

    /// Create a binding for this entire buffer.
    /// Note: descriptor_type defaults to STORAGE_BUFFER.
    fn as_binding(&self, binding: u32) -> BufferBinding {
        BufferBinding {
            buffer: self.buffer(),
            binding,
            offset: 0,
            range: self.buffer_size(),
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
        }
    }

    /// Create a binding for a range within this buffer.
    /// Note: descriptor_type defaults to STORAGE_BUFFER.
    fn as_binding_range(
        &self,
        binding: u32,
        offset: vk::DeviceSize,
        range: vk::DeviceSize,
    ) -> BufferBinding {
        BufferBinding {
            buffer: self.buffer(),
            binding,
            offset,
            range,
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
        }
    }
}

// Implement BufferDescriptorSource for DeviceAddressBuffer
impl BufferDescriptorSource for crate::vulkan::bda::DeviceAddressBuffer {
    fn buffer(&self) -> vk::Buffer {
        self.buffer
    }

    fn buffer_size(&self) -> vk::DeviceSize {
        self.size()
    }
}

// Implement BufferDescriptorSource for SkeletonBuffer
impl BufferDescriptorSource for crate::vulkan::skeleton_buffer::SkeletonBuffer {
    fn buffer(&self) -> vk::Buffer {
        self.buffer()
    }

    fn buffer_size(&self) -> vk::DeviceSize {
        self.size()
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
    fn buffer(&self) -> vk::Buffer {
        self.buffer
    }

    fn buffer_size(&self) -> vk::DeviceSize {
        self.size()
    }
}

impl<T: Copy> Drop for UniformBuffer<T> {
    fn drop(&mut self) {
        self.context
            .free_buffer(self.buffer, std::mem::take(&mut self.allocation));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_binding_creation() {
        let binding = BufferBinding {
            buffer: vk::Buffer::null(),
            binding: 0,
            offset: 0,
            range: 1024,
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
        };
        assert_eq!(binding.binding, 0);
        assert_eq!(binding.range, 1024);
    }

    #[test]
    fn test_binding_accumulation() {
        // This tests creating multiple bindings without Vulkan
        let bindings: Vec<BufferBinding> = vec![
            BufferBinding {
                buffer: vk::Buffer::null(),
                binding: 0,
                offset: 0,
                range: 256,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
            },
            BufferBinding {
                buffer: vk::Buffer::null(),
                binding: 1,
                offset: 256,
                range: 24576,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
            },
        ];
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].binding, 0);
        assert_eq!(bindings[1].binding, 1);
    }
}
