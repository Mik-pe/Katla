//! Skeleton descriptor set for GPU skeletal animation.
//!
//! Provides a descriptor set that binds a SkeletonBuffer to Set 2
//! for use with skinned shaders.
//!
//! This uses a shared descriptor pool and layout managed by MaterialCompiler.

use ash::vk;
use std::rc::Rc;

use crate::RendererError;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::skeleton_buffer::SkeletonBuffer;

/// Descriptor set for binding skeleton joint matrices to the GPU.
///
/// Each animated mesh needs its own SkeletonDescriptorSet that wraps
/// its SkeletonBuffer. This is bound as Set 2 in the pipeline.
///
/// Note: This does NOT own the descriptor pool or layout - those are
/// shared resources managed by MaterialCompiler. The descriptor set
/// will be automatically freed when the pool is destroyed.
///
/// # Example
/// ```ignore
/// // Get shared pool and layout from MaterialCompiler
/// let pool = material_compiler.skeleton_descriptor_pool();
/// let layout = material_compiler.skeleton_descriptor_layout();
///
/// let skeleton_buffer = SkeletonBuffer::new(context.clone(), joint_count);
/// let descriptor_set = SkeletonDescriptorSet::new(
///     context,
///     &skeleton_buffer,
///     pool,
///     layout,
/// )?;
///
/// // In render loop:
/// device.cmd_bind_descriptor_sets(cmd, GRAPHICS, layout, 2, &[descriptor_set.vk_set()], &[]);
/// ```
pub struct SkeletonDescriptorSet {
    set: vk::DescriptorSet,
}

impl SkeletonDescriptorSet {
    /// Create a new skeleton descriptor set using a shared pool and layout.
    ///
    /// Allocates a descriptor set from a shared pool and writes the skeleton buffer.
    /// The pool and layout should be created once and reused for all skeletons
    /// to avoid exhausting Vulkan descriptor limits.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `skeleton_buffer` - The skeleton buffer to bind
    /// * `pool` - Shared descriptor pool for allocating skeleton descriptor sets
    /// * `layout` - Shared descriptor set layout for skeleton binding
    ///
    /// # Returns
    /// A new SkeletonDescriptorSet, or an error if creation fails
    pub fn new(
        context: Rc<VulkanContext>,
        skeleton_buffer: &SkeletonBuffer,
        pool: vk::DescriptorPool,
        layout: vk::DescriptorSetLayout,
    ) -> Result<Self, RendererError> {
        let device = &context.device;

        // 1. Allocate descriptor set from shared pool
        let layouts = [layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);

        let sets = unsafe { device.allocate_descriptor_sets(&alloc_info)? };
        let set = sets[0];

        // 2. Write buffer info to binding 0
        let buffer_infos = [vk::DescriptorBufferInfo::default()
            .buffer(skeleton_buffer.buffer())
            .offset(0)
            .range(skeleton_buffer.size())];

        let writes = [vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .buffer_info(&buffer_infos)];

        unsafe { device.update_descriptor_sets(&writes, &[]) };

        Ok(Self { set })
    }

    /// Get the raw Vulkan descriptor set handle.
    pub fn vk_set(&self) -> vk::DescriptorSet {
        self.set
    }
}
