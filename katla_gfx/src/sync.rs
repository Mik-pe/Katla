//! Wrapper types for Vulkan objects.
//!
//! This module provides wrapper types for Vulkan objects to avoid exposing
//! `ash::vk` types in the public API of katla_gfx.

use ash::vk;

// Re-export Vulkan 1.3 synchronization types for barrier helpers
pub(crate) use ash::vk::AccessFlags2;

/// Type alias for Vulkan pipeline stage flags 2.
pub(crate) type PipelineStage2Flags = ash::vk::PipelineStageFlags2;

//=============================================================================
// Common Subresource Range Constants
//=============================================================================

/// Standard color subresource range for single-layer images.
///
/// Use this for most color attachment and texture operations.
pub(crate) const COLOR_SUBRESOURCE_RANGE: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};

/// Standard depth-stencil subresource range for depth buffers.
///
/// Use this for depth attachment operations.
pub(crate) const DEPTH_SUBRESOURCE_RANGE: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::from_raw(
        vk::ImageAspectFlags::DEPTH.as_raw() | vk::ImageAspectFlags::STENCIL.as_raw(),
    ),
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};

macro_rules! define_vk_wrapper {
    ($name:ident, $vk_type:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(pub $vk_type);

        unsafe impl Send for $name {}
        unsafe impl Sync for $name {}

        impl $name {
            #[allow(dead_code)]
            pub fn new(handle: $vk_type) -> Self {
                Self(handle)
            }

            #[allow(dead_code)]
            pub(crate) fn vk(&self) -> $vk_type {
                self.0
            }
        }

        impl From<$vk_type> for $name {
            fn from(handle: $vk_type) -> Self {
                Self(handle)
            }
        }

        impl From<$name> for $vk_type {
            fn from(wrapper: $name) -> Self {
                wrapper.0
            }
        }

        impl AsRef<$vk_type> for $name {
            fn as_ref(&self) -> &$vk_type {
                &self.0
            }
        }
    };
    ($name:ident, $vk_type:ty, default) => {
        define_vk_wrapper!($name, $vk_type);

        impl Default for $name {
            fn default() -> Self {
                Self(<$vk_type>::null())
            }
        }
    };
}

define_vk_wrapper!(VkSemaphore, vk::Semaphore);
define_vk_wrapper!(VkFence, vk::Fence);
define_vk_wrapper!(VkSampler, vk::Sampler);
define_vk_wrapper!(VkRenderPass, vk::RenderPass, default);
define_vk_wrapper!(VkDescriptorSet, vk::DescriptorSet);
define_vk_wrapper!(VkDescriptorSetLayout, vk::DescriptorSetLayout);
define_vk_wrapper!(VkDescriptorPool, vk::DescriptorPool);
define_vk_wrapper!(VkPipeline, vk::Pipeline, default);
define_vk_wrapper!(VkPipelineLayout, vk::PipelineLayout, default);
define_vk_wrapper!(VkBuffer, vk::Buffer, default);
define_vk_wrapper!(VkShaderModule, vk::ShaderModule);

//=============================================================================
// Crate-local wrapper types (not public API)
//=============================================================================

/// Wrapper for Vulkan image handle (crate-local).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct VkImage(pub vk::Image);

unsafe impl Send for VkImage {}
unsafe impl Sync for VkImage {}

impl VkImage {
    pub(crate) fn new(handle: vk::Image) -> Self {
        Self(handle)
    }

    pub(crate) fn vk(&self) -> vk::Image {
        self.0
    }
}

impl From<vk::Image> for VkImage {
    fn from(handle: vk::Image) -> Self {
        Self(handle)
    }
}

impl From<VkImage> for vk::Image {
    fn from(wrapper: VkImage) -> Self {
        wrapper.0
    }
}

/// Wrapper for Vulkan image view handle (crate-local).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct VkImageView(pub vk::ImageView);

unsafe impl Send for VkImageView {}
unsafe impl Sync for VkImageView {}

impl VkImageView {
    pub(crate) fn new(handle: vk::ImageView) -> Self {
        Self(handle)
    }

    pub(crate) fn vk(&self) -> vk::ImageView {
        self.0
    }
}

impl From<vk::ImageView> for VkImageView {
    fn from(handle: vk::ImageView) -> Self {
        Self(handle)
    }
}

impl From<VkImageView> for vk::ImageView {
    fn from(wrapper: VkImageView) -> Self {
        wrapper.0
    }
}

//=============================================================================
// Dynamic Rendering Types (Vulkan 1.3)
//=============================================================================

/// Wrapper for Vulkan 1.3 Viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct VkViewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

impl VkViewport {
    /// Create a new viewport.
    pub(crate) fn new(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        min_depth: f32,
        max_depth: f32,
    ) -> Self {
        Self {
            x,
            y,
            width,
            height,
            min_depth,
            max_depth,
        }
    }

