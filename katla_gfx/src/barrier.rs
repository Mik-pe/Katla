//! Image barrier helpers for Vulkan 1.3 synchronization.
//!
//! This module provides high-level helpers for common image layout transitions,
//! eliminating the need to manually construct pipeline stages and access masks.
//!
//! # Common Transitions
//!
//! Convenience functions for the most common transitions:
//!
//! ```ignore
//! use katla_gfx::barrier::ImageBarrier;
//! use ash::vk;
//!
//! // Single function call replaces 20+ lines of boilerplate
//! ImageBarrier::to_color_attachment(cmd, image);
//! ImageBarrier::to_shader_read(cmd, image);
//! ImageBarrier::to_present_src(cmd, image);
//! ```
//!
//! # Default Subresource Range
//!
//! Functions default to a single mip level and array layer. For custom ranges:
//!
//! ```ignore
//! ImageBarrier::to_color_attachment_with_range(cmd, image, custom_range);
//! ```
//!
//! # Automatic Deduction
//!
//! The transition function automatically deduces correct stages and access masks:
//!
//! ```ignore
//! ImageBarrier::transition(cmd, image,
//!     vk::ImageLayout::TRANSFER_DST_OPTIMAL,
//!     vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
//! );
//! ```

use crate::sync::{
    AccessFlags2, DEPTH_SUBRESOURCE_RANGE,
    DependencyInfo, ImageMemoryBarrier2, PipelineStage2Flags, VkImage,
};
use ash::vk;

/// Default subresource range for single-mip, single-layer images.
///
/// This is the most common case for textures and render targets.
const DEFAULT_SUBRESOURCE_RANGE: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: vk::REMAINING_MIP_LEVELS, // All remaining mip levels
    base_array_layer: 0,
    layer_count: vk::REMAINING_ARRAY_LAYERS, // All remaining layers
};

/// High-level image barrier helpers.
///
/// Provides convenience functions for common image layout transitions
/// and a builder pattern for custom transitions.
pub struct ImageBarrier;

impl ImageBarrier {
    //=========================================================================
    // Common Transitions - Convenience Functions (with defaults!)
    //=========================================================================

    /// Transition image to color attachment layout for rendering.
    ///
    /// Uses default subresource range (all mip levels and array layers).
    /// For custom ranges, use [`Self::to_color_attachment_with_range`].
    ///
    /// Transitions from UNDEFINED to COLOR_ATTACHMENT_OPTIMAL.
    pub fn to_color_attachment(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
    ) {
        Self::to_color_attachment_with_range(cmd_buffer, device, image, DEFAULT_SUBRESOURCE_RANGE);
    }

    /// Transition image to color attachment layout with custom subresource range.
    ///
    /// Transitions from UNDEFINED to COLOR_ATTACHMENT_OPTIMAL.
    pub fn to_color_attachment_with_range(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        Self::transition_with_range(
            cmd_buffer,
            device,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            subresource_range,
        );
    }

    /// Transition color attachment output to present source.
    ///
    /// Uses default subresource range (all mip levels and array layers).
    /// For custom ranges, use [`Self::to_present_src_with_range`].
    ///
    /// Transitions from COLOR_ATTACHMENT_OPTIMAL to PRESENT_SRC_KHR.
    pub fn to_present_src(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
    ) {
        Self::to_present_src_with_range(cmd_buffer, device, image, DEFAULT_SUBRESOURCE_RANGE);
    }

    /// Transition color attachment output to present source with custom subresource range.
    ///
    /// Transitions from COLOR_ATTACHMENT_OPTIMAL to PRESENT_SRC_KHR.
    pub fn to_present_src_with_range(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        Self::transition_with_range(
            cmd_buffer,
            device,
            image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            subresource_range,
        );
    }

    /// Transition image to shader read-only layout.
    ///
    /// Uses default subresource range (all mip levels and array layers).
    /// For custom ranges, use [`Self::to_shader_read_with_range`].
    ///
    /// Transfers from UNDEFINED to SHADER_READ_ONLY_OPTIMAL.
    pub fn to_shader_read(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
    ) {
        Self::to_shader_read_with_range(cmd_buffer, device, image, DEFAULT_SUBRESOURCE_RANGE);
    }

