//! Generic buffer descriptor set builder.
//!
//! Provides a flexible builder pattern for creating descriptor sets
//! that bind storage/uniform buffers to shaders. This unifies the
//! descriptor creation logic that was previously duplicated between
//! `SkeletonDescriptorSet` and `StorageDescriptorSet`.
//!
//! # Example
//!
//! ```ignore
//! // Single binding (e.g., skeleton buffer)
//! let desc_set = BufferDescriptorSetBuilder::new(&context)
//!     .add_binding(skeleton_buffer.buffer(), 0, 0, skeleton_buffer.size())
//!     .build(layout)?;
//!
//! // Multiple bindings (e.g., frame + object uniforms)
//! let desc_set = BufferDescriptorSetBuilder::new(&context)
//!     .add_binding(storage_buffer.buffer, 0, 0, frame_size)
//!     .add_binding(storage_buffer.buffer, 1, object_offset, object_range)
//!     .build(layout)?;
//! ```

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

/// Descriptor set for binding buffers to shaders.
///
/// Contains the descriptor set and pool, with automatic cleanup on drop.
/// This is a low-level type - use `BufferDescriptorSetBuilder` for construction.
pub struct BufferDescriptorSet {
    /// The descriptor set handle.
    descriptor_set: vk::DescriptorSet,
    /// The descriptor pool (owned, for cleanup).
    descriptor_pool: vk::DescriptorPool,
    /// Optional owned descriptor set layout (for cleanup).
    owned_layout: Option<vk::DescriptorSetLayout>,
    /// Device for cleanup.
    device: ash::Device,
}

impl BufferDescriptorSet {
    /// Get the descriptor set handle as a wrapper type.
    pub fn set(&self) -> crate::sync::VkDescriptorSet {
        crate::sync::VkDescriptorSet::new(self.descriptor_set)
    }

    /// Get the raw Vulkan descriptor set handle (for internal use).
    pub(crate) fn vk_set(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }
}

impl Drop for BufferDescriptorSet {
    fn drop(&mut self) {
        unsafe {
            // Destroying the pool automatically frees all descriptor sets in it
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            // Destroy owned layout if present
            if let Some(layout) = self.owned_layout.take() {
                self.device.destroy_descriptor_set_layout(layout, None);
            }
        }
    }
}

/// Builder for creating buffer descriptor sets.
///
/// Provides a fluent API for configuring buffer bindings before
/// allocating and writing the descriptor set.
pub struct BufferDescriptorSetBuilder<'a> {
    context: &'a Rc<VulkanContext>,
    bindings: Vec<BufferBinding>,
    descriptor_type: vk::DescriptorType,
}

impl<'a> BufferDescriptorSetBuilder<'a> {
    /// Create a new builder with STORAGE_BUFFER as the default descriptor type.
    pub fn new(context: &'a Rc<VulkanContext>) -> Self {
        Self {
            context,
            bindings: Vec::new(),
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
        }
    }

    /// Set the descriptor type (default: STORAGE_BUFFER).
    ///
    /// Use this for uniform buffers:
    /// ```ignore
    /// builder.with_descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
    /// ```
    pub fn with_descriptor_type(mut self, descriptor_type: vk::DescriptorType) -> Self {
        self.descriptor_type = descriptor_type;
        self
    }

    /// Set the descriptor type using the wrapper type.
    ///
    /// Use this for uniform buffers:
    /// ```ignore
    /// builder.with_descriptor_type_wrapped(DescriptorType::UniformBuffer)
    /// ```
    pub fn with_descriptor_type_wrapped(mut self, descriptor_type: crate::vulkan::pipeline_state::DescriptorType) -> Self {
        self.descriptor_type = descriptor_type.into();
        self
    }