    /// Create a viewport from position and size (uses default depth range 0.0-1.0).
    pub(crate) fn from_rect(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            min_depth: 0.0,
            max_depth: 1.0,
        }
    }
}

impl From<VkViewport> for vk::Viewport {
    fn from(viewport: VkViewport) -> Self {
        vk::Viewport::default()
            .x(viewport.x)
            .y(viewport.y)
            .width(viewport.width)
            .height(viewport.height)
            .min_depth(viewport.min_depth)
            .max_depth(viewport.max_depth)
    }
}

/// Wrapper for Vulkan 1.3 Rect2D (scissor rectangle).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect2D {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect2D {
    /// Create a new 2D rectangle.
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Create a rectangle from an extent (position 0,0).
    pub fn from_extent(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }
}

impl From<Rect2D> for vk::Rect2D {
    fn from(rect: Rect2D) -> Self {
        vk::Rect2D::default()
            .offset(vk::Offset2D {
                x: rect.x,
                y: rect.y,
            })
            .extent(vk::Extent2D {
                width: rect.width,
                height: rect.height,
            })
    }
}

//=============================================================================
// Synchronization2 Wrapper Types (Vulkan 1.3)
//=============================================================================

/// Image memory barrier 2 for Vulkan 1.3 synchronization.
///
/// This structure provides a more flexible and expressive way to describe
/// image memory barriers compared to the legacy vk::ImageMemoryBarrier.
#[derive(Clone, Debug)]
pub struct ImageMemoryBarrier2 {
    pub(crate) src_stage_mask: PipelineStage2Flags,
    pub(crate) dst_stage_mask: PipelineStage2Flags,
    pub(crate) src_access_mask: AccessFlags2,
    pub(crate) dst_access_mask: AccessFlags2,
    pub(crate) old_layout: vk::ImageLayout,
    pub(crate) new_layout: vk::ImageLayout,
    pub(crate) src_queue_family_index: u32,
    pub(crate) dst_queue_family_index: u32,
    pub(crate) image: VkImage,
    pub(crate) subresource_range: vk::ImageSubresourceRange,
}

impl ImageMemoryBarrier2 {
    /// Create a new image memory barrier 2.
    pub(crate) fn new(image: VkImage) -> Self {
        Self {
            src_stage_mask: PipelineStage2Flags::TOP_OF_PIPE,
            dst_stage_mask: PipelineStage2Flags::BOTTOM_OF_PIPE,
            src_access_mask: AccessFlags2::NONE,
            dst_access_mask: AccessFlags2::NONE,
            old_layout: vk::ImageLayout::UNDEFINED,
            new_layout: vk::ImageLayout::UNDEFINED,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            image,
            subresource_range: vk::ImageSubresourceRange::default(),
        }
    }

    /// Set source stage mask.
    pub(crate) fn src_stage(mut self, stage: PipelineStage2Flags) -> Self {
        self.src_stage_mask = stage;
        self
    }

    /// Set destination stage mask.
    pub(crate) fn dst_stage(mut self, stage: PipelineStage2Flags) -> Self {
        self.dst_stage_mask = stage;
        self
    }

    /// Set source access mask.
    pub(crate) fn src_access(mut self, access: AccessFlags2) -> Self {
        self.src_access_mask = access;
        self
    }

    /// Set destination access mask.
    pub(crate) fn dst_access(mut self, access: AccessFlags2) -> Self {
        self.dst_access_mask = access;
        self
    }

    /// Set old image layout.
    pub(crate) fn old_layout(mut self, layout: vk::ImageLayout) -> Self {
        self.old_layout = layout;
        self
    }

    /// Set new image layout.
    pub(crate) fn new_layout(mut self, layout: vk::ImageLayout) -> Self {
        self.new_layout = layout;
        self
    }

    /// Set image subresource range.
    pub(crate) fn subresource_range(mut self, range: vk::ImageSubresourceRange) -> Self {
        self.subresource_range = range;
        self
    }

    /// Convert to Vulkan vk::ImageMemoryBarrier2KHR.
    pub(crate) fn into_vk(self) -> vk::ImageMemoryBarrier2<'static> {
        vk::ImageMemoryBarrier2::default()
            .src_stage_mask(self.src_stage_mask)
            .dst_stage_mask(self.dst_stage_mask)
            .src_access_mask(self.src_access_mask)
            .dst_access_mask(self.dst_access_mask)
            .old_layout(self.old_layout)
            .new_layout(self.new_layout)
            .src_queue_family_index(self.src_queue_family_index)
            .dst_queue_family_index(self.dst_queue_family_index)
            .image(self.image.vk())
            .subresource_range(self.subresource_range)
    }
}