    /// Transition image to shader read-only layout with custom subresource range.
    ///
    /// Transfers from UNDEFINED to SHADER_READ_ONLY_OPTIMAL.
    pub fn to_shader_read_with_range(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        Self::transition_with_range(
            cmd_buffer,
            device,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            subresource_range,
        );
    }

    /// Transition image to depth stencil attachment layout.
    ///
    /// Uses default subresource range (all mip levels and array layers).
    /// For custom ranges, use [`Self::to_depth_attachment_with_range`].
    ///
    /// Transfers from UNDEFINED to DEPTH_STENCIL_ATTACHMENT_OPTIMAL.
    pub fn to_depth_attachment(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
    ) {
        Self::to_depth_attachment_with_range(cmd_buffer, device, image, DEPTH_SUBRESOURCE_RANGE);
    }

    /// Transition image to depth stencil attachment layout with custom subresource range.
    ///
    /// Transfers from UNDEFINED to DEPTH_STENCIL_ATTACHMENT_OPTIMAL.
    pub fn to_depth_attachment_with_range(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        Self::transition_with_range(
            cmd_buffer,
            device,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            subresource_range,
        );
    }

    /// Transition from UNDEFINED to TRANSFER_DST_OPTIMAL.
    ///
    /// Uses default subresource range (all mip levels and array layers).
    /// For custom ranges, use [`Self::undefined_to_transfer_dst_with_range`].
    ///
    /// Used before copying buffer data to image.
    pub fn undefined_to_transfer_dst(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
    ) {
        Self::undefined_to_transfer_dst_with_range(cmd_buffer, device, image, DEFAULT_SUBRESOURCE_RANGE);
    }

    /// Transition from UNDEFINED to TRANSFER_DST_OPTIMAL with custom subresource range.
    ///
    /// Used before copying buffer data to image.
    pub fn undefined_to_transfer_dst_with_range(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        Self::transition_with_range(
            cmd_buffer,
            device,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            subresource_range,
        );
    }

    /// Transition from TRANSFER_DST_OPTIMAL to SHADER_READ_ONLY_OPTIMAL.
    ///
    /// Uses default subresource range (all mip levels and array layers).
    /// For custom ranges, use [`Self::transfer_dst_to_shader_read_with_range`].
    ///
    /// Used after uploading texture data via buffer copy.
    pub fn transfer_dst_to_shader_read(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
    ) {
        Self::transfer_dst_to_shader_read_with_range(cmd_buffer, device, image, DEFAULT_SUBRESOURCE_RANGE);
    }

    /// Transition from TRANSFER_DST_OPTIMAL to SHADER_READ_ONLY_OPTIMAL with custom subresource range.
    ///
    /// Used after uploading texture data via buffer copy.
    pub fn transfer_dst_to_shader_read_with_range(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        Self::transition_with_range(
            cmd_buffer,
            device,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            subresource_range,
        );
    }

    /// Transition from SHADER_READ_ONLY_OPTIMAL to TRANSFER_DST_OPTIMAL.
    ///
    /// Uses default subresource range (all mip levels and array layers).
    /// For custom ranges, use [`Self::shader_read_to_transfer_dst_with_range`].
    ///
    /// Used when updating existing texture data.
    pub fn shader_read_to_transfer_dst(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
    ) {
        Self::shader_read_to_transfer_dst_with_range(cmd_buffer, device, image, DEFAULT_SUBRESOURCE_RANGE);
    }

    /// Transition from SHADER_READ_ONLY_OPTIMAL to TRANSFER_DST_OPTIMAL with custom subresource range.
    ///
    /// Used when updating existing texture data.
    pub fn shader_read_to_transfer_dst_with_range(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        Self::transition_with_range(
            cmd_buffer,
            device,
            image,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            subresource_range,
        );
    }

    //=========================================================================
    // Automatic Deduction - Smart Transition (with defaults!)
    //=========================================================================

