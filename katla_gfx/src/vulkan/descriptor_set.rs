//! Unified descriptor set builder.
//!
//! Provides a single builder pattern for creating descriptor sets that can bind
//! any combination of buffers, images, and samplers.
//!
//! # Example
//!
//! ```ignore
//! // Buffer-only descriptor set
//! let desc_set = DescriptorSetBuilder::new(&context)
//!     .storage_buffer(0, &particle_buffer)
//!     .uniform_buffer(1, &frame_data_buffer)
//!     .build(layout)?;
//!
//! // Mixed descriptor set (images + samplers + buffers)
//! let desc_set = DescriptorSetBuilder::new(&context)
//!     .sampled_image(0, font_texture.image_view())
//!     .sampler(1, sampler)
//!     .uniform_buffer(3, &uniform_buffer)
//!     .build(layout)?;
//! ```

use ash::vk;
use std::rc::Rc;

use super::context::VulkanContext;
use crate::sync::VkDescriptorSetLayout;

/// Resource binding types for descriptor sets.
#[derive(Clone, Debug)]
pub(crate) enum ResourceBinding {
    /// Storage buffer binding.
    StorageBuffer {
        buffer: crate::sync::VkBuffer,
        offset: vk::DeviceSize,
        range: vk::DeviceSize,
    },
}

impl ResourceBinding {
    /// Get the Vulkan descriptor type for this binding.
    pub(crate) fn descriptor_type(&self) -> vk::DescriptorType {
        match self {
            Self::StorageBuffer { .. } => vk::DescriptorType::STORAGE_BUFFER,
        }
    }
}

/// Trait for types that expose a Vulkan buffer for descriptor binding.
///
/// Implement this for your buffer types to enable easy descriptor creation.
pub(crate) trait BufferSource {
    /// Get the Vulkan buffer handle.
    fn buffer(&self) -> crate::sync::VkBuffer;
}

// ============================================================================
// BufferSource implementations for existing types
// ============================================================================

impl BufferSource for crate::vulkan::bda::DeviceAddressBuffer {
    fn buffer(&self) -> crate::sync::VkBuffer {
        self.buffer.into()
    }
}

impl BufferSource for crate::vulkan::skeleton_buffer::SkeletonBuffer {
    fn buffer(&self) -> crate::sync::VkBuffer {
        self.buffer().into()
    }
}

impl BufferSource for crate::vulkan::particle_buffer::ParticleBuffer {
    fn buffer(&self) -> crate::sync::VkBuffer {
        self.buffer().into()
    }
}

impl BufferSource for crate::vulkan::particle_buffer::EmitterConfigBuffer {
    fn buffer(&self) -> crate::sync::VkBuffer {
        self.buffer().into()
    }
}

impl<T: Copy> BufferSource for crate::vulkan::material::buffer_descriptor::UniformBuffer<T> {
    fn buffer(&self) -> crate::sync::VkBuffer {
        // Access the inherent method on UniformBuffer, not the trait method
        <Self as crate::vulkan::material::buffer_descriptor::BufferDescriptorSource>::buffer(self)
    }
}

/// Owned descriptor set with automatic cleanup.
///
/// Contains the descriptor set and its pool. When dropped, both are destroyed.
pub(crate) struct DescriptorSet {
    set: vk::DescriptorSet,
    pool: vk::DescriptorPool,
    owned_layout: Option<vk::DescriptorSetLayout>,
    device: ash::Device,
}

impl DescriptorSet {
    /// Get the raw Vulkan descriptor set handle.
    pub(crate) fn vk(&self) -> vk::DescriptorSet {
        self.set
    }

    /// Create from existing pool allocation (for advanced use cases).
    ///
    /// # Safety
    /// The pool must remain valid for the lifetime of this DescriptorSet.
    pub(crate) unsafe fn from_raw(
        set: vk::DescriptorSet,
        pool: vk::DescriptorPool,
        device: ash::Device,
    ) -> Self {
        Self {
            set,
            pool,
            owned_layout: None,
            device,
        }
    }
}

impl Drop for DescriptorSet {
    fn drop(&mut self) {
        unsafe {
            // Destroying the pool automatically frees all descriptor sets in it
            self.device.destroy_descriptor_pool(self.pool, None);
            if let Some(layout) = self.owned_layout.take() {
                self.device.destroy_descriptor_set_layout(layout, None);
            }
        }
    }
}

/// Flags for descriptor set creation.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DescriptorSetFlags {
    /// Enable UPDATE_AFTER_BIND for dynamic texture registration.
    pub update_after_bind: bool,
}

