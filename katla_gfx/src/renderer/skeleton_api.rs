use super::*;

impl VulkanRenderer {
    /// Get the skeleton descriptor set for a handle.
    pub fn get_skeleton_descriptor(
        &self,
        handle: SkeletonHandle,
    ) -> Option<&SkeletonDescriptorSet> {
        self.skeleton_descriptors.get(handle)
    }

    /// Create a new skeleton for GPU skeletal animation.
    ///
    /// Allocates a storage buffer for joint matrices and creates a descriptor set
    /// for binding to shaders (Set 2).
    ///
    /// # Arguments
    /// * `joint_count` - Number of joints in the skeleton
    ///
    /// # Returns
    /// A SkeletonHandle for the created skeleton, or an error if creation fails.
    pub fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        use crate::vulkan::skeleton_buffer::SkeletonBuffer;

        let buffer = SkeletonBuffer::new(self.context.clone(), joint_count);

        let pool = self.material_compiler.skeleton_descriptor_pool();
        let layout = self.material_compiler.skeleton_descriptor_layout();

        let descriptor_set =
            SkeletonDescriptorSet::new(self.context.clone(), &buffer, pool, layout).map_err(
                |e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create skeleton descriptor: {:?}",
                        e
                    ))
                },
            )?;

        // Insert the descriptor and buffer back to back so both storages
        // assign the same slot and generation; one handle addresses both.
        let handle = self.skeleton_descriptors.insert(descriptor_set);
        let buffer_handle = self.skeleton_buffers.insert(buffer);
        debug_assert_eq!(handle, buffer_handle);

        Ok(handle)
    }

    /// Update skeleton joint matrices on the GPU.
    ///
    /// Uploads the current pose to the skeleton's storage buffer.
    /// Call this each frame after computing animation but before rendering.
    ///
    /// # Arguments
    /// * `handle` - Skeleton handle from `create_skeleton()`
    /// * `matrices` - Joint matrices as column-major [f32; 16] arrays (one per joint)
    pub fn update_skeleton(&mut self, handle: SkeletonHandle, matrices: &[[f32; 16]]) {
        if let Some(buffer) = self.skeleton_buffers.get_mut(handle) {
            buffer.update(matrices);
        }
    }

    /// Get the raw Vulkan buffer for a skeleton handle.
    pub fn skeleton_buffer_handle(&self, handle: SkeletonHandle) -> Option<vk::Buffer> {
        self.skeleton_buffers.get(handle).map(|b| b.buffer())
    }
}
