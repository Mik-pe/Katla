//! Skeleton descriptor set for GPU skeletal animation.
//!
//! Provides a descriptor set that binds a SkeletonBuffer to Set 2
//! for use with skinned shaders.
//!
//! This is a convenience wrapper around [`BufferDescriptorSetBuilder`]
//! for the common case of binding a single skeleton buffer.

use ash::vk;
use std::cell::RefCell;
use std::rc::Rc;

use super::{BufferDescriptorSetBuilder, VulkanContext};
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
    inner: super::BufferDescriptorSet,
    #[allow(dead_code)]
    skeleton_buffer: Rc<RefCell<SkeletonBuffer>>,
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
        // Get buffer info while holding the borrow
        let (buffer, size) = {
            let buf = skeleton_buffer.borrow();
            (buf.buffer(), buf.size())
        };

        // Use the generic builder
        let inner = BufferDescriptorSetBuilder::new(&context)
            .add_binding(buffer, 0, 0, size)
            .build(layout)?;

        Ok(Self {
            inner,
            skeleton_buffer,
        })
    }

    /// Get the descriptor set handle.
    pub fn set(&self) -> vk::DescriptorSet {
        self.inner.set()
    }
}
