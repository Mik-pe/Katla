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

use crate::sync::{VkDescriptorSet, VkDescriptorSetLayout, VkImageView, VkSampler};
use crate::vulkan::VulkanContext;

/// Resource binding types for descriptor sets.
#[derive(Clone, Debug)]
pub enum DescriptorBinding {
    /// Storage buffer binding.
    StorageBuffer {
        buffer: vk::Buffer,
        offset: vk::DeviceSize,
        range: vk::DeviceSize,
    },
    /// Uniform buffer binding.
    UniformBuffer {
        buffer: vk::Buffer,
        offset: vk::DeviceSize,
        range: vk::DeviceSize,
    },
    /// Sampled image binding (requires separate sampler).
    SampledImage {
        view: vk::ImageView,
        layout: vk::ImageLayout,
    },
    /// Sampler binding.
    Sampler { sampler: vk::Sampler },
    /// Combined image sampler binding (sampler + image together).
    CombinedImageSampler {
        view: vk::ImageView,
        sampler: vk::Sampler,
        layout: vk::ImageLayout,
    },
    /// Storage image binding (read/write access).
    StorageImage {
        view: vk::ImageView,
        layout: vk::ImageLayout,
    },
}

impl DescriptorBinding {
    /// Get the Vulkan descriptor type for this binding.
    pub fn descriptor_type(&self) -> vk::DescriptorType {
        match self {
            Self::StorageBuffer { .. } => vk::DescriptorType::STORAGE_BUFFER,
            Self::UniformBuffer { .. } => vk::DescriptorType::UNIFORM_BUFFER,
            Self::SampledImage { .. } => vk::DescriptorType::SAMPLED_IMAGE,
            Self::Sampler { .. } => vk::DescriptorType::SAMPLER,
            Self::CombinedImageSampler { .. } => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            Self::StorageImage { .. } => vk::DescriptorType::STORAGE_IMAGE,
        }
    }
}

/// Trait for types that expose a Vulkan buffer for descriptor binding.
///
/// Implement this for your buffer types to enable easy descriptor creation.
pub trait BufferSource {
    /// Get the Vulkan buffer handle.
    fn buffer(&self) -> vk::Buffer;
    /// Get the buffer size in bytes.
    fn size(&self) -> vk::DeviceSize;
}

// ============================================================================
// BufferSource implementations for existing types
// ============================================================================

impl BufferSource for crate::vulkan::bda::DeviceAddressBuffer {
    fn buffer(&self) -> vk::Buffer {
        self.buffer
    }
    fn size(&self) -> vk::DeviceSize {
        self.size
    }
}

impl BufferSource for crate::vulkan::skeleton_buffer::SkeletonBuffer {
    fn buffer(&self) -> vk::Buffer {
        self.buffer()
    }
    fn size(&self) -> vk::DeviceSize {
        self.size()
    }
}

impl BufferSource for crate::vulkan::particle_buffer::ParticleBuffer {
    fn buffer(&self) -> vk::Buffer {
        self.buffer()
    }
    fn size(&self) -> vk::DeviceSize {
        self.size()
    }
}

impl BufferSource for crate::vulkan::particle_buffer::EmitterConfigBuffer {
    fn buffer(&self) -> vk::Buffer {
        self.buffer()
    }
    fn size(&self) -> vk::DeviceSize {
        std::mem::size_of::<crate::vulkan::particle_buffer::EmitterConfig>() as vk::DeviceSize
    }
}

impl<T: Copy> BufferSource for crate::vulkan::material::buffer_descriptor::UniformBuffer<T> {
    fn buffer(&self) -> vk::Buffer {
        // Access the inherent method on UniformBuffer, not the trait method
        <Self as crate::vulkan::material::buffer_descriptor::BufferDescriptorSource>::buffer(self)
    }
    fn size(&self) -> vk::DeviceSize {
        // Access the inherent method on UniformBuffer
        <Self as crate::vulkan::material::buffer_descriptor::BufferDescriptorSource>::buffer_size(self)
    }
}

/// Owned descriptor set with automatic cleanup.
///
/// Contains the descriptor set and its pool. When dropped, both are destroyed.
pub struct DescriptorSet {
    set: vk::DescriptorSet,
    pool: vk::DescriptorPool,
    owned_layout: Option<vk::DescriptorSetLayout>,
    device: ash::Device,
}

impl DescriptorSet {
    /// Get the descriptor set handle as a wrapper type.
    pub fn wrapped(&self) -> VkDescriptorSet {
        VkDescriptorSet::new(self.set)
    }

    /// Get the raw Vulkan descriptor set handle.
    pub fn vk(&self) -> vk::DescriptorSet {
        self.set
    }

