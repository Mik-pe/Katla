//! Skeleton descriptor set for GPU skeletal animation.
//!
//! Provides a descriptor set that binds a SkeletonBuffer to Set 2
//! for use with skinned shaders.

use ash::vk;
use std::cell::RefCell;
use std::rc::Rc;

use super::VulkanContext;
use crate::vulkan::skeleton_buffer::SkeletonBuffer;

/// Descriptor set for binding skeleton joint matrices to the GPU.
///
/// Each animated mesh needs its own SkeletonDescriptorSet that wraps
/// its SkeletonBuffer. This is bound as Set 2 in the pipeline.
pub struct SkeletonDescriptorSet {
    descriptor_set: vk::DescriptorSet,
    descriptor_pool: vk::DescriptorPool,
    skeleton_buffer: Rc<RefCell<SkeletonBuffer>>,
    context: Rc<VulkanContext>,
}

impl SkeletonDescriptorSet {
    /// Create a new skeleton descriptor set for the given skeleton buffer.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `skeleton_buffer` - The skeleton buffer containing joint matrices (wrapped in RefCell for mutation)
    /// * `layout` - The skeleton descriptor set layout from the pipeline
    pub fn new(
        context: Rc<VulkanContext>,
        skeleton_buffer: Rc<RefCell<SkeletonBuffer>>,
        layout: vk::DescriptorSetLayout,
    ) -> Result<Self, vk::Result> {
        // Create a descriptor pool for this skeleton
        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        }];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_sizes);

        let descriptor_pool = unsafe {
            context.device.create_descriptor_pool(&pool_info, None)?
        };

        // Allocate descriptor set
        let layouts = [layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe {
            context.device.allocate_descriptor_sets(&alloc_info)?
        };
        let descriptor_set = descriptor_sets[0];

        // Update descriptor set to point to skeleton buffer
        let buffer = skeleton_buffer.borrow();
        let buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(buffer.buffer())
            .offset(0)
            .range(buffer.size())];
        drop(buffer); // Release borrow before moving skeleton_buffer

        let write = [vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&buffer_info)];

        unsafe {
            context.device.update_descriptor_sets(&write, &[]);
        }

        Ok(Self {
            descriptor_set,
            descriptor_pool,
            skeleton_buffer,
            context,
        })
    }

    /// Get the descriptor set handle.
    pub fn set(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }
}

impl Drop for SkeletonDescriptorSet {
    fn drop(&mut self) {
        unsafe {
            // Destroying the pool automatically frees all descriptor sets in it
            self.context
                .device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}