/// Buffer memory barrier 2 for Vulkan 1.3 synchronization.
///
/// This structure provides a builder pattern for buffer memory barriers,
/// similar to `ImageMemoryBarrier2` but for buffer resources.
/// Used for compute-graphics synchronization with particle buffers.
#[derive(Clone, Debug)]
pub struct BufferMemoryBarrier2 {
    pub(crate) src_stage_mask: PipelineStage2Flags,
    pub(crate) dst_stage_mask: PipelineStage2Flags,
    pub(crate) src_access_mask: AccessFlags2,
    pub(crate) dst_access_mask: AccessFlags2,
    pub(crate) src_queue_family_index: u32,
    pub(crate) dst_queue_family_index: u32,
    pub(crate) buffer: VkBuffer,
    pub(crate) offset: vk::DeviceSize,
    pub(crate) size: vk::DeviceSize,
}

impl BufferMemoryBarrier2 {
    /// Set source stage mask.
    pub fn src_stage(mut self, stage: PipelineStage2Flags) -> Self {
        self.src_stage_mask = stage;
        self
    }

    /// Set destination stage mask.
    pub fn dst_stage(mut self, stage: PipelineStage2Flags) -> Self {
        self.dst_stage_mask = stage;
        self
    }

    /// Set source access mask.
    pub fn src_access(mut self, access: AccessFlags2) -> Self {
        self.src_access_mask = access;
        self
    }

    /// Set destination access mask.
    pub fn dst_access(mut self, access: AccessFlags2) -> Self {
        self.dst_access_mask = access;
        self
    }

    /// Convert to Vulkan vk::BufferMemoryBarrier2KHR.
    pub(crate) fn into_vk(self) -> vk::BufferMemoryBarrier2KHR<'static> {
        vk::BufferMemoryBarrier2KHR::default()
            .src_stage_mask(self.src_stage_mask)
            .dst_stage_mask(self.dst_stage_mask)
            .src_access_mask(self.src_access_mask)
            .dst_access_mask(self.dst_access_mask)
            .src_queue_family_index(self.src_queue_family_index)
            .dst_queue_family_index(self.dst_queue_family_index)
            .buffer(self.buffer.vk())
            .offset(self.offset)
            .size(self.size)
    }
}

/// Dependency info for Vulkan 1.3 synchronization.
///
/// This structure replaces the traditional vk::DependencyInfo
/// and provides a more flexible way to specify synchronization barriers.
#[derive(Clone, Debug)]
pub struct DependencyInfo {
    pub(crate) memory_barriers: Vec<vk::MemoryBarrier2KHR<'static>>,
    pub(crate) buffer_barriers: Vec<vk::BufferMemoryBarrier2KHR<'static>>,
    pub(crate) image_barriers: Vec<ImageMemoryBarrier2>,
}

impl DependencyInfo {
    /// Create a new dependency info.
    pub fn new() -> Self {
        Self {
            memory_barriers: Vec::new(),
            buffer_barriers: Vec::new(),
            image_barriers: Vec::new(),
        }
    }

    /// Add a buffer memory barrier using the wrapper type.
    ///
    /// This is the preferred method for adding buffer barriers.
    pub fn add_buffer_barrier2(mut self, barrier: BufferMemoryBarrier2) -> Self {
        self.buffer_barriers.push(barrier.into_vk());
        self
    }

    /// Add an image memory barrier.
    pub fn add_image_barrier(mut self, barrier: ImageMemoryBarrier2) -> Self {
        self.image_barriers.push(barrier);
        self
    }

    /// Build and execute with the given callback.
    /// This handles the lifetime issues by keeping the Vulkan structs alive during the callback.
    pub fn build<F, R>(self, f: F) -> R
    where
        F: FnOnce(&vk::DependencyInfoKHR) -> R,
    {
        let image_barriers_vk: Vec<vk::ImageMemoryBarrier2KHR> = self
            .image_barriers
            .iter()
            .map(|b| b.clone().into_vk())
            .collect();

        let dep_info = vk::DependencyInfoKHR::default()
            .memory_barriers(&self.memory_barriers)
            .buffer_memory_barriers(&self.buffer_barriers)
            .image_memory_barriers(&image_barriers_vk);

        f(&dep_info)
    }
}