    /// Transition with automatic stage/access mask deduction.
    ///
    /// Uses default subresource range (all mip levels and array layers).
    /// For custom ranges, use [`Self::transition_with_range`].
    ///
    /// Deduces the correct pipeline stages and access masks based on
    /// the source and destination layouts.
    ///
    /// # Supported Transitions
    ///
    /// | From | To | Source Stage | Dest Stage | Source Access | Dest Access |
    /// |------|-----|--------------|------------|---------------|-------------|
    /// | UNDEFINED | TRANSFER_DST | TOP_OF_PIPE | TRANSFER | NONE | TRANSFER_WRITE |
    /// | TRANSFER_DST | SHADER_READ_ONLY | TRANSFER | FRAGMENT_SHADER | TRANSFER_WRITE | SHADER_READ |
    /// | SHADER_READ_ONLY | TRANSFER_DST | FRAGMENT_SHADER | TRANSFER | SHADER_READ | TRANSFER_WRITE |
    /// | UNDEFINED | COLOR_ATTACHMENT | TOP_OF_PIPE | COLOR_ATTACHMENT_OUTPUT | NONE | COLOR_ATTACHMENT_WRITE |
    /// | COLOR_ATTACHMENT | PRESENT_SRC | COLOR_ATTACHMENT_OUTPUT | BOTTOM_OF_PIPE | COLOR_ATTACHMENT_WRITE | NONE |
    /// | UNDEFINED | DEPTH_STENCIL_ATTACHMENT | TOP_OF_PIPE | EARLY_FRAGMENT_TESTS | NONE | DEPTH_STENCIL_* |
    /// | UNDEFINED | SHADER_READ_ONLY | TOP_OF_PIPE | FRAGMENT_SHADER | NONE | SHADER_READ |
    ///
    /// # Panics
    ///
    /// Panics if the transition is not in the supported table.
    pub fn transition(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) {
        Self::transition_with_range(
            cmd_buffer,
            device,
            image,
            old_layout,
            new_layout,
            DEFAULT_SUBRESOURCE_RANGE,
        );
    }

    /// Transition with automatic deduction and custom subresource range.
    ///
    /// See [`Self::transition`] for the list of supported transitions.
    pub fn transition_with_range(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        let (src_stage, dst_stage, src_access, dst_access) =
            Self::deduce_transition_masks(old_layout, new_layout);

        let barrier = ImageMemoryBarrier2::new(VkImage::new(image))
            .src_stage(src_stage)
            .dst_stage(dst_stage)
            .src_access(src_access)
            .dst_access(dst_access)
            .old_layout(old_layout)
            .new_layout(new_layout)
            .subresource_range(subresource_range);

        let dep_info = DependencyInfo::new().add_image_barrier(barrier);
        dep_info.build(|dep_info| unsafe {
            device.cmd_pipeline_barrier2(*cmd_buffer, dep_info);
        });
    }

    //=========================================================================
    // Builder Pattern - Custom Transitions
    //=========================================================================

    /// Create a builder for custom image barriers.
    ///
    /// Allows full control over all barrier parameters.
    /// For automatic deduction, use [`Self::transition()`].
    ///
    /// # Example
    ///
    /// ```ignore
    /// use katla_gfx::barrier::ImageBarrier;
    /// use ash::vk;
    ///
    /// ImageBarrier::builder()
    ///     .from_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
    ///     .to_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
    ///     .from_stage(vk::PipelineStageFlags2::COMPUTE_SHADER)
    ///     .to_stage(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
    ///     .from_access(vk::AccessFlags2::SHADER_WRITE)
    ///     .to_access(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
    ///     .apply(cmd_buffer, &device, image, subresource_range);
    /// ```
    pub fn builder() -> ImageBarrierBuilder {
        ImageBarrierBuilder::new()
    }

    //=========================================================================
    // Internal Helpers
    //=========================================================================

