//! Wrapper types for Vulkan objects.
//!
//! This module provides wrapper types for Vulkan objects to avoid exposing
//! `ash::vk` types in the public API of katla_vulkan.

use ash::vk;

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

//=============================================================================
// Barrier Helper Functions
//=============================================================================

/// Create a barrier for transitioning color image from SHADER_READ_ONLY to COLOR_ATTACHMENT.
///
/// Common pattern when rendering to a texture that was previously sampled.
#[inline]
pub(crate) fn color_read_to_attachment_barrier(image: VkImage) -> ImageMemoryBarrier2 {
    ImageMemoryBarrier2::new(image)
        .src_stage(PipelineStage2Flags::FRAGMENT_SHADER)
        .src_access(AccessFlags2::SHADER_READ)
        .dst_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .subresource_range(COLOR_SUBRESOURCE_RANGE)
}

/// Create a barrier for transitioning color image from COLOR_ATTACHMENT to SHADER_READ_ONLY.
///
/// Common pattern after rendering to a texture that will be sampled later.
#[inline]
pub(crate) fn color_attachment_to_read_barrier(image: VkImage) -> ImageMemoryBarrier2 {
    ImageMemoryBarrier2::new(image)
        .src_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
        .src_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage(PipelineStage2Flags::FRAGMENT_SHADER)
        .dst_access(AccessFlags2::SHADER_READ)
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .subresource_range(COLOR_SUBRESOURCE_RANGE)
}