    /// Add a buffer binding.
    ///
    /// # Arguments
    /// * `buffer` - The Vulkan buffer handle
    /// * `binding` - Binding number in the shader (0, 1, 2, ...)
    /// * `offset` - Offset into the buffer (bytes)
    /// * `range` - Size of the binding (bytes)
    ///
    /// Note: Uses the current descriptor type set by `with_descriptor_type()`
    /// (default: STORAGE_BUFFER).
    pub fn add_binding(
        mut self,
        buffer: vk::Buffer,
        binding: u32,
        offset: vk::DeviceSize,
        range: vk::DeviceSize,
    ) -> Self {
        self.bindings.push(BufferBinding {
            buffer,
            binding,
            offset,
            range,
            descriptor_type: self.descriptor_type,
        });
        self
    }

    /// Add a buffer binding from a pre-constructed `BufferBinding`.
    pub fn add_binding_raw(mut self, binding: BufferBinding) -> Self {
        self.bindings.push(binding);
        self
    }

    /// Add an entire buffer as a binding.
    ///
    /// Convenience method for binding a whole buffer at offset 0.
    ///
    /// # Arguments
    /// * `source` - Any type implementing `BufferDescriptorSource`
    /// * `binding` - Binding number in the shader (0, 1, 2, ...)
    ///
    /// Note: Uses the current descriptor type set by `with_descriptor_type()`
    /// (default: STORAGE_BUFFER).
    pub fn add_entire_buffer(mut self, source: &impl BufferDescriptorSource, binding: u32) -> Self {
        let mut binding_info = source.as_binding(binding);
        binding_info.descriptor_type = self.descriptor_type;
        self.bindings.push(binding_info);
        self
    }

    /// Add a buffer range as a binding.
    ///
    /// Convenience method for binding a portion of a buffer.
    ///
    /// # Arguments
    /// * `source` - Any type implementing `BufferDescriptorSource`
    /// * `binding` - Binding number in the shader (0, 1, 2, ...)
    /// * `offset` - Offset into the buffer (bytes)
    /// * `range` - Size of the binding (bytes)
    ///
    /// Note: Uses the current descriptor type set by `with_descriptor_type()`
    /// (default: STORAGE_BUFFER).
    pub fn add_buffer_range(
        mut self,
        source: &impl BufferDescriptorSource,
        binding: u32,
        offset: vk::DeviceSize,
        range: vk::DeviceSize,
    ) -> Self {
        let mut binding_info = source.as_binding_range(binding, offset, range);
        binding_info.descriptor_type = self.descriptor_type;
        self.bindings.push(binding_info);
        self
    }