/// Unified descriptor set builder.
///
/// Provides a fluent API for configuring any combination of buffer and image bindings
/// before allocating and writing the descriptor set.
///
/// # Example
///
/// ```ignore
/// let desc_set = DescriptorSetBuilder::new(&context)
///     .storage_buffer(0, &storage_buffer)
///     .storage_buffer_range(1, &storage_buffer, 256, 28672)
///     .build(layout)?;
/// ```
pub(crate) struct DescriptorSetBuilder<'a> {
    context: &'a Rc<VulkanContext>,
    bindings: Vec<(u32, ResourceBinding)>,
    flags: DescriptorSetFlags,
}

impl<'a> DescriptorSetBuilder<'a> {
    /// Create a new builder.
    pub(crate) fn new(context: &'a Rc<VulkanContext>) -> Self {
        Self {
            context,
            bindings: Vec::new(),
            flags: DescriptorSetFlags::default(),
        }
    }

    /// Set creation flags.
    pub(crate) fn with_flags(mut self, flags: DescriptorSetFlags) -> Self {
        self.flags = flags;
        self
    }

    // ========================================================================
    // Buffer bindings
    // ========================================================================

    /// Add a storage buffer binding with explicit range.
    pub(crate) fn storage_buffer_range(
        mut self,
        binding: u32,
        buffer: &dyn BufferSource,
        offset: vk::DeviceSize,
        range: vk::DeviceSize,
    ) -> Self {
        let vk_buffer = buffer.buffer();
        self.bindings.push((
            binding,
            ResourceBinding::StorageBuffer {
                buffer: vk_buffer,
                offset,
                range,
            },
        ));
        self
    }

    // ========================================================================
    // Build methods
    // ========================================================================

    /// Build the descriptor set.
    ///
    /// Creates a descriptor pool, allocates a descriptor set from the layout,
    /// and writes all configured bindings.
    ///
    /// # Panics
    /// Panics if no bindings have been added.
    pub(crate) fn build(self, layout: VkDescriptorSetLayout) -> Result<DescriptorSet, vk::Result> {
        if self.bindings.is_empty() {
            panic!("DescriptorSetBuilder requires at least one binding");
        }

        let device = &self.context.device;

        // Calculate pool sizes for each descriptor type
        let mut pool_sizes: Vec<vk::DescriptorPoolSize> = Vec::new();
        for (_, binding) in &self.bindings {
            let ty = binding.descriptor_type();
            if let Some(existing) = pool_sizes.iter_mut().find(|p| p.ty == ty) {
                existing.descriptor_count += 1;
            } else {
                pool_sizes.push(vk::DescriptorPoolSize {
                    ty,
                    descriptor_count: 1,
                });
            }
        }

        let mut pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_sizes);

        // Add UPDATE_AFTER_BIND flag if requested
        if self.flags.update_after_bind {
            pool_info = pool_info.flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND);
        }

        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None)? };

        // Allocate descriptor set
        let layout_vk: vk::DescriptorSetLayout = layout.into();
        let layouts = [layout_vk];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe { device.allocate_descriptor_sets(&alloc_info)? };
        let descriptor_set = descriptor_sets[0];

        // Build descriptor infos (must stay in scope until update_descriptor_sets)
        let mut buffer_infos: Vec<vk::DescriptorBufferInfo> = Vec::new();

        for (_, binding) in &self.bindings {
            match binding {
                ResourceBinding::StorageBuffer {
                    buffer,
                    offset,
                    range,
                } => {
                    buffer_infos.push(
                        vk::DescriptorBufferInfo::default()
                            .buffer(vk::Buffer::from(*buffer))
                            .offset(*offset)
                            .range(*range),
                    );
                }
            }
        }

        // Build writes referencing the infos
        let mut writes: Vec<vk::WriteDescriptorSet> = Vec::new();
        let mut buffer_idx = 0;

        for (binding_num, binding) in &self.bindings {
            let write = match binding {
                ResourceBinding::StorageBuffer { .. } => {
                    let write = vk::WriteDescriptorSet::default()
                        .dst_set(descriptor_set)
                        .dst_binding(*binding_num)
                        .dst_array_element(0)
                        .descriptor_type(binding.descriptor_type())
                        .buffer_info(std::slice::from_ref(&buffer_infos[buffer_idx]));
                    buffer_idx += 1;
                    write
                }
            };
            writes.push(write);
        }

        // Update descriptor sets while infos are still in scope
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }

        Ok(DescriptorSet {
            set: descriptor_set,
            pool: descriptor_pool,
            owned_layout: None,
            device: device.clone(),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_binding_types() {
        let storage = ResourceBinding::StorageBuffer {
            buffer: crate::sync::VkBuffer::default(),
            offset: 0,
            range: 1024,
        };
        assert_eq!(
            storage.descriptor_type(),
            vk::DescriptorType::STORAGE_BUFFER
        );
    }

    #[test]
    fn test_descriptor_set_flags() {
        let flags = DescriptorSetFlags::default();
        assert!(!flags.update_after_bind);

        let flags = DescriptorSetFlags {
            update_after_bind: true,
        };
        assert!(flags.update_after_bind);
    }
}
