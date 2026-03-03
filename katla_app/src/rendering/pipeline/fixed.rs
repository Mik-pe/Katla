//! Fixed render pipeline implementation.
//!
//! This module provides a fixed render pipeline that executes a sequence of
//! render passes with managed attachments.

use ash::vk;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use katla_gfx::material::MaterialPipelineCache;
use katla_gfx::render_pass::{PassExecutor, RenderPass};
use katla_gfx::renderer::AssetRegistry;
use katla_gfx::texture::ImageFormat;
use katla_gfx::CommandBuffer;

use super::builder::FixedPipelineBuilder;

/// Describes the size of an attachment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttachmentSize {
    /// Size relative to the swapchain (1.0 = swapchain size).
    SwapchainRelative(f32),
    /// Fixed size in pixels.
    Fixed(u32, u32),
}

impl Default for AttachmentSize {
    fn default() -> Self {
        Self::SwapchainRelative(1.0)
    }
}

/// Configuration for a single attachment.
#[derive(Debug, Clone)]
pub struct AttachmentConfig {
    /// Image format.
    pub format: ImageFormat,
    /// Size specification.
    pub size: AttachmentSize,
    /// Vulkan usage flags.
    pub usage: vk::ImageUsageFlags,
}

impl AttachmentConfig {
    /// Create a new color attachment configuration.
    pub fn new(format: ImageFormat, size: AttachmentSize) -> Self {
        Self {
            format,
            size,
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC,
        }
    }

    /// Create a new depth attachment configuration.
    pub fn depth(format: ImageFormat, size: AttachmentSize) -> Self {
        Self {
            format,
            size,
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        }
    }
}

/// A fixed render pipeline that executes a sequence of render passes.
///
/// The pipeline manages attachment resources and executes passes in order.
/// Attachments can be shared between passes (e.g., output of one pass
/// becomes input to another).
pub struct FixedPipeline {
    passes: Vec<Box<dyn RenderPass>>,
    executor: PassExecutor,
    attachment_configs: HashMap<String, AttachmentConfig>,
    current_extent: vk::Extent2D,
}

impl FixedPipeline {
    /// Create a new pipeline from components.
    ///
    /// This is used internally by `FixedPipelineBuilder`.
    pub(crate) fn new(
        passes: Vec<Box<dyn RenderPass>>,
        executor: PassExecutor,
        attachment_configs: HashMap<String, AttachmentConfig>,
        current_extent: vk::Extent2D,
    ) -> Self {
        Self {
            passes,
            executor,
            attachment_configs,
            current_extent,
        }
    }

    /// Create a new pipeline builder.
    pub fn builder() -> FixedPipelineBuilder {
        FixedPipelineBuilder::new()
    }

    /// Render a frame using this pipeline.
    ///
    /// This method:
    /// 1. Checks if resize is needed
    /// 2. Executes all render passes in order
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record into
    /// * `extent` - Current swapchain extent
    /// * `frame_index` - Current frame index for resource selection
    /// * `draw_list` - Optional draw list containing geometry to render
    /// * `asset_registry` - Asset registry for accessing meshes and materials
    /// * `pipeline_cache` - Pipeline cache for resolving pipeline handles
    /// * `storage_descriptor_set` - Storage descriptor set (set 0) for frame/object uniforms
    /// * `bindless_descriptor_set` - Optional bindless texture descriptor set (set 1)
    pub fn render(
        &mut self,
        command_buffer: &CommandBuffer,
        extent: vk::Extent2D,
        frame_index: u32,
        draw_list: Option<&katla_gfx::renderer::DrawList>,
        asset_registry: &AssetRegistry,
        pipeline_cache: &Rc<RefCell<MaterialPipelineCache>>,
        storage_descriptor_set: vk::DescriptorSet,
        bindless_descriptor_set: Option<vk::DescriptorSet>,
    ) {
        // Check if resize is needed
        if extent.width != self.current_extent.width || extent.height != self.current_extent.height
        {
            self.resize(extent);
        }

        // Execute all passes
        self.executor.execute(
            command_buffer,
            &self.passes,
            self.current_extent,
            frame_index,
            draw_list,
            asset_registry,
            pipeline_cache,
            storage_descriptor_set,
            bindless_descriptor_set,
        );
    }

    /// Resize all attachments to match the new extent.
    ///
    /// This is called automatically by `render()` when the extent changes.
    pub fn resize(&mut self, extent: vk::Extent2D) {
        self.current_extent = extent;

        // Recreate attachments at new size
        // Note: This is a simplified implementation
        // A full implementation would:
        // 1. Wait for GPU to finish using old attachments
        // 2. Destroy old attachments
        // 3. Create new attachments at the new size

        log::debug!(
            "FixedPipeline resized to {}x{}",
            self.current_extent.width,
            self.current_extent.height
        );
    }

    /// Get the current extent.
    pub fn extent(&self) -> vk::Extent2D {
        self.current_extent
    }

    /// Get the number of passes in this pipeline.
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    /// Get a reference to the attachment resources.
    pub fn attachments(&self) -> &katla_gfx::render_pass::AttachmentResources {
        self.executor.attachments()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attachment_size_default() {
        let size = AttachmentSize::default();
        assert!(matches!(size, AttachmentSize::SwapchainRelative(1.0)));
    }

    #[test]
    fn test_attachment_config_new() {
        let config = AttachmentConfig::new(
            ImageFormat::R8G8B8A8Srgb,
            AttachmentSize::SwapchainRelative(1.0),
        );
        assert_eq!(config.format, ImageFormat::R8G8B8A8Srgb);
        assert!(config.usage.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT));
    }

    #[test]
    fn test_attachment_config_depth() {
        let config = AttachmentConfig::depth(
            ImageFormat::D32Sfloat,
            AttachmentSize::SwapchainRelative(1.0),
        );
        assert_eq!(config.format, ImageFormat::D32Sfloat);
        assert!(config
            .usage
            .contains(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT));
    }

    #[test]
    fn test_fixed_pipeline_builder() {
        let builder = FixedPipeline::builder();
        assert_eq!(builder.pass_count(), 0);
    }
}