impl Default for DependencyInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper around `vk::CommandBuffer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct VkCommandBuffer(pub vk::CommandBuffer);

unsafe impl Send for VkCommandBuffer {}
unsafe impl Sync for VkCommandBuffer {}

impl Default for VkCommandBuffer {
    fn default() -> Self {
        Self(vk::CommandBuffer::null())
    }
}

impl From<vk::CommandBuffer> for VkCommandBuffer {
    fn from(command_buffer: vk::CommandBuffer) -> Self {
        Self(command_buffer)
    }
}

impl From<VkCommandBuffer> for vk::CommandBuffer {
    fn from(wrapper: VkCommandBuffer) -> Self {
        wrapper.0
    }
}

impl AsRef<vk::CommandBuffer> for VkCommandBuffer {
    fn as_ref(&self) -> &vk::CommandBuffer {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semaphore_wrapper() {
        let vk_sem = vk::Semaphore::null();
        let sem = VkSemaphore::new(vk_sem);
        assert_eq!(sem.vk(), vk_sem);
    }

    #[test]
    fn test_fence_wrapper() {
        let vk_fence = vk::Fence::null();
        let fence = VkFence::new(vk_fence);
        assert_eq!(fence.vk(), vk_fence);
    }

    #[test]
    fn test_semaphore_conversions() {
        let vk_sem = vk::Semaphore::null();
        let sem: VkSemaphore = vk_sem.into();
        let back: vk::Semaphore = sem.into();
        assert_eq!(vk_sem, back);
    }

    #[test]
    fn test_fence_conversions() {
        let vk_fence = vk::Fence::null();
        let fence: VkFence = vk_fence.into();
        let back: vk::Fence = fence.into();
        assert_eq!(vk_fence, back);
    }

    // Synchronization2 tests

    #[test]
    fn test_access_flags2() {
        let access = AccessFlags2::COLOR_ATTACHMENT_WRITE;
        assert!(!access.is_empty());

        let empty = AccessFlags2::NONE;
        assert!(empty.is_empty());
    }

    #[test]
    fn test_access_flags2_conversions() {
        let vk_flags = vk::AccessFlags2KHR::COLOR_ATTACHMENT_WRITE_KHR;
        let wrapper: AccessFlags2 = vk_flags.into();
        let back: vk::AccessFlags2KHR = wrapper.into();
        assert_eq!(vk_flags, back);
    }

    #[test]
    fn test_image_memory_barrier2_builder() {
        let image = VkImage::new(vk::Image::null());
        let barrier = ImageMemoryBarrier2::new(image)
            .src_stage(PipelineStage2Flags::TRANSFER)
            .dst_stage(PipelineStage2Flags::FRAGMENT_SHADER)
            .src_access(AccessFlags2::TRANSFER_WRITE)
            .dst_access(AccessFlags2::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        assert_eq!(barrier.image, image);
        assert_eq!(barrier.old_layout, vk::ImageLayout::TRANSFER_DST_OPTIMAL);
        assert_eq!(
            barrier.new_layout,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        );
    }

    #[test]
    fn test_dependency_info() {
        let image = VkImage::new(vk::Image::null());
        let barrier = ImageMemoryBarrier2::new(image)
            .src_stage(PipelineStage2Flags::TRANSFER)
            .dst_stage(PipelineStage2Flags::FRAGMENT_SHADER);

        let dep_info = DependencyInfo::new().add_image_barrier(barrier);

        assert_eq!(dep_info.image_barriers.len(), 1);
    }

    #[test]
    fn test_dependency_info_default() {
        let dep_info = DependencyInfo::default();
        assert!(dep_info.memory_barriers.is_empty());
        assert!(dep_info.buffer_barriers.is_empty());
        assert!(dep_info.image_barriers.is_empty());
    }

    #[test]
    fn test_pipeline_wrapper() {
        let vk_pipeline = vk::Pipeline::null();
        let pipeline = VkPipeline::new(vk_pipeline);
        assert_eq!(pipeline.vk(), vk_pipeline);
    }

    #[test]
    fn test_pipeline_conversions() {
        let vk_pipeline = vk::Pipeline::null();
        let pipeline: VkPipeline = vk_pipeline.into();
        let back: vk::Pipeline = pipeline.into();
        assert_eq!(vk_pipeline, back);
    }

    #[test]
    fn test_pipeline_layout_wrapper() {
        let vk_layout = vk::PipelineLayout::null();
        let layout = VkPipelineLayout::new(vk_layout);
        assert_eq!(layout.vk(), vk_layout);
    }

    #[test]
    fn test_pipeline_layout_conversions() {
        let vk_layout = vk::PipelineLayout::null();
        let layout: VkPipelineLayout = vk_layout.into();
        let back: vk::PipelineLayout = layout.into();
        assert_eq!(vk_layout, back);
    }

    #[test]
    fn test_buffer_wrapper() {
        let vk_buffer = vk::Buffer::null();
        let buffer = VkBuffer::new(vk_buffer);
        assert_eq!(buffer.vk(), vk_buffer);
    }

    #[test]
    fn test_buffer_conversions() {
        let vk_buffer = vk::Buffer::null();
        let buffer: VkBuffer = vk_buffer.into();
        let back: vk::Buffer = buffer.into();
        assert_eq!(vk_buffer, back);
    }
}