    /// Deduce pipeline stages and access masks for a layout transition.
    ///
    /// Returns (src_stage, dst_stage, src_access, dst_access).
    fn deduce_transition_masks(
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) -> (PipelineStage2Flags, PipelineStage2Flags, AccessFlags2, AccessFlags2) {
        // UNDEFINED -> TRANSFER_DST_OPTIMAL
        if old_layout == vk::ImageLayout::UNDEFINED
            && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
        {
            return (
                PipelineStage2Flags::TOP_OF_PIPE,
                PipelineStage2Flags::TRANSFER,
                AccessFlags2::NONE,
                AccessFlags2::TRANSFER_WRITE,
            );
        }

        // TRANSFER_DST_OPTIMAL -> SHADER_READ_ONLY_OPTIMAL
        if old_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
            && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        {
            return (
                PipelineStage2Flags::TRANSFER,
                PipelineStage2Flags::FRAGMENT_SHADER,
                AccessFlags2::TRANSFER_WRITE,
                AccessFlags2::SHADER_READ,
            );
        }

        // SHADER_READ_ONLY_OPTIMAL -> TRANSFER_DST_OPTIMAL
        if old_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
        {
            return (
                PipelineStage2Flags::FRAGMENT_SHADER,
                PipelineStage2Flags::TRANSFER,
                AccessFlags2::SHADER_READ,
                AccessFlags2::TRANSFER_WRITE,
            );
        }

        // UNDEFINED -> COLOR_ATTACHMENT_OPTIMAL
        if old_layout == vk::ImageLayout::UNDEFINED
            && new_layout == vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        {
            return (
                PipelineStage2Flags::TOP_OF_PIPE,
                PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT,
                AccessFlags2::NONE,
                AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
        }

        // COLOR_ATTACHMENT_OPTIMAL -> PRESENT_SRC_KHR
        if old_layout == vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            && new_layout == vk::ImageLayout::PRESENT_SRC_KHR
        {
            return (
                PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT,
                PipelineStage2Flags::BOTTOM_OF_PIPE,
                AccessFlags2::COLOR_ATTACHMENT_WRITE,
                AccessFlags2::NONE,
            );
        }

        // UNDEFINED -> DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        if old_layout == vk::ImageLayout::UNDEFINED
            && new_layout == vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        {
            return (
                PipelineStage2Flags::TOP_OF_PIPE,
                PipelineStage2Flags::EARLY_FRAGMENT_TESTS,
                AccessFlags2::NONE,
                AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                    | AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
            );
        }

        // UNDEFINED -> SHADER_READ_ONLY_OPTIMAL
        if old_layout == vk::ImageLayout::UNDEFINED
            && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        {
            return (
                PipelineStage2Flags::TOP_OF_PIPE,
                PipelineStage2Flags::FRAGMENT_SHADER,
                AccessFlags2::NONE,
                AccessFlags2::SHADER_READ,
            );
        }

        panic!(
            "Unsupported layout transition: {:?} -> {:?}",
            old_layout, new_layout
        );
    }
}

/// Builder for custom image barriers.
///
/// Provides full control over all barrier parameters.
/// Use via [`ImageBarrier::builder()`].
#[must_use]
pub struct ImageBarrierBuilder {
    src_stage: Option<PipelineStage2Flags>,
    dst_stage: Option<PipelineStage2Flags>,
    src_access: Option<AccessFlags2>,
    dst_access: Option<AccessFlags2>,
    old_layout: Option<vk::ImageLayout>,
    new_layout: Option<vk::ImageLayout>,
    src_queue_family_index: Option<u32>,
    dst_queue_family_index: Option<u32>,
}

impl ImageBarrierBuilder {
    fn new() -> Self {
        Self {
            src_stage: None,
            dst_stage: None,
            src_access: None,
            dst_access: None,
            old_layout: None,
            new_layout: None,
            src_queue_family_index: None,
            dst_queue_family_index: None,
        }
    }

    /// Set the source pipeline stage.
    pub fn src_stage(mut self, stage: PipelineStage2Flags) -> Self {
        self.src_stage = Some(stage);
        self
    }

    /// Set the destination pipeline stage.
    pub fn dst_stage(mut self, stage: PipelineStage2Flags) -> Self {
        self.dst_stage = Some(stage);
        self
    }

    /// Set the source access mask.
    pub fn src_access(mut self, access: AccessFlags2) -> Self {
        self.src_access = Some(access);
        self
    }

    /// Set the destination access mask.
    pub fn dst_access(mut self, access: AccessFlags2) -> Self {
        self.dst_access = Some(access);
        self
    }