    /// Build the descriptor set.
    ///
    /// Creates a descriptor pool, allocates a descriptor set from the layout,
    /// and writes all configured buffer bindings.
    ///
    /// # Arguments
    /// * `layout` - The descriptor set layout to allocate from (wrapper type)
    ///
    /// # Returns
    /// A `BufferDescriptorSet` containing the allocated and written descriptor set.
    pub fn build(self, layout: crate::sync::VkDescriptorSetLayout) -> Result<BufferDescriptorSet, vk::Result> {
        if self.bindings.is_empty() {
            panic!("BufferDescriptorSetBuilder requires at least one binding");
        }

        let device = &self.context.device;

        // Calculate pool sizes for each descriptor type used
        let mut storage_count = 0u32;
        let mut uniform_count = 0u32;
        for binding in &self.bindings {
            match binding.descriptor_type {
                vk::DescriptorType::STORAGE_BUFFER => storage_count += 1,
                vk::DescriptorType::UNIFORM_BUFFER => uniform_count += 1,
                _ => {}
            }
        }

        let mut pool_sizes = Vec::new();
        if storage_count > 0 {
            pool_sizes.push(vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: storage_count,
            });
        }
        if uniform_count > 0 {
            pool_sizes.push(vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: uniform_count,
            });
        }

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_sizes);

        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None)? };

        // Allocate descriptor set
        let layout_vk: vk::DescriptorSetLayout = layout.into();
        let layouts = [layout_vk];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe { device.allocate_descriptor_sets(&alloc_info)? };
        let descriptor_set = descriptor_sets[0];

        // Create buffer infos and writes for all bindings
        let buffer_infos: Vec<vk::DescriptorBufferInfo> = self
            .bindings
            .iter()
            .map(|b| {
                vk::DescriptorBufferInfo::default()
                    .buffer(b.buffer)
                    .offset(b.offset)
                    .range(b.range)
            })
            .collect();

        let descriptor_writes: Vec<vk::WriteDescriptorSet> = self
            .bindings
            .iter()
            .enumerate()
            .map(|(i, b)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(b.binding)
                    .dst_array_element(0)
                    .descriptor_type(b.descriptor_type) // Use per-binding descriptor type
                    .buffer_info(std::slice::from_ref(&buffer_infos[i]))
            })
            .collect();

        unsafe {
            device.update_descriptor_sets(&descriptor_writes, &[]);
        }

        Ok(BufferDescriptorSet {
            descriptor_set,
            descriptor_pool,
            owned_layout: None,
            device: device.clone(),
        })
    }

    /// Build the descriptor set and take ownership of the layout.
    ///
    /// This is useful when the descriptor set should own its layout for cleanup,
    /// such as when the layout is created specifically for this descriptor set
    /// and not shared with other resources.
    pub fn build_with_owned_layout(
        self,
        layout: crate::sync::VkDescriptorSetLayout,
    ) -> Result<BufferDescriptorSet, vk::Result> {
        let layout_vk: vk::DescriptorSetLayout = layout.into();
        let mut descriptor_set = self.build(layout)?;
        descriptor_set.owned_layout = Some(layout_vk);
        Ok(descriptor_set)
    }

    /// Build the descriptor set with a pre-allocated pool.
    ///
    /// This is useful when you want to manage pool lifetime yourself,
    /// or when allocating multiple descriptor sets from the same pool.
    ///
    /// # Safety
    /// The pool must have enough space for the descriptor set and all bindings.
    pub unsafe fn build_with_pool(
        self,
        layout: vk::DescriptorSetLayout,
        descriptor_pool: vk::DescriptorPool,
    ) -> Result<(vk::DescriptorSet, Vec<vk::DescriptorBufferInfo>), vk::Result> {
        if self.bindings.is_empty() {
            panic!("BufferDescriptorSetBuilder requires at least one binding");
        }

        let device = &self.context.device;

        // Allocate descriptor set
        let layouts = [layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = device.allocate_descriptor_sets(&alloc_info)?;
        let descriptor_set = descriptor_sets[0];

        // Create buffer infos and writes for all bindings
        let buffer_infos: Vec<vk::DescriptorBufferInfo> = self
            .bindings
            .iter()
            .map(|b| {
                vk::DescriptorBufferInfo::default()
                    .buffer(b.buffer)
                    .offset(b.offset)
                    .range(b.range)
            })
            .collect();

        let descriptor_writes: Vec<vk::WriteDescriptorSet> = self
            .bindings
            .iter()
            .enumerate()
            .map(|(i, b)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(b.binding)
                    .dst_array_element(0)
                    .descriptor_type(self.descriptor_type)
                    .buffer_info(std::slice::from_ref(&buffer_infos[i]))
            })
            .collect();

        device.update_descriptor_sets(&descriptor_writes, &[]);

        Ok((descriptor_set, buffer_infos))
    }
}

/// Trait for types that can provide buffer binding info.
///
/// Implement this for your buffer types to enable easy descriptor creation.
pub trait BufferDescriptorSource {
    /// Get the Vulkan buffer handle.
    fn buffer(&self) -> vk::Buffer;

    /// Get the buffer size in bytes.
    fn buffer_size(&self) -> vk::DeviceSize;

    /// Create a binding for this entire buffer.
    /// Note: descriptor_type defaults to STORAGE_BUFFER; override with builder's with_descriptor_type().
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
    /// Note: descriptor_type defaults to STORAGE_BUFFER; override with builder's with_descriptor_type().
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
    fn test_builder_accumulates_bindings() {
        // This tests the builder pattern without Vulkan
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
