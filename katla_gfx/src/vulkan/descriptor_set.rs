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
    /// Create a new descriptor set from raw Vulkan handles.
    ///
    /// # Safety
    /// The caller must ensure that all handles are valid and that the
    /// descriptor set is properly allocated from the pool.
    pub(crate) fn from_raw(
        set: vk::DescriptorSet,
        pool: vk::DescriptorPool,
        owned_layout: Option<vk::DescriptorSetLayout>,
        device: ash::Device,
    ) -> Self {
        Self {
            set,
            pool,
            owned_layout,
            device,
        }
    }

    /// Get the raw Vulkan descriptor set handle.
    pub(crate) fn vk(&self) -> vk::DescriptorSet {
        self.set
    }

    /// Get the descriptor set layout.
    ///
    /// Panics if the layout was not stored during creation.
    pub(crate) fn layout(&self) -> vk::DescriptorSetLayout {
        self.owned_layout
            .expect("DescriptorSet layout not stored during creation")
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
