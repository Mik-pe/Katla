//! Image barrier helpers for Vulkan 1.3 synchronization.
//!
//! This module provides high-level helpers for image layout transitions
//! with explicit source and destination layouts.
//!
//! # Core Philosophy
//!
//! The source layout determines whether contents are preserved:
//! - `UNDEFINED` source = discard contents, don't care what was there
//! - Specific layout source = preserve contents and synchronize properly
//!
//! # Basic Usage
//!
//! ```ignore
//! use katla_gfx::barrier::ImageBarrier;
//! use ash::vk;
//!
//! // Fresh images (discard contents)
//! ImageBarrier::transition_from_undefined(cmd, device, image,
//!     vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
//!
//! // Preserving contents (transitioning between used states)
//! ImageBarrier::transition(cmd, device, image,
//!     vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,   // from
//!     vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);  // to
//! ```
//!
//! # Common Workflows
//!
//! ## Swapchain Rendering
//! ```ignore
//! // Fresh swapchain image (discard previous frame's contents)
//! ImageBarrier::transition_from_undefined(cmd, device, swapchain_image,
//!     vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
//!
//! // After rendering, prepare for presentation
//! ImageBarrier::transition(cmd, device, swapchain_image,
//!     vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
//!     vk::ImageLayout::PRESENT_SRC_KHR);
//! ```
//!
//! ## Texture Upload
//! ```ignore
//! // Prepare for buffer copy
//! ImageBarrier::transition_from_undefined(cmd, device, texture_image,
//!     vk::ImageLayout::TRANSFER_DST_OPTIMAL);
//!
//! // After upload, prepare for sampling
//! ImageBarrier::transition(cmd, device, texture_image,
//!     vk::ImageLayout::TRANSFER_DST_OPTIMAL,
//!     vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
//! ```
//!
//! ## Texture Update (preserving existing contents)
//! ```ignore
//! // Transition away from shader read
//! ImageBarrier::transition(cmd, device, texture_image,
//!     vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
//!     vk::ImageLayout::TRANSFER_DST_OPTIMAL);
//!
//! // Upload new data...
//!
//! // Restore to shader read
//! ImageBarrier::transition(cmd, device, texture_image,
//!     vk::ImageLayout::TRANSFER_DST_OPTIMAL,
//!     vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
//! ```
//!
//! # Custom Subresource Ranges
//!
//! For custom ranges (mip levels, array layers):
//! ```ignore
//! use katla_gfx::sync::DEPTH_SUBRESOURCE_RANGE;
//!
//! ImageBarrier::transition_with_range(cmd, device, image,
//!     vk::ImageLayout::UNDEFINED,
//!     vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
//!     DEPTH_SUBRESOURCE_RANGE);
//! ```

use crate::sync::{
    AccessFlags2, DependencyInfo, ImageMemoryBarrier2, PipelineStage2Flags, VkImage,
};
use ash::vk;

/// Default subresource range for single-mip, single-layer images.
///
/// This is the most common case for textures and render targets.
const DEFAULT_SUBRESOURCE_RANGE: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: vk::REMAINING_MIP_LEVELS,
    base_array_layer: 0,
    layer_count: vk::REMAINING_ARRAY_LAYERS,
};

/// High-level image barrier helpers.
///
/// Provides explicit layout transitions with automatic stage/access mask deduction.
pub struct ImageBarrier;

impl ImageBarrier {
    //=========================================================================
    // Core API: Explicit Layout Transitions
    //=========================================================================

    /// Transition with explicit source and destination layouts.
    ///
    /// Uses default subresource range (all mip levels and array layers).
    /// For custom ranges, use [`Self::transition_with_range`].
    ///
    /// The source layout determines whether contents are preserved:
    /// - `UNDEFINED` = discard contents
    /// - Specific layout = preserve contents
    ///
    /// # Arguments
    ///
    /// * `old_layout` - Source layout (determines content preservation)
    /// * `new_layout` - Destination layout
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
    /// | PRESENT_SRC | COLOR_ATTACHMENT | BOTTOM_OF_PIPE | COLOR_ATTACHMENT_OUTPUT | NONE | COLOR_ATTACHMENT_WRITE |
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

    /// Transition with custom subresource range.
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
    // Convenience: Transition from UNDEFINED (99% case)
    //=========================================================================

    /// Transition from UNDEFINED layout (discard contents).
    ///
    /// This is the most common pattern for fresh images where you don't care
    /// about preserving previous contents.
    ///
    /// Uses default subresource range (all mip levels and array layers).
    /// For custom ranges, use [`Self::transition_from_undefined_with_range`].
    ///
    /// Equivalent to calling:
    /// ```ignore
    /// ImageBarrier::transition(cmd, device, image,
    ///     vk::ImageLayout::UNDEFINED,
    ///     new_layout);
    /// ```
    pub fn transition_from_undefined(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        new_layout: vk::ImageLayout,
    ) {
        Self::transition(
            cmd_buffer,
            device,
            image,
            vk::ImageLayout::UNDEFINED,
            new_layout,
        );
    }