    /// Create from existing pool allocation (for advanced use cases).
    ///
    /// # Safety
    /// The pool must remain valid for the lifetime of this DescriptorSet.
    pub unsafe fn from_raw(
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
pub struct DescriptorSetFlags {
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
pub struct DescriptorSetBuilder<'a> {
    context: &'a Rc<VulkanContext>,
    bindings: Vec<(u32, DescriptorBinding)>,
    flags: DescriptorSetFlags,
}

impl<'a> DescriptorSetBuilder<'a> {
    /// Create a new builder.
    pub fn new(context: &'a Rc<VulkanContext>) -> Self {
        Self {
            context,
            bindings: Vec::new(),
            flags: DescriptorSetFlags::default(),
        }
    }

    /// Set creation flags.
    pub fn with_flags(mut self, flags: DescriptorSetFlags) -> Self {
        self.flags = flags;
        self
    }

    // ========================================================================
    // Buffer bindings
    // ========================================================================

    /// Add a storage buffer binding (entire buffer).
    pub fn storage_buffer(mut self, binding: u32, source: &impl BufferSource) -> Self {
        self.bindings.push((
            binding,
            DescriptorBinding::StorageBuffer {
                buffer: source.buffer(),
                offset: 0,
                range: source.size(),
            },
        ));
        self
    }

    /// Add a storage buffer binding with explicit range.
    pub fn storage_buffer_range(
        mut self,
        binding: u32,
        source: &impl BufferSource,
        offset: vk::DeviceSize,
        range: vk::DeviceSize,
    ) -> Self {
        self.bindings.push((
            binding,
            DescriptorBinding::StorageBuffer {
                buffer: source.buffer(),
                offset,
                range,
            },
        ));
        self
    }

    /// Add a uniform buffer binding (entire buffer).
    pub fn uniform_buffer(mut self, binding: u32, source: &impl BufferSource) -> Self {
        self.bindings.push((
            binding,
            DescriptorBinding::UniformBuffer {
                buffer: source.buffer(),
                offset: 0,
                range: source.size(),
            },
        ));
        self
    }

    /// Add a uniform buffer binding with explicit range.
    pub fn uniform_buffer_range(
        mut self,
        binding: u32,
        source: &impl BufferSource,
        offset: vk::DeviceSize,
        range: vk::DeviceSize,
    ) -> Self {
        self.bindings.push((
            binding,
            DescriptorBinding::UniformBuffer {
                buffer: source.buffer(),
                offset,
                range,
            },
        ));
        self
    }

    /// Add a raw buffer binding with explicit type.
    ///
    /// This is a low-level method for cases where you need full control.
    pub fn raw_buffer(
        mut self,
        binding: u32,
        buffer: vk::Buffer,
        offset: vk::DeviceSize,
        range: vk::DeviceSize,
        descriptor_type: vk::DescriptorType,
    ) -> Self {
        let desc_binding = match descriptor_type {
            vk::DescriptorType::STORAGE_BUFFER => DescriptorBinding::StorageBuffer {
                buffer,
                offset,
                range,
            },
            vk::DescriptorType::UNIFORM_BUFFER => DescriptorBinding::UniformBuffer {
                buffer,
                offset,
                range,
            },
            _ => panic!("raw_buffer only supports STORAGE_BUFFER or UNIFORM_BUFFER"),
        };
        self.bindings.push((binding, desc_binding));
        self
    }

    // ========================================================================
    // Image bindings
    // ========================================================================

    /// Add a sampled image binding.
    pub fn sampled_image(mut self, binding: u32, view: VkImageView) -> Self {
        self.bindings.push((
            binding,
            DescriptorBinding::SampledImage {
                view: view.vk(),
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            },
        ));
        self
    }

    /// Add a sampled image binding with custom layout.
    pub fn sampled_image_with_layout(
        mut self,
        binding: u32,
        view: VkImageView,
        layout: vk::ImageLayout,
    ) -> Self {
        self.bindings.push((
            binding,
            DescriptorBinding::SampledImage {
                view: view.vk(),
                layout,
            },
        ));
        self
    }

    /// Add a sampler binding.
    pub fn sampler(mut self, binding: u32, sampler: VkSampler) -> Self {
        self.bindings.push((
            binding,
            DescriptorBinding::Sampler {
                sampler: sampler.vk(),
            },
        ));
        self
    }

    /// Add a combined image sampler binding.
    pub fn combined_image_sampler(
        mut self,
        binding: u32,
        view: VkImageView,
        sampler: VkSampler,
    ) -> Self {
        self.bindings.push((
            binding,
            DescriptorBinding::CombinedImageSampler {
                view: view.vk(),
                sampler: sampler.vk(),
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            },
        ));
        self
    }

    /// Add a storage image binding.
    pub fn storage_image(mut self, binding: u32, view: VkImageView) -> Self {
        self.bindings.push((
            binding,
            DescriptorBinding::StorageImage {
                view: view.vk(),
                layout: vk::ImageLayout::GENERAL,
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
    pub fn build(self, layout: VkDescriptorSetLayout) -> Result<DescriptorSet, vk::Result> {
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
        let mut image_infos: Vec<vk::DescriptorImageInfo> = Vec::new();

        for (_, binding) in &self.bindings {
            match binding {
                DescriptorBinding::StorageBuffer { buffer, offset, range }
                | DescriptorBinding::UniformBuffer { buffer, offset, range } => {
                    buffer_infos.push(
                        vk::DescriptorBufferInfo::default()
                            .buffer(*buffer)
                            .offset(*offset)
                            .range(*range),
                    );
                }
                DescriptorBinding::SampledImage { view, layout } => {
                    image_infos.push(
                        vk::DescriptorImageInfo::default()
                            .sampler(vk::Sampler::null())
                            .image_view(*view)
                            .image_layout(*layout),
                    );
                }
                DescriptorBinding::Sampler { sampler } => {
                    image_infos.push(
                        vk::DescriptorImageInfo::default()
                            .sampler(*sampler)
                            .image_view(vk::ImageView::null())
                            .image_layout(vk::ImageLayout::UNDEFINED),
                    );
                }
                DescriptorBinding::CombinedImageSampler {
                    view,
                    sampler,
                    layout,
                } => {
                    image_infos.push(
                        vk::DescriptorImageInfo::default()
                            .sampler(*sampler)
                            .image_view(*view)
                            .image_layout(*layout),
                    );
                }
                DescriptorBinding::StorageImage { view, layout } => {
                    image_infos.push(
                        vk::DescriptorImageInfo::default()
                            .sampler(vk::Sampler::null())
                            .image_view(*view)
                            .image_layout(*layout),
                    );
                }
            }
        }

        // Build writes referencing the infos
        let mut writes: Vec<vk::WriteDescriptorSet> = Vec::new();
        let mut buffer_idx = 0;
        let mut image_idx = 0;

        for (binding_num, binding) in &self.bindings {
            let write = match binding {
                DescriptorBinding::StorageBuffer { .. } | DescriptorBinding::UniformBuffer { .. } => {
                    let write = vk::WriteDescriptorSet::default()
                        .dst_set(descriptor_set)
                        .dst_binding(*binding_num)
                        .dst_array_element(0)
                        .descriptor_type(binding.descriptor_type())
                        .buffer_info(std::slice::from_ref(&buffer_infos[buffer_idx]));
                    buffer_idx += 1;
                    write
                }
                DescriptorBinding::SampledImage { .. }
                | DescriptorBinding::Sampler { .. }
                | DescriptorBinding::CombinedImageSampler { .. }
                | DescriptorBinding::StorageImage { .. } => {
                    let write = vk::WriteDescriptorSet::default()
                        .dst_set(descriptor_set)
                        .dst_binding(*binding_num)
                        .dst_array_element(0)
                        .descriptor_type(binding.descriptor_type())
                        .image_info(std::slice::from_ref(&image_infos[image_idx]));
                    image_idx += 1;
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

    /// Build and take ownership of the layout.
    ///
    /// Use this when the descriptor set should own its layout for cleanup.
    pub fn build_with_owned_layout(
        self,
        layout: VkDescriptorSetLayout,
    ) -> Result<DescriptorSet, vk::Result> {
        let layout_vk: vk::DescriptorSetLayout = layout.into();
        let mut descriptor_set = self.build(layout)?;
        descriptor_set.owned_layout = Some(layout_vk);
        Ok(descriptor_set)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor_binding_types() {
        let storage = DescriptorBinding::StorageBuffer {
            buffer: vk::Buffer::null(),
            offset: 0,
            range: 1024,
        };
        assert_eq!(storage.descriptor_type(), vk::DescriptorType::STORAGE_BUFFER);

        let uniform = DescriptorBinding::UniformBuffer {
            buffer: vk::Buffer::null(),
            offset: 0,
            range: 256,
        };
        assert_eq!(uniform.descriptor_type(), vk::DescriptorType::UNIFORM_BUFFER);

        let sampled = DescriptorBinding::SampledImage {
            view: vk::ImageView::null(),
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        };
        assert_eq!(sampled.descriptor_type(), vk::DescriptorType::SAMPLED_IMAGE);

        let sampler = DescriptorBinding::Sampler {
            sampler: vk::Sampler::null(),
        };
        assert_eq!(sampler.descriptor_type(), vk::DescriptorType::SAMPLER);
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
