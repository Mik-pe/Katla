//! Owned descriptor sets with automatic pool and layout cleanup.

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