    /// Transition from UNDEFINED with custom subresource range.
    pub fn transition_from_undefined_with_range(
        cmd_buffer: &vk::CommandBuffer,
        device: &ash::Device,
        image: vk::Image,
        new_layout: vk::ImageLayout,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        Self::transition_with_range(
            cmd_buffer,
            device,
            image,
            vk::ImageLayout::UNDEFINED,
            new_layout,
            subresource_range,
        );
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
    ) -> (
        PipelineStage2Flags,
        PipelineStage2Flags,
        AccessFlags2,
        AccessFlags2,
    ) {
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

        // COLOR_ATTACHMENT_OPTIMAL -> SHADER_READ_ONLY_OPTIMAL (render-to-texture)
        if old_layout == vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        {
            return (
                PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT,
                PipelineStage2Flags::FRAGMENT_SHADER,
                AccessFlags2::COLOR_ATTACHMENT_WRITE | AccessFlags2::COLOR_ATTACHMENT_READ,
                AccessFlags2::SHADER_READ,
            );
        }

        // SHADER_READ_ONLY_OPTIMAL -> COLOR_ATTACHMENT_OPTIMAL (texture reuse)
        // Used when a texture that was sampled needs to become a render target again
        if old_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            && new_layout == vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        {
            return (
                PipelineStage2Flags::FRAGMENT_SHADER,
                PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT,
                AccessFlags2::SHADER_READ,
                AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
        }

        // PRESENT_SRC_KHR -> COLOR_ATTACHMENT_OPTIMAL
        // Used when transitioning swapchain image from presentation back to rendering.
        // After presentation completes, the image is in PRESENT_SRC_KHR and needs to
        // transition back to COLOR_ATTACHMENT for the next frame's rendering.
        if old_layout == vk::ImageLayout::PRESENT_SRC_KHR
            && new_layout == vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        {
            return (
                PipelineStage2Flags::BOTTOM_OF_PIPE,
                PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT,
                AccessFlags2::NONE,
                AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
        }

        panic!(
            "Unsupported layout transition: {:?} -> {:?}",
            old_layout, new_layout
        );
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
    fn test_deduce_present_to_color_attachment() {
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );

        assert_eq!(src_stage, PipelineStage2Flags::BOTTOM_OF_PIPE);
        assert_eq!(dst_stage, PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT);
        assert_eq!(src_access, AccessFlags2::NONE);
        assert_eq!(dst_access, AccessFlags2::COLOR_ATTACHMENT_WRITE);
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
    fn test_deduce_shader_read_to_transfer_dst() {
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );

        assert_eq!(src_stage, PipelineStage2Flags::FRAGMENT_SHADER);
        assert_eq!(dst_stage, PipelineStage2Flags::TRANSFER);
        assert_eq!(src_access, AccessFlags2::SHADER_READ);
        assert_eq!(dst_access, AccessFlags2::TRANSFER_WRITE);
    }

    #[test]
    #[should_panic(expected = "Unsupported layout transition")]
    fn test_deduce_unsupported_transition() {
        ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
    }

    //=========================================================================
    // Real-World Workflow Tests
    //=========================================================================

    /// Test the complete texture upload workflow.
    ///
    /// Pattern for loading textures from disk:
    /// 1. Create image in UNDEFINED layout
    /// 2. Transition to TRANSFER_DST for buffer copy
    /// 3. Copy buffer data to image
    /// 4. Transition to SHADER_READ_ONLY for sampling
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
    /// Pattern used every frame when rendering to swapchain:
    /// 1. Acquire swapchain image
    /// 2. Transition to COLOR_ATTACHMENT_OPTIMAL for rendering
    /// 3. Render frame
    /// 4. Transition to PRESENT_SRC_KHR for presentation
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
    /// When creating offscreen render targets:
    /// 1. Create color image -> SHADER_READ_ONLY (for sampling)
    /// 2. Create depth image -> DEPTH_STENCIL_ATTACHMENT (for depth testing)
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

    /// Test the render-to-texture workflow (HDR rendering).
    ///
    /// Pattern for HDR rendering with tonemapping:
    /// 1. Render to HDR color attachment (COLOR_ATTACHMENT_OPTIMAL)
    /// 2. Transition to SHADER_READ_ONLY for tonemap pass sampling
    /// 3. Tonemap pass samples HDR and outputs to swapchain
    #[test]
    fn test_workflow_render_to_texture() {
        // COLOR_ATTACHMENT -> SHADER_READ_ONLY (for post-processing sampling)
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT);
        assert_eq!(dst_stage, PipelineStage2Flags::FRAGMENT_SHADER);
        assert_eq!(
            src_access,
            AccessFlags2::COLOR_ATTACHMENT_WRITE | AccessFlags2::COLOR_ATTACHMENT_READ
        );
        assert_eq!(dst_access, AccessFlags2::SHADER_READ);
    }

    /// Test the transient texture reuse workflow.
    ///
    /// Pattern for reusing transient textures across frames/passes:
    /// 1. Render to color attachment (COLOR_ATTACHMENT_OPTIMAL)
    /// 2. Transition to SHADER_READ_ONLY for sampling (e.g., UI pass)
    /// 3. Transition back to COLOR_ATTACHMENT for reuse (next frame/pass)
    #[test]
    fn test_workflow_transient_texture_reuse() {
        // Step 1: COLOR_ATTACHMENT -> SHADER_READ_ONLY (for sampling)
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT);
        assert_eq!(dst_stage, PipelineStage2Flags::FRAGMENT_SHADER);
        assert_eq!(
            src_access,
            AccessFlags2::COLOR_ATTACHMENT_WRITE | AccessFlags2::COLOR_ATTACHMENT_READ
        );
        assert_eq!(dst_access, AccessFlags2::SHADER_READ);

        // Step 2: SHADER_READ_ONLY -> COLOR_ATTACHMENT (for reuse)
        let (src_stage, dst_stage, src_access, dst_access) = ImageBarrier::deduce_transition_masks(
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        assert_eq!(src_stage, PipelineStage2Flags::FRAGMENT_SHADER);
        assert_eq!(dst_stage, PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT);
        assert_eq!(src_access, AccessFlags2::SHADER_READ);
        assert_eq!(dst_access, AccessFlags2::COLOR_ATTACHMENT_WRITE);
    }
}
