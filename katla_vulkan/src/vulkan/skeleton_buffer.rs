//! Skeleton buffer for GPU skeletal animation.
//!
//! Provides storage buffer for joint matrices that can be bound to the GPU
//! for vertex skinning in the shader.

use ash::vk;
use gpu_allocator::vulkan::Allocation;
use std::rc::Rc;

use super::context::VulkanContext;

/// Maximum number of joints per skeleton.
/// Must match the constant in model_pbr_skinned.wgsl
pub const MAX_JOINTS: usize = 256;

/// GPU-friendly joint matrix format (4x4 matrix as column-major [f32; 16]).
pub type JointMatrix = [f32; 16];

/// Storage buffer for skeleton joint matrices.
///
/// Each animated mesh has its own SkeletonBuffer that stores
/// the current pose as an array of 4x4 matrices.
pub struct SkeletonBuffer {
    buffer: vk::Buffer,
    allocation: Option<Allocation>,
    context: Rc<VulkanContext>,
    joint_count: usize,
}

impl SkeletonBuffer {
    /// Create a new skeleton buffer for the given number of joints.
    ///
    /// The buffer is allocated with enough space for up to MAX_JOINTS matrices,
    /// but only `joint_count` matrices will typically be uploaded.
    pub fn new(context: Rc<VulkanContext>, joint_count: usize) -> Self {
        let joint_count = joint_count.min(MAX_JOINTS);
        let buffer_size = (MAX_JOINTS * std::mem::size_of::<JointMatrix>()) as vk::DeviceSize;

        let create_info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .size(buffer_size);

        let (buffer, allocation) =
            context.allocate_buffer(&create_info, gpu_allocator::MemoryLocation::CpuToGpu);

        Self {
            buffer,
            allocation: Some(allocation),
            context,
            joint_count,
        }
    }

    /// Upload joint matrices to the GPU.
    ///
    /// The matrices should already be computed as:
    /// `joint_matrix = joint_world_transform * inverse_bind_matrix`
    ///
    /// This is the final matrix used in the shader for skinning.
    pub fn upload(&mut self, joint_matrices: &[JointMatrix]) {
        if let Some(allocation) = &self.allocation {
            let mapped_ptr = self.context.map_buffer(allocation);
            let joint_count = joint_matrices.len().min(MAX_JOINTS);
            let byte_size = joint_count * std::mem::size_of::<JointMatrix>();

            unsafe {
                std::ptr::copy_nonoverlapping(
                    joint_matrices.as_ptr() as *const u8,
                    mapped_ptr,
                    byte_size,
                );
            }
        }
    }

    /// Get the Vulkan buffer handle.
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer
    }

    /// Get the number of joints this buffer was created for.
    pub fn joint_count(&self) -> usize {
        self.joint_count
    }

    /// Get the buffer size in bytes.
    pub fn size(&self) -> vk::DeviceSize {
        (MAX_JOINTS * std::mem::size_of::<JointMatrix>()) as vk::DeviceSize
    }
}

impl Drop for SkeletonBuffer {
    fn drop(&mut self) {
        if let Some(allocation) = self.allocation.take() {
            self.context.free_buffer(crate::sync::VkBuffer::new(self.buffer), allocation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_joints_constant() {
        // Ensure MAX_JOINTS matches shader constant
        assert_eq!(MAX_JOINTS, 256);
    }

    #[test]
    fn test_joint_matrix_size() {
        // Each joint matrix should be 64 bytes (16 floats)
        assert_eq!(std::mem::size_of::<JointMatrix>(), 64);
    }
}