    /// Set the old (source) image layout.
    pub fn from_layout(mut self, layout: vk::ImageLayout) -> Self {
        self.old_layout = Some(layout);
        self
    }

    /// Set the new (destination) image layout.
    pub fn to_layout(mut self, layout: vk::ImageLayout) -> Self {
        self.new_layout = Some(layout);
        self
    }

    /// Set both source and destination queue family indices.
    ///
    /// Use `vk::QUEUE_FAMILY_IGNORED` for no ownership transfer.
    pub fn queue_family(mut self, src: u32, dst: u32) -> Self {
        self.src_queue_family_index = Some(src);
        self.dst_queue_family_index = Some(dst);
        self
    }

    /// Apply the barrier to a command buffer.
    ///
    /// # Panics
    ///
    /// Panics if required fields (layouts, stages) are not set.
    pub fn apply(
        self,
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        let old_layout = self
            .old_layout
            .expect("ImageBarrierBuilder: from_layout() must be set");
        let new_layout = self
            .new_layout
            .expect("ImageBarrierBuilder: to_layout() must be set");
        let src_stage = self.src_stage.unwrap_or(PipelineStage2Flags::TOP_OF_PIPE);
        let dst_stage = self.dst_stage.unwrap_or(PipelineStage2Flags::BOTTOM_OF_PIPE);
        let src_access = self.src_access.unwrap_or(AccessFlags2::NONE);
        let dst_access = self.dst_access.unwrap_or(AccessFlags2::NONE);
        let src_queue_family_index = self.src_queue_family_index.unwrap_or(vk::QUEUE_FAMILY_IGNORED);
        let dst_queue_family_index = self.dst_queue_family_index.unwrap_or(vk::QUEUE_FAMILY_IGNORED);

        let mut barrier = ImageMemoryBarrier2::new(VkImage::new(image))
            .src_stage(src_stage)
            .dst_stage(dst_stage)
            .src_access(src_access)
            .dst_access(dst_access)
            .old_layout(old_layout)
            .new_layout(new_layout)
            .subresource_range(subresource_range);

        // Manually set queue family indices (not exposed via builder methods)
        barrier.src_queue_family_index = src_queue_family_index;
        barrier.dst_queue_family_index = dst_queue_family_index;

        let dep_info = DependencyInfo::new().add_image_barrier(barrier);
        dep_info.build(|dep_info| unsafe {
            device.cmd_pipeline_barrier2(*cmd_buffer, dep_info);
        });
    }
}

impl Default for ImageBarrierBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduce_undefined_to_transfer_dst() {
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );

        assert_eq!(src_stage, PipelineStage2Flags::TOP_OF_PIPE);
        assert_eq!(dst_stage, PipelineStage2Flags::TRANSFER);
        assert_eq!(src_access, AccessFlags2::NONE);
        assert_eq!(dst_access, AccessFlags2::TRANSFER_WRITE);
    }

    #[test]
    fn test_deduce_transfer_dst_to_shader_read() {
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );

        assert_eq!(src_stage, PipelineStage2Flags::TRANSFER);
        assert_eq!(dst_stage, PipelineStage2Flags::FRAGMENT_SHADER);
        assert_eq!(src_access, AccessFlags2::TRANSFER_WRITE);
        assert_eq!(dst_access, AccessFlags2::SHADER_READ);
    }

    #[test]
    fn test_deduce_undefined_to_color_attachment() {
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );

        assert_eq!(src_stage, PipelineStage2Flags::TOP_OF_PIPE);
        assert_eq!(dst_stage, PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT);
        assert_eq!(src_access, AccessFlags2::NONE);
        assert_eq!(dst_access, AccessFlags2::COLOR_ATTACHMENT_WRITE);
    }

    #[test]
    fn test_deduce_color_attachment_to_present() {
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );

        assert_eq!(src_stage, PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT);
        assert_eq!(dst_stage, PipelineStage2Flags::BOTTOM_OF_PIPE);
        assert_eq!(src_access, AccessFlags2::COLOR_ATTACHMENT_WRITE);
        assert_eq!(dst_access, AccessFlags2::NONE);
    }

    #[test]
    fn test_deduce_undefined_to_depth_attachment() {
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        );

        assert_eq!(src_stage, PipelineStage2Flags::TOP_OF_PIPE);
        assert_eq!(dst_stage, PipelineStage2Flags::EARLY_FRAGMENT_TESTS);
        assert_eq!(src_access, AccessFlags2::NONE);
        assert_eq!(
            dst_access,
            AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                | AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
        );
    }

    #[test]
    #[should_panic(expected = "Unsupported layout transition")]
    fn test_deduce_unsupported_transition() {
        ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
    }

    #[test]
    fn test_builder_default() {
        let builder = ImageBarrierBuilder::default();
        assert!(builder.old_layout.is_none());
        assert!(builder.new_layout.is_none());
    }

    #[test]
    fn test_builder_chaining() {
        let builder = ImageBarrier::builder()
            .from_layout(vk::ImageLayout::UNDEFINED)
            .to_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_stage(PipelineStage2Flags::TOP_OF_PIPE)
            .dst_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
            .src_access(AccessFlags2::NONE)
            .dst_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .queue_family(vk::QUEUE_FAMILY_IGNORED, vk::QUEUE_FAMILY_IGNORED);

        assert_eq!(
            builder.old_layout,
            Some(vk::ImageLayout::UNDEFINED)
        );
        assert_eq!(
            builder.new_layout,
            Some(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        );
        assert_eq!(builder.src_stage, Some(PipelineStage2Flags::TOP_OF_PIPE));
        assert_eq!(
            builder.dst_stage,
            Some(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
        );
        assert_eq!(builder.src_access, Some(AccessFlags2::NONE));
        assert_eq!(
            builder.dst_access,
            Some(AccessFlags2::COLOR_ATTACHMENT_WRITE)
        );
    }

    //=========================================================================
    // Real-World Workflow Tests (based on Vulkan-Samples patterns)
    //=========================================================================

    /// Test the complete texture upload workflow.
    ///
    /// This is the most common pattern for loading textures from disk:
    /// 1. Create image in UNDEFINED layout
    /// 2. Transition to TRANSFER_DST for buffer copy
    /// 3. Copy buffer data to image
    /// 4. Transition to SHADER_READ_ONLY for sampling
    ///
    /// Pattern source: Vulkan-Samples texture loading, Vulkan Tutorial
    #[test]
    fn test_workflow_texture_upload() {
        // Step 1: UNDEFINED -> TRANSFER_DST (prepare for upload)
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::TOP_OF_PIPE);
        assert_eq!(dst_stage, PipelineStage2Flags::TRANSFER);
        assert_eq!(src_access, AccessFlags2::NONE);
        assert_eq!(dst_access, AccessFlags2::TRANSFER_WRITE);

        // Step 2: TRANSFER_DST -> SHADER_READ_ONLY (prepare for sampling)
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::TRANSFER);
        assert_eq!(dst_stage, PipelineStage2Flags::FRAGMENT_SHADER);
        assert_eq!(src_access, AccessFlags2::TRANSFER_WRITE);
        assert_eq!(dst_access, AccessFlags2::SHADER_READ);
    }

    /// Test the swapchain frame rendering workflow.
    ///
    /// This is the pattern used every frame when rendering to swapchain:
    /// 1. Acquire swapchain image (UNDEFINED or from previous frame)
    /// 2. Transition to COLOR_ATTACHMENT_OPTIMAL for rendering
    /// 3. Render frame
    /// 4. Transition to PRESENT_SRC_KHR for presentation
    ///
    /// Pattern source: Vulkan-Samples swapchain rendering
    #[test]
    fn test_workflow_swapchain_frame() {
        // Step 1: UNDEFINED -> COLOR_ATTACHMENT (prepare for rendering)
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::TOP_OF_PIPE);
        assert_eq!(dst_stage, PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT);
        assert_eq!(src_access, AccessFlags2::NONE);
        assert_eq!(dst_access, AccessFlags2::COLOR_ATTACHMENT_WRITE);

        // Step 2: COLOR_ATTACHMENT -> PRESENT_SRC (prepare for presentation)
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
        assert_eq!(src_stage, PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT);
        assert_eq!(dst_stage, PipelineStage2Flags::BOTTOM_OF_PIPE);
        assert_eq!(src_access, AccessFlags2::COLOR_ATTACHMENT_WRITE);
        assert_eq!(dst_access, AccessFlags2::NONE);
    }

    /// Test the dynamic texture update workflow.
    ///
    /// When updating an existing texture (e.g., font atlas, UI texture):
    /// 1. Transition from SHADER_READ_ONLY to TRANSFER_DST
    /// 2. Upload new data
    /// 3. Transition back to SHADER_READ_ONLY
    ///
    /// Pattern source: Vulkan-Samples dynamic texture updates
    #[test]
    fn test_workflow_texture_update() {
        // Step 1: SHADER_READ_ONLY -> TRANSFER_DST (prepare for upload)
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::FRAGMENT_SHADER);
        assert_eq!(dst_stage, PipelineStage2Flags::TRANSFER);
        assert_eq!(src_access, AccessFlags2::SHADER_READ);
        assert_eq!(dst_access, AccessFlags2::TRANSFER_WRITE);

        // Step 2: TRANSFER_DST -> SHADER_READ_ONLY (restore for sampling)
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::TRANSFER);
        assert_eq!(dst_stage, PipelineStage2Flags::FRAGMENT_SHADER);
        assert_eq!(src_access, AccessFlags2::TRANSFER_WRITE);
        assert_eq!(dst_access, AccessFlags2::SHADER_READ);
    }

    /// Test the render target creation workflow.
    ///
    /// When creating offscreen render targets (for deferred rendering,
    /// shadow maps, etc.):
    /// 1. Create color image -> SHADER_READ_ONLY (for sampling)
    /// 2. Create depth image -> DEPTH_STENCIL_ATTACHMENT (for depth testing)
    ///
    /// Pattern source: Vulkan-Samples viewport/render target creation
    #[test]
    fn test_workflow_render_target_creation() {
        // Color attachment: UNDEFINED -> SHADER_READ_ONLY
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::TOP_OF_PIPE);
        assert_eq!(dst_stage, PipelineStage2Flags::FRAGMENT_SHADER);
        assert_eq!(src_access, AccessFlags2::NONE);
        assert_eq!(dst_access, AccessFlags2::SHADER_READ);

        // Depth attachment: UNDEFINED -> DEPTH_STENCIL_ATTACHMENT
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::TOP_OF_PIPE);
        assert_eq!(dst_stage, PipelineStage2Flags::EARLY_FRAGMENT_TESTS);
        assert_eq!(src_access, AccessFlags2::NONE);
        assert_eq!(
            dst_access,
            AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                | AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
        );
    }

    /// Test that all shader-readable image types can transition from UNDEFINED.
    ///
    /// Common pattern for textures that will be sampled in shaders but
    /// aren't written to (loaded from disk, generated procedurally, etc.).
    #[test]
    fn test_direct_to_shader_read() {
        // UNDEFINED -> SHADER_READ_ONLY is valid for direct initialization
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::TOP_OF_PIPE);
        assert_eq!(dst_stage, PipelineStage2Flags::FRAGMENT_SHADER);
        assert_eq!(src_access, AccessFlags2::NONE);
        assert_eq!(dst_access, AccessFlags2::SHADER_READ);
    }

    /// Test that render pass output transitions are correctly configured.
    ///
    /// Verifies that the pipeline stages match what the hardware expects:
    /// - COLOR_ATTACHMENT_OUTPUT stage for color attachment writes
    /// - EARLY_FRAGMENT_TESTS stage for depth testing
    ///
    /// Pattern source: Vulkan spec synchronization requirements
    #[test]
    fn test_render_pass_pipeline_stages() {
        // Color attachment writes happen in COLOR_ATTACHMENT_OUTPUT stage
        let (_, dst_stage, _, _) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        assert_eq!(dst_stage, PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT);

        // Depth operations happen in EARLY_FRAGMENT_TESTS stage
        let (_, dst_stage, _, _) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        );
        assert_eq!(dst_stage, PipelineStage2Flags::EARLY_FRAGMENT_TESTS);
    }
}