/// Create a barrier for transitioning color image to TRANSFER_DST.
///
/// Common pattern when preparing to blit/copy to an image.
#[inline]
pub(crate) fn color_to_transfer_dst_barrier(image: VkImage) -> ImageMemoryBarrier2 {
    ImageMemoryBarrier2::new(image)
        .src_stage(PipelineStage2Flags::TOP_OF_PIPE)
        .src_access(AccessFlags2::NONE)
        .dst_stage(PipelineStage2Flags::TRANSFER)
        .dst_access(AccessFlags2::TRANSFER_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .subresource_range(COLOR_SUBRESOURCE_RANGE)
}

/// Create a barrier for transitioning color image from TRANSFER_DST to SHADER_READ_ONLY.
///
/// Common pattern after uploading texture data.
#[inline]
pub(crate) fn transfer_dst_to_read_barrier(image: VkImage) -> ImageMemoryBarrier2 {
    ImageMemoryBarrier2::new(image)
        .src_stage(PipelineStage2Flags::TRANSFER)
        .src_access(AccessFlags2::TRANSFER_WRITE)
        .dst_stage(PipelineStage2Flags::FRAGMENT_SHADER)
        .dst_access(AccessFlags2::SHADER_READ)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .subresource_range(COLOR_SUBRESOURCE_RANGE)
}

/// Create a depth attachment barrier for synchronization.
///
/// Ensures depth attachment is properly synchronized between passes.
#[inline]
pub(crate) fn depth_attachment_barrier(image: VkImage) -> ImageMemoryBarrier2 {
    ImageMemoryBarrier2::new(image)
        .src_stage(PipelineStage2Flags::LATE_FRAGMENT_TESTS)
        .src_access(AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
        .dst_stage(PipelineStage2Flags::EARLY_FRAGMENT_TESTS)
        .dst_access(
            AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                .union(AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE),
        )
        .old_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .subresource_range(DEPTH_SUBRESOURCE_RANGE)
}

/// Create a barrier for transitioning depth image from UNDEFINED to DEPTH_ATTACHMENT.
///
/// Common pattern when initializing a depth buffer.
#[inline]
pub(crate) fn depth_init_barrier(image: VkImage) -> ImageMemoryBarrier2 {
    ImageMemoryBarrier2::new(image)
        .src_stage(PipelineStage2Flags::TOP_OF_PIPE)
        .src_access(AccessFlags2::NONE)
        .dst_stage(PipelineStage2Flags::EARLY_FRAGMENT_TESTS)
        .dst_access(
            AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                .union(AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE),
        )
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .subresource_range(DEPTH_SUBRESOURCE_RANGE)
}

/// Create a barrier for transitioning color image from UNDEFINED to COLOR_ATTACHMENT.
///
/// Common pattern when initializing a color attachment.
#[inline]
pub(crate) fn color_init_barrier(image: VkImage) -> ImageMemoryBarrier2 {
    ImageMemoryBarrier2::new(image)
        .src_stage(PipelineStage2Flags::TOP_OF_PIPE)
        .src_access(AccessFlags2::NONE)
        .dst_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .subresource_range(COLOR_SUBRESOURCE_RANGE)
}

/// Create a barrier for transitioning swapchain image from UNDEFINED to PRESENT_SRC.
///
/// Common pattern before presenting.
#[inline]
pub(crate) fn swapchain_to_present_barrier(image: VkImage) -> ImageMemoryBarrier2 {
    ImageMemoryBarrier2::new(image)
        .src_stage(PipelineStage2Flags::TRANSFER)
        .src_access(AccessFlags2::TRANSFER_WRITE)
        .dst_stage(PipelineStage2Flags::BOTTOM_OF_PIPE)
        .dst_access(AccessFlags2::NONE)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .subresource_range(COLOR_SUBRESOURCE_RANGE)
}

macro_rules! define_vk_wrapper {
    ($name:ident, $vk_type:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub(crate) struct $name(pub $vk_type);

        unsafe impl Send for $name {}
        unsafe impl Sync for $name {}

        impl $name {
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
define_vk_wrapper!(VkImageView, vk::ImageView);
define_vk_wrapper!(VkSampler, vk::Sampler);
define_vk_wrapper!(VkImage, vk::Image);
define_vk_wrapper!(VkRenderPass, vk::RenderPass, default);
define_vk_wrapper!(VkFramebuffer, vk::Framebuffer);
define_vk_wrapper!(VkDescriptorSet, vk::DescriptorSet);
define_vk_wrapper!(VkDescriptorSetLayout, vk::DescriptorSetLayout);
define_vk_wrapper!(VkDescriptorPool, vk::DescriptorPool);
define_vk_wrapper!(VkPipeline, vk::Pipeline, default);
define_vk_wrapper!(VkPipelineLayout, vk::PipelineLayout, default);
define_vk_wrapper!(VkBuffer, vk::Buffer, default);
define_vk_wrapper!(VkShaderModule, vk::ShaderModule);

//=============================================================================
// Dynamic Rendering Types (Vulkan 1.3)
//=============================================================================

/// Wrapper for Vulkan 1.3 Viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

impl Viewport {
    /// Create a new viewport.
    pub fn new(x: f32, y: f32, width: f32, height: f32, min_depth: f32, max_depth: f32) -> Self {
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
    pub fn from_rect(x: f32, y: f32, width: f32, height: f32) -> Self {
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

impl From<Viewport> for vk::Viewport {
    fn from(viewport: Viewport) -> Self {
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

/// Wrapper for Vulkan 1.3 RenderingInfo (dynamic rendering).
#[derive(Clone, Debug)]
pub struct RenderingInfo {
    pub render_area: Rect2D,
    pub layer_count: u32,
    pub view_mask: u32,
    pub color_attachments: Vec<vk::RenderingAttachmentInfo<'static>>,
    pub depth_attachment: Option<vk::RenderingAttachmentInfo<'static>>,
    pub stencil_attachment: Option<vk::RenderingAttachmentInfo<'static>>,
}

impl RenderingInfo {
    /// Create a new rendering info.
    pub fn new(render_area: Rect2D) -> Self {
        Self {
            render_area,
            layer_count: 1,
            view_mask: 0,
            color_attachments: Vec::new(),
            depth_attachment: None,
            stencil_attachment: None,
        }
    }

    /// Set the render area.
    pub fn render_area(mut self, rect: Rect2D) -> Self {
        self.render_area = rect;
        self
    }

    /// Set the layer count.
    pub fn layer_count(mut self, count: u32) -> Self {
        self.layer_count = count;
        self
    }

    /// Add a color attachment.
    pub fn color_attachment(mut self, attachment: vk::RenderingAttachmentInfo<'static>) -> Self {
        self.color_attachments.push(attachment);
        self
    }

    /// Set the depth attachment.
    pub fn depth_attachment(mut self, attachment: vk::RenderingAttachmentInfo<'static>) -> Self {
        self.depth_attachment = Some(attachment);
        self
    }

    /// Set the stencil attachment.
    pub fn stencil_attachment(mut self, attachment: vk::RenderingAttachmentInfo<'static>) -> Self {
        self.stencil_attachment = Some(attachment);
        self
    }

    /// Build and execute with the given callback.
    /// This handles the lifetime issues by keeping the Vulkan structs alive during the callback.
    pub fn build<F, R>(self, f: F) -> R
    where
        F: FnOnce(&vk::RenderingInfo<'_>) -> R,
    {
        let vk_render_area: vk::Rect2D = self.render_area.into();

        // Build color attachments slice
        let color_attachments_vk: Vec<vk::RenderingAttachmentInfo<'_>> =
            self.color_attachments.iter().map(|a| a.clone()).collect();

        let mut rendering_info = vk::RenderingInfo::default()
            .render_area(vk_render_area)
            .layer_count(self.layer_count)
            .view_mask(self.view_mask)
            .color_attachments(&color_attachments_vk);

        // Handle optional depth attachment
        if let Some(ref depth) = self.depth_attachment {
            rendering_info = rendering_info.depth_attachment(depth);
        }

        // Handle optional stencil attachment
        if let Some(ref stencil) = self.stencil_attachment {
            rendering_info = rendering_info.stencil_attachment(stencil);
        }

        f(&rendering_info)
    }
}

//=============================================================================
// Synchronization2 Wrapper Types (Vulkan 1.3)
//=============================================================================

/// Wrapper for Vulkan 1.3 Pipeline Stage 2 flags.
/// Provides type-safe pipeline stage masks for modern synchronization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipelineStage2Flags(pub vk::PipelineStageFlags2KHR);

impl PipelineStage2Flags {
    /// No stage.
    pub const NONE: Self = Self(vk::PipelineStageFlags2KHR::empty());

    /// Top of pipe.
    pub const TOP_OF_PIPE: Self = Self(vk::PipelineStageFlags2KHR::TOP_OF_PIPE_KHR);

    /// Draw indirect.
    pub const DRAW_INDIRECT: Self = Self(vk::PipelineStageFlags2KHR::DRAW_INDIRECT_KHR);

    /// Vertex input.
    pub const VERTEX_INPUT: Self = Self(vk::PipelineStageFlags2KHR::VERTEX_INPUT_KHR);

    /// Vertex shader.
    pub const VERTEX_SHADER: Self = Self(vk::PipelineStageFlags2KHR::VERTEX_SHADER_KHR);

    /// Fragment shader.
    pub const FRAGMENT_SHADER: Self = Self(vk::PipelineStageFlags2KHR::FRAGMENT_SHADER_KHR);

    /// Early fragment tests.
    pub const EARLY_FRAGMENT_TESTS: Self =
        Self(vk::PipelineStageFlags2KHR::EARLY_FRAGMENT_TESTS_KHR);

    /// Late fragment tests.
    pub const LATE_FRAGMENT_TESTS: Self = Self(vk::PipelineStageFlags2KHR::LATE_FRAGMENT_TESTS_KHR);

    /// Color attachment output.
    pub const COLOR_ATTACHMENT_OUTPUT: Self =
        Self(vk::PipelineStageFlags2KHR::COLOR_ATTACHMENT_OUTPUT_KHR);

    /// Compute shader.
    pub const COMPUTE_SHADER: Self = Self(vk::PipelineStageFlags2KHR::COMPUTE_SHADER_KHR);

    /// Transfer.
    pub const TRANSFER: Self = Self(vk::PipelineStageFlags2KHR::TRANSFER_KHR);

    /// Bottom of pipe.
    pub const BOTTOM_OF_PIPE: Self = Self(vk::PipelineStageFlags2KHR::BOTTOM_OF_PIPE_KHR);

    /// Host.
    pub const HOST: Self = Self(vk::PipelineStageFlags2KHR::HOST_KHR);

    /// All graphics.
    pub const ALL_GRAPHICS: Self = Self(vk::PipelineStageFlags2KHR::ALL_GRAPHICS_KHR);

    /// All commands.
    pub const ALL_COMMANDS: Self = Self(vk::PipelineStageFlags2KHR::ALL_COMMANDS_KHR);

    /// Copy.
    pub const COPY: Self = Self(vk::PipelineStageFlags2KHR::COPY_KHR);

    /// Resolve.
    pub const RESOLVE: Self = Self(vk::PipelineStageFlags2KHR::RESOLVE_KHR);

    /// Blit.
    pub const BLIT: Self = Self(vk::PipelineStageFlags2KHR::BLIT_KHR);

    /// Clear.
    pub const CLEAR: Self = Self(vk::PipelineStageFlags2KHR::CLEAR_KHR);

    /// Index input.
    pub const INDEX_INPUT: Self = Self(vk::PipelineStageFlags2KHR::INDEX_INPUT_KHR);

    /// Vertex attribute input.
    pub const VERTEX_ATTRIBUTE_INPUT: Self =
        Self(vk::PipelineStageFlags2KHR::VERTEX_ATTRIBUTE_INPUT_KHR);

    /// Pre-rasterization shaders.
    pub const PRE_RASTERIZATION_SHADERS: Self =
        Self(vk::PipelineStageFlags2KHR::PRE_RASTERIZATION_SHADERS_KHR);

    /// Get the raw Vulkan flags.
    pub(crate) fn into_vk(self) -> vk::PipelineStageFlags2KHR {
        self.0
    }

    /// Check if any flags are set.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Check if all specified flags are set.
    pub fn contains(&self, other: Self) -> bool {
        self.0.contains(other.0)
    }

    /// Bitwise OR.
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl Default for PipelineStage2Flags {
    fn default() -> Self {
        Self::NONE
    }
}

impl From<vk::PipelineStageFlags2KHR> for PipelineStage2Flags {
    fn from(flags: vk::PipelineStageFlags2KHR) -> Self {
        Self(flags)
    }
}

impl From<PipelineStage2Flags> for vk::PipelineStageFlags2KHR {
    fn from(wrapper: PipelineStage2Flags) -> Self {
        wrapper.0
    }
}

/// Conversion from legacy `vk::PipelineStageFlags` to modern `PipelineStage2Flags`.
/// This enables gradual migration from Vulkan 1.0 to Vulkan 1.3 synchronization.
impl From<vk::PipelineStageFlags> for PipelineStage2Flags {
    fn from(flags: vk::PipelineStageFlags) -> Self {
        Self(vk::PipelineStageFlags2KHR::from_raw(flags.as_raw() as u64))
    }
}

/// Wrapper for Vulkan 1.3 Access 2 flags.
/// Provides type-safe access masks for modern synchronization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AccessFlags2(pub vk::AccessFlags2KHR);

impl AccessFlags2 {
    /// No access.
    pub const NONE: Self = Self(vk::AccessFlags2KHR::empty());

    /// Indirect command read.
    pub const INDIRECT_COMMAND_READ: Self = Self(vk::AccessFlags2KHR::INDIRECT_COMMAND_READ_KHR);

    /// Index read.
    pub const INDEX_READ: Self = Self(vk::AccessFlags2KHR::INDEX_READ_KHR);

    /// Vertex attribute read.
    pub const VERTEX_ATTRIBUTE_READ: Self = Self(vk::AccessFlags2KHR::VERTEX_ATTRIBUTE_READ_KHR);

    /// Uniform read.
    pub const UNIFORM_READ: Self = Self(vk::AccessFlags2KHR::UNIFORM_READ_KHR);

    /// Input attachment read.
    pub const INPUT_ATTACHMENT_READ: Self = Self(vk::AccessFlags2KHR::INPUT_ATTACHMENT_READ_KHR);

    /// Shader read.
    pub const SHADER_READ: Self = Self(vk::AccessFlags2KHR::SHADER_READ_KHR);

    /// Shader write.
    pub const SHADER_WRITE: Self = Self(vk::AccessFlags2KHR::SHADER_WRITE_KHR);

    /// Color attachment read.
    pub const COLOR_ATTACHMENT_READ: Self = Self(vk::AccessFlags2KHR::COLOR_ATTACHMENT_READ_KHR);

    /// Color attachment write.
    pub const COLOR_ATTACHMENT_WRITE: Self = Self(vk::AccessFlags2KHR::COLOR_ATTACHMENT_WRITE_KHR);

    /// Depth-stencil attachment read.
    pub const DEPTH_STENCIL_ATTACHMENT_READ: Self =
        Self(vk::AccessFlags2KHR::DEPTH_STENCIL_ATTACHMENT_READ_KHR);

    /// Depth-stencil attachment write.
    pub const DEPTH_STENCIL_ATTACHMENT_WRITE: Self =
        Self(vk::AccessFlags2KHR::DEPTH_STENCIL_ATTACHMENT_WRITE_KHR);

    /// Transfer read.
    pub const TRANSFER_READ: Self = Self(vk::AccessFlags2KHR::TRANSFER_READ_KHR);

    /// Transfer write.
    pub const TRANSFER_WRITE: Self = Self(vk::AccessFlags2KHR::TRANSFER_WRITE_KHR);

    /// Host read.
    pub const HOST_READ: Self = Self(vk::AccessFlags2KHR::HOST_READ_KHR);

    /// Host write.
    pub const HOST_WRITE: Self = Self(vk::AccessFlags2KHR::HOST_WRITE_KHR);

    /// Memory read.
    pub const MEMORY_READ: Self = Self(vk::AccessFlags2KHR::MEMORY_READ_KHR);

    /// Memory write.
    pub const MEMORY_WRITE: Self = Self(vk::AccessFlags2KHR::MEMORY_WRITE_KHR);

    /// Shader sampled read.
    pub const SHADER_SAMPLED_READ: Self = Self(vk::AccessFlags2KHR::SHADER_SAMPLED_READ_KHR);

    /// Shader storage read.
    pub const SHADER_STORAGE_READ: Self = Self(vk::AccessFlags2KHR::SHADER_STORAGE_READ_KHR);

    /// Shader storage write.
    pub const SHADER_STORAGE_WRITE: Self = Self(vk::AccessFlags2KHR::SHADER_STORAGE_WRITE_KHR);

    /// Get the raw Vulkan flags.
    pub(crate) fn into_vk(self) -> vk::AccessFlags2KHR {
        self.0
    }

    /// Check if any flags are set.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Check if all specified flags are set.
    pub fn contains(&self, other: Self) -> bool {
        self.0.contains(other.0)
    }

    /// Bitwise OR.
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl Default for AccessFlags2 {
    fn default() -> Self {
        Self::NONE
    }
}

impl From<vk::AccessFlags2KHR> for AccessFlags2 {
    fn from(flags: vk::AccessFlags2KHR) -> Self {
        Self(flags)
    }
}

impl From<AccessFlags2> for vk::AccessFlags2KHR {
    fn from(wrapper: AccessFlags2) -> Self {
        wrapper.0
    }
}

/// Conversion from legacy `vk::AccessFlags` to modern `AccessFlags2`.
/// This enables gradual migration from Vulkan 1.0 to Vulkan 1.3 synchronization.
impl From<vk::AccessFlags> for AccessFlags2 {
    fn from(flags: vk::AccessFlags) -> Self {
        Self(vk::AccessFlags2KHR::from_raw(flags.as_raw() as u64))
    }
}

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

    /// Set old image layout.
    pub fn old_layout(mut self, layout: vk::ImageLayout) -> Self {
        self.old_layout = layout;
        self
    }

    /// Set new image layout.
    pub fn new_layout(mut self, layout: vk::ImageLayout) -> Self {
        self.new_layout = layout;
        self
    }

    /// Set image subresource range.
    pub fn subresource_range(mut self, range: vk::ImageSubresourceRange) -> Self {
        self.subresource_range = range;
        self
    }

    /// Convert to Vulkan vk::ImageMemoryBarrier2KHR.
    pub(crate) fn into_vk(self) -> vk::ImageMemoryBarrier2KHR<'static> {
        vk::ImageMemoryBarrier2KHR::default()
            .src_stage_mask(self.src_stage_mask.into_vk())
            .dst_stage_mask(self.dst_stage_mask.into_vk())
            .src_access_mask(self.src_access_mask.into_vk())
            .dst_access_mask(self.dst_access_mask.into_vk())
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
    /// Create a new buffer memory barrier 2.
    pub(crate) fn new(buffer: VkBuffer) -> Self {
        Self {
            src_stage_mask: PipelineStage2Flags::TOP_OF_PIPE,
            dst_stage_mask: PipelineStage2Flags::BOTTOM_OF_PIPE,
            src_access_mask: AccessFlags2::NONE,
            dst_access_mask: AccessFlags2::NONE,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer,
            offset: 0,
            size: vk::WHOLE_SIZE,
        }
    }

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

    /// Set buffer offset.
    pub fn offset(mut self, offset: vk::DeviceSize) -> Self {
        self.offset = offset;
        self
    }

    /// Set buffer size.
    pub fn size(mut self, size: vk::DeviceSize) -> Self {
        self.size = size;
        self
    }

    /// Convert to Vulkan vk::BufferMemoryBarrier2KHR.
    pub(crate) fn into_vk(self) -> vk::BufferMemoryBarrier2KHR<'static> {
        vk::BufferMemoryBarrier2KHR::default()
            .src_stage_mask(self.src_stage_mask.into_vk())
            .dst_stage_mask(self.dst_stage_mask.into_vk())
            .src_access_mask(self.src_access_mask.into_vk())
            .dst_access_mask(self.dst_access_mask.into_vk())
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

    /// Add a memory barrier.
    pub fn add_memory_barrier(mut self, barrier: vk::MemoryBarrier2KHR<'static>) -> Self {
        self.memory_barriers.push(barrier);
        self
    }

    /// Add a buffer memory barrier (raw Vulkan type).
    pub fn add_buffer_barrier(mut self, barrier: vk::BufferMemoryBarrier2KHR<'static>) -> Self {
        self.buffer_barriers.push(barrier);
        self
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

impl VkCommandBuffer {
    pub fn new(command_buffer: vk::CommandBuffer) -> Self {
        Self(command_buffer)
    }
}

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
    fn test_pipeline_stage2_flags() {
        let stage = PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT;
        assert!(!stage.is_empty());

        let empty = PipelineStage2Flags::NONE;
        assert!(empty.is_empty());
    }

    #[test]
    fn test_pipeline_stage2_flags_union() {
        let stage1 = PipelineStage2Flags::VERTEX_SHADER;
        let stage2 = PipelineStage2Flags::FRAGMENT_SHADER;
        let combined = stage1.union(stage2);

        assert!(combined.contains(stage1));
        assert!(combined.contains(stage2));
    }

    #[test]
    fn test_pipeline_stage2_conversions() {
        let vk_flags = vk::PipelineStageFlags2KHR::COLOR_ATTACHMENT_OUTPUT_KHR;
        let wrapper: PipelineStage2Flags = vk_flags.into();
        let back: vk::PipelineStageFlags2KHR = wrapper.into();
        assert_eq!(vk_flags, back);
    }

    #[test]
    fn test_access_flags2() {
        let access = AccessFlags2::COLOR_ATTACHMENT_WRITE;
        assert!(!access.is_empty());

        let empty = AccessFlags2::NONE;
        assert!(empty.is_empty());
    }

    #[test]
    fn test_access_flags2_union() {
        let access1 = AccessFlags2::SHADER_READ;
        let access2 = AccessFlags2::SHADER_WRITE;
        let combined = access1.union(access2);

        assert!(combined.contains(access1));
        assert!(combined.contains(access2));
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
    fn test_buffer_memory_barrier2_builder() {
        let buffer = VkBuffer::new(vk::Buffer::null());
        let barrier = BufferMemoryBarrier2::new(buffer)
            .src_stage(PipelineStage2Flags::COMPUTE_SHADER)
            .dst_stage(PipelineStage2Flags::VERTEX_SHADER)
            .src_access(AccessFlags2::SHADER_WRITE)
            .dst_access(AccessFlags2::VERTEX_ATTRIBUTE_READ)
            .offset(0)
            .size(1024);

        assert_eq!(barrier.buffer, buffer);
        assert_eq!(barrier.offset, 0);
        assert_eq!(barrier.size, 1024);
    }

    #[test]
    fn test_buffer_memory_barrier2_into_vk() {
        let buffer = VkBuffer::new(vk::Buffer::null());
        let barrier = BufferMemoryBarrier2::new(buffer)
            .src_stage(PipelineStage2Flags::COMPUTE_SHADER)
            .dst_stage(PipelineStage2Flags::VERTEX_INPUT);

        let vk_barrier = barrier.into_vk();
        assert_eq!(vk_barrier.buffer, buffer.vk());
    }

    #[test]
    fn test_dependency_info_with_buffer_barrier2() {
        let buffer = VkBuffer::new(vk::Buffer::null());
        let barrier = BufferMemoryBarrier2::new(buffer)
            .src_stage(PipelineStage2Flags::COMPUTE_SHADER)
            .dst_stage(PipelineStage2Flags::VERTEX_SHADER);

        let dep_info = DependencyInfo::new().add_buffer_barrier2(barrier);

        assert_eq!(dep_info.buffer_barriers.len(), 1);
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

    // Barrier helper function tests

    #[test]
    fn test_color_subresource_range() {
        assert_eq!(
            COLOR_SUBRESOURCE_RANGE.aspect_mask,
            vk::ImageAspectFlags::COLOR
        );
        assert_eq!(COLOR_SUBRESOURCE_RANGE.base_mip_level, 0);
        assert_eq!(COLOR_SUBRESOURCE_RANGE.level_count, 1);
        assert_eq!(COLOR_SUBRESOURCE_RANGE.base_array_layer, 0);
        assert_eq!(COLOR_SUBRESOURCE_RANGE.layer_count, 1);
    }

    #[test]
    fn test_depth_subresource_range() {
        assert_eq!(
            DEPTH_SUBRESOURCE_RANGE.aspect_mask,
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        );
        assert_eq!(DEPTH_SUBRESOURCE_RANGE.base_mip_level, 0);
        assert_eq!(DEPTH_SUBRESOURCE_RANGE.level_count, 1);
        assert_eq!(DEPTH_SUBRESOURCE_RANGE.base_array_layer, 0);
        assert_eq!(DEPTH_SUBRESOURCE_RANGE.layer_count, 1);
    }

    #[test]
    fn test_color_read_to_attachment_barrier() {
        let image = VkImage::new(vk::Image::null());
        let barrier = color_read_to_attachment_barrier(image);

        assert_eq!(barrier.image, image);
        assert_eq!(
            barrier.old_layout,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        );
        assert_eq!(
            barrier.new_layout,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        );
        assert_eq!(
            barrier.subresource_range.aspect_mask,
            COLOR_SUBRESOURCE_RANGE.aspect_mask
        );
    }

    #[test]
    fn test_color_attachment_to_read_barrier() {
        let image = VkImage::new(vk::Image::null());
        let barrier = color_attachment_to_read_barrier(image);

        assert_eq!(barrier.image, image);
        assert_eq!(
            barrier.old_layout,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        );
        assert_eq!(
            barrier.new_layout,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        );
        assert_eq!(
            barrier.subresource_range.aspect_mask,
            COLOR_SUBRESOURCE_RANGE.aspect_mask
        );
    }

    #[test]
    fn test_depth_attachment_barrier() {
        let image = VkImage::new(vk::Image::null());
        let barrier = depth_attachment_barrier(image);

        assert_eq!(barrier.image, image);
        assert_eq!(
            barrier.old_layout,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        );
        assert_eq!(
            barrier.new_layout,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        );
        assert_eq!(
            barrier.subresource_range.aspect_mask,
            DEPTH_SUBRESOURCE_RANGE.aspect_mask
        );
    }

    #[test]
    fn test_color_init_barrier() {
        let image = VkImage::new(vk::Image::null());
        let barrier = color_init_barrier(image);

        assert_eq!(barrier.image, image);
        assert_eq!(barrier.old_layout, vk::ImageLayout::UNDEFINED);
        assert_eq!(
            barrier.new_layout,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        );
        assert_eq!(
            barrier.subresource_range.aspect_mask,
            COLOR_SUBRESOURCE_RANGE.aspect_mask
        );
    }

    #[test]
    fn test_depth_init_barrier() {
        let image = VkImage::new(vk::Image::null());
        let barrier = depth_init_barrier(image);

        assert_eq!(barrier.image, image);
        assert_eq!(barrier.old_layout, vk::ImageLayout::UNDEFINED);
        assert_eq!(
            barrier.new_layout,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        );
        assert_eq!(
            barrier.subresource_range.aspect_mask,
            DEPTH_SUBRESOURCE_RANGE.aspect_mask
        );
    }

    #[test]
    fn test_transfer_dst_to_read_barrier() {
        let image = VkImage::new(vk::Image::null());
        let barrier = transfer_dst_to_read_barrier(image);

        assert_eq!(barrier.image, image);
        assert_eq!(barrier.old_layout, vk::ImageLayout::TRANSFER_DST_OPTIMAL);
        assert_eq!(
            barrier.new_layout,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        );
        assert_eq!(
            barrier.subresource_range.aspect_mask,
            COLOR_SUBRESOURCE_RANGE.aspect_mask
        );
    }
}
