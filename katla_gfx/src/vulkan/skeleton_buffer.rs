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
    /// Size of the buffer in bytes
    size: vk::DeviceSize,
}

impl SkeletonBuffer {
    /// Create a new skeleton buffer for the given number of joints.
    ///
    /// Each joint requires a 4x4 matrix (64 bytes).
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `joint_count` - Number of joints in the skeleton
    pub fn new(context: Rc<VulkanContext>, joint_count: usize) -> Self {
        use gpu_allocator::MemoryLocation;

        let buffer_size = (joint_count * 64) as vk::DeviceSize;

        let buffer_info = vk::BufferCreateInfo::default()
            .size(buffer_size)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (buffer, allocation) = context.allocate_buffer(&buffer_info, MemoryLocation::CpuToGpu);

        Self {
            buffer,
            allocation: Some(allocation),
            context,
            size: buffer_size,
        }
    }

    /// Update the skeleton buffer with new joint matrices.
    ///
    /// Matrices are written directly to mapped memory.
    ///
    /// # Arguments
    /// * `matrices` - Slice of joint matrices, each as [f32; 16] in column-major format
    pub fn update(&mut self, matrices: &[[f32; 16]]) {
        if let Some(allocation) = &self.allocation {
            if let Some(mapped_ptr) = allocation.mapped_ptr() {
                let dst = mapped_ptr.as_ptr() as *mut f32;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        matrices.as_ptr() as *const f32,
                        dst,
                        matrices.len() * 16,
                    );
                }
                // Flush the buffer to make CPU writes visible to GPU
                self.context.flush_mapped_memory(allocation, 0, self.size);
            } else {
                // Map manually if not persistently mapped
                let ptr = self.context.map_buffer(allocation);
                let dst = ptr as *mut f32;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        matrices.as_ptr() as *const f32,
                        dst,
                        matrices.len() * 16,
                    );
                }
                self.context.flush_mapped_memory(allocation, 0, self.size);
            }
        }
    }

    /// Get the raw Vulkan buffer handle.
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer
    }

    /// Get the buffer size in bytes.
    pub fn size(&self) -> vk::DeviceSize {
        self.size
    }
}

impl Drop for SkeletonBuffer {
    fn drop(&mut self) {
        if let Some(allocation) = self.allocation.take() {
            self.context.free_buffer(self.buffer, allocation);
        }
    }
}
