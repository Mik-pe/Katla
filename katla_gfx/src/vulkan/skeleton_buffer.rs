//! Skeleton buffer for GPU skeletal animation.
//!
//! Provides storage buffer for joint matrices that can be bound to the GPU
//! for vertex skinning in the shader.

use ash::vk;
use gpu_allocator::vulkan::Allocation;
use std::rc::Rc;

use super::context::VulkanContext;

/// Storage buffer for skeleton joint matrices.
///
/// Each animated mesh has its own SkeletonBuffer that stores
/// the current pose as an array of 4x4 matrices.
pub struct SkeletonBuffer {
    buffer: vk::Buffer,
    allocation: Option<Allocation>,
    context: Rc<VulkanContext>,
}

impl Drop for SkeletonBuffer {
    fn drop(&mut self) {
        if let Some(allocation) = self.allocation.take() {
            self.context.free_buffer(self.buffer, allocation);
        }
    }
}
