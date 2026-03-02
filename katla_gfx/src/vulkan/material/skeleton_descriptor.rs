//! Skeleton descriptor set for GPU skeletal animation.
//!
//! Provides a descriptor set that binds a SkeletonBuffer to Set 2
//! for use with skinned shaders.
//!
//! This is a convenience wrapper around [`DescriptorSetBuilder`]
//! for the common case of binding a single skeleton buffer.

use ash::vk;
use std::cell::RefCell;
use std::rc::Rc;

use crate::vulkan::skeleton_buffer::SkeletonBuffer;

/// Descriptor set for binding skeleton joint matrices to the GPU.
///
/// Each animated mesh needs its own SkeletonDescriptorSet that wraps
/// its SkeletonBuffer. This is bound as Set 2 in the pipeline.
///
/// # Example
/// ```ignore
/// let skeleton_buffer = Rc::new(RefCell::new(SkeletonBuffer::new(context.clone(), joint_count)));
/// let descriptor_set = SkeletonDescriptorSet::new(
///     context,
///     skeleton_buffer.clone(),
///     skeleton_layout
/// )?;
///
/// // In render loop:
/// device.cmd_bind_descriptor_sets(cmd, GRAPHICS, layout, 2, &[descriptor_set.set()], &[]);
/// ```
pub struct SkeletonDescriptorSet {
    inner: crate::vulkan::DescriptorSet,
    #[allow(dead_code)]
    skeleton_buffer: Rc<RefCell<SkeletonBuffer>>,
}

impl SkeletonDescriptorSet {
    /// Get the raw Vulkan descriptor set handle.
    pub fn vk_set(&self) -> vk::DescriptorSet {
        self.inner.vk()
    }
}
