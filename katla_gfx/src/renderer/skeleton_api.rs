use super::*;

impl VulkanRenderer {
    /// Get the skeleton descriptor set for a handle.
    pub fn get_skeleton_descriptor(
        &self,
        handle: SkeletonHandle,
    ) -> Option<&SkeletonDescriptorSet> {
        self.skeleton_descriptors.get(handle.index())
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

        // Store both the descriptor and the buffer with matching IDs
        let id = self.skeleton_descriptors.insert(descriptor_set);
        let _ = self.skeleton_buffers.insert(buffer);
        let handle = SkeletonHandle::new(id);

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
        if let Some(buffer) = self.skeleton_buffers.get_mut(handle.index()) {
            buffer.update(matrices);
        }
    }

    /// Copy joint matrices from the animation compute output buffer to a
    /// specific entity's skeleton buffer via GPU-side buffer copy.
    ///
    /// Call this after the animation compute dispatch and its barrier.
    ///
    /// # Arguments
    /// * `cmd` - Command buffer to record the copy on
    /// * `skeleton_handle` - Target skeleton buffer handle
    /// * `src_buffer` - Animation compute output buffer (TRANSFER_SRC)
    /// * `joint_offset` - Offset into src_buffer in bytes (joint_index * 64)
    /// * `joint_count` - Number of joints to copy
    pub fn copy_skeleton_from_compute_output(
        &self,
        cmd: vk::CommandBuffer,
        skeleton_handle: SkeletonHandle,
        src_buffer: vk::Buffer,
        joint_offset: u32,
        joint_count: u32,
    ) {
        let dst_buffer = match self
            .skeleton_buffers
            .get(skeleton_handle.index())
            .map(|b| b.buffer())
        {
            Some(b) => b,
            None => return,
        };

        let src_offset = (joint_offset as u64) * 64;
        let size = (joint_count as u64) * 64;

        let copy_region = vk::BufferCopy::default()
            .src_offset(src_offset)
            .dst_offset(0)
            .size(size);

        unsafe {
            self.context
                .device
                .cmd_copy_buffer(cmd, src_buffer, dst_buffer, &[copy_region]);
        }
    }

    /// Get the raw Vulkan buffer for a skeleton handle.
    pub fn skeleton_buffer_handle(&self, handle: SkeletonHandle) -> Option<vk::Buffer> {
        self.skeleton_buffers
            .get(handle.index())
            .map(|b| b.buffer())
    }
}
