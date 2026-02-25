//! Viewport system for configurable render targets.
//!
//! Provides a builder-pattern API for creating viewports that can be used for:
//! - Main scene rendering (offscreen or direct to swapchain)
//! - Preview/model viewer panels
//! - Minimaps, reflection probes, etc.
//!
//! # Example
//!
//! ```ignore
//! // Create an offscreen viewport for UI display
//! let viewport = renderer.create_viewport()
//!     .size(512, 512)
//!     .with_depth(DepthFormat::D32SfloatS8Uint)
//!     .output_mode(OutputMode::Offscreen)
//!     .label("preview")
//!     .build(&mut renderer)?;
//!
//! // Render to viewport
//! renderer.render_viewport(viewport, &camera, &draw_list);
//!
//! // Get texture for UI
//! ui.image(renderer.viewport_texture(viewport));
//! ```

use log::info;

use crate::render_graph::types::{Extent2D, ImageFormat};
use crate::renderer::DrawList;
use crate::renderer::ViewportRenderTarget;
use crate::sync::{
    AccessFlags2, DependencyInfo, ImageMemoryBarrier2, PipelineStage2Flags, VkImage, VkImageView,
};
use crate::vulkan::material::storage_uniform::{StorageDescriptorSet, StorageUniformManager};
use crate::{CompiledRenderGraph, FrameUniforms, RenderGraphError, VulkanContext};
use ash::vk;
use std::cell::RefCell;
use std::rc::Rc;

// ============================================================================
// Public Types
// ============================================================================

/// Output mode for a viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// Render to offscreen texture (UI can sample it).
    /// Used for editor viewports, preview panels, minimaps, etc.
    #[default]
    Offscreen,
    /// Render directly to swapchain (standalone game, no UI overlay).
    /// Maximum performance for shipped games.
    DirectToSwapchain,
}

/// Depth buffer format for a viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DepthFormat {
    /// No depth buffer.
    None,
    /// 32-bit floating point depth.
    D32Sfloat,
    /// 32-bit floating point depth + 8-bit stencil.
    #[default]
    D32SfloatS8Uint,
    /// 24-bit depth + 8-bit stencil.
    D24UnormS8Uint,
}

/// Opaque handle to a created viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewportHandle(pub usize);

impl ViewportHandle {
    /// Create a new viewport handle (internal use only).
    pub(crate) fn new(index: usize) -> Self {
        Self(index)
    }
}

// ============================================================================
// ViewportBuilder
// ============================================================================

/// Builder for creating configurable viewports.
///
/// # Example
///
/// ```ignore
/// let viewport = renderer.create_viewport()
///     .size(1024, 768)
///     .with_depth(DepthFormat::D32SfloatS8Uint)
///     .output_mode(OutputMode::Offscreen)
///     .clear_color(0.1, 0.1, 0.1, 1.0)
///     .label("preview")
///     .build(&mut renderer)?;
/// ```
pub struct ViewportBuilder {
    width: u32,
    height: u32,
    depth_format: DepthFormat,
    color_format: ImageFormat,
    output_mode: OutputMode,
    clear_color: [f32; 4],
    label: String,
}

impl ViewportBuilder {
    /// Create a new viewport builder with default settings.
    pub fn new() -> Self {
        Self {
            width: 512,
            height: 512,
            depth_format: DepthFormat::default(),
            color_format: ImageFormat::R16G16B16A16Sfloat,
            output_mode: OutputMode::default(),
            clear_color: [0.1, 0.1, 0.1, 1.0],
            label: String::from("viewport"),
        }
    }

    /// Set the viewport size in pixels.
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Set the depth buffer format.
    pub fn with_depth(mut self, format: DepthFormat) -> Self {
        self.depth_format = format;
        self
    }

    /// Set the color buffer format.
    pub fn color_format(mut self, format: ImageFormat) -> Self {
        self.color_format = format;
        self
    }

    /// Set the output mode (offscreen vs direct to swapchain).
    pub fn output_mode(mut self, mode: OutputMode) -> Self {
        self.output_mode = mode;
        self
    }

    /// Set the clear color (RGBA, 0.0-1.0 range).
    pub fn clear_color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.clear_color = [r, g, b, a];
        self
    }

    /// Set a debug label for the viewport.
    pub fn label(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }

    /// Get the viewport extent.
    pub fn extent(&self) -> Extent2D {
        Extent2D::new(self.width, self.height)
    }

    /// Get the clear color.
    pub fn get_clear_color(&self) -> [f32; 4] {
        self.clear_color
    }

    /// Get the label.
    pub fn get_label(&self) -> &str {
        &self.label
    }

    /// Get the depth format.
    pub fn get_depth_format(&self) -> DepthFormat {
        self.depth_format
    }

    /// Get the output mode.
    pub fn get_output_mode(&self) -> OutputMode {
        self.output_mode
    }

    /// Get the color format.
    pub fn get_color_format(&self) -> ImageFormat {
        self.color_format
    }

    /// Get the width.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get the height.
    pub fn height(&self) -> u32 {
        self.height
    }
}

impl Default for ViewportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Viewport (Internal)
// ============================================================================

/// Internal viewport state.
///
/// Holds all resources needed for rendering to a viewport:
/// - Color and depth render targets
/// - Compiled render graph
/// - Storage uniform manager for camera
pub struct Viewport {
    /// Debug label.
    pub label: String,
    /// Viewport extent.
    pub extent: Extent2D,
    /// Output mode.
    pub output_mode: OutputMode,
    /// Render target (color + depth).
    pub render_target: ViewportRenderTarget,
    /// Compiled render graph.
    pub render_graph: Option<CompiledRenderGraph>,
    /// Storage uniform manager for camera.
    pub storage_manager: Option<StorageUniformManager>,
    /// Storage descriptor set.
    pub storage_descriptor: Option<StorageDescriptorSet>,
    /// Draw list cell for render graph.
    pub draw_list_cell: Rc<RefCell<Option<DrawList>>>,
    /// Current frame uniforms.
    pub frame_uniforms: Option<FrameUniforms>,
    /// Clear color.
    pub clear_color: [f32; 4],
}

impl Viewport {
    /// Create a new viewport from builder configuration.
    pub fn new(
        builder: &ViewportBuilder,
        context: &Rc<VulkanContext>,
    ) -> Result<Self, RenderGraphError> {
        // Create render target using existing ViewportRenderTarget
        let render_target = ViewportRenderTarget::new(
            context.clone(),
            builder.width,
            builder.height,
        )
        .map_err(|e| {
            RenderGraphError::CompilationError(format!("Failed to create render target: {:?}", e))
        })?;

        info!(
            "Created viewport '{}' ({}x{}, depth={:?}, mode={:?})",
            builder.label, builder.width, builder.height, builder.depth_format, builder.output_mode
        );

        Ok(Self {
            label: builder.label.clone(),
            extent: builder.extent(),
            output_mode: builder.output_mode,
            render_target,
            render_graph: None,
            storage_manager: None,
            storage_descriptor: None,
            draw_list_cell: Rc::new(RefCell::new(None)),
            frame_uniforms: None,
            clear_color: builder.clear_color,
        })
    }

    /// Get the color image view (for UI sampling).
    pub fn color_view(&self) -> VkImageView {
        self.render_target.color_view()
    }

    /// Get the color image.
    pub fn color_image(&self) -> VkImage {
        VkImage::new(self.render_target.color_image())
    }

    /// Get the depth image view.
    pub fn depth_view(&self) -> VkImageView {
        VkImageView::new(self.render_target.depth_view())
    }

    /// Get the depth image.
    pub fn depth_image(&self) -> VkImage {
        VkImage::new(self.render_target.depth_image())
    }

    /// Get the viewport extent.
    pub fn get_extent(&self) -> Extent2D {
        self.extent
    }

    /// Set frame uniforms for this viewport.
    pub fn set_frame_uniforms(&mut self, uniforms: FrameUniforms) {
        self.frame_uniforms = Some(uniforms);
    }

    /// Set the draw list for rendering.
    pub fn set_draw_list(&self, draw_list: DrawList) {
        *self.draw_list_cell.borrow_mut() = Some(draw_list);
    }

    /// Clear the draw list.
    pub fn clear_draw_list(&self) {
        *self.draw_list_cell.borrow_mut() = None;
    }

    /// Transition color texture from shader read to color attachment.
    /// Call BEFORE rendering when UI has sampled the texture.
    pub fn transition_to_render(&self, cmd_buf: vk::CommandBuffer, device: &ash::Device) {
        let color_image = self.color_image();
        let depth_image = self.depth_image();

        let color_subresource = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        let depth_subresource = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        // Color: SHADER_READ_ONLY -> COLOR_ATTACHMENT
        let color_barrier = ImageMemoryBarrier2::new(color_image)
            .src_stage(PipelineStage2Flags::FRAGMENT_SHADER)
            .src_access(AccessFlags2::SHADER_READ)
            .dst_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .subresource_range(color_subresource);

        // Depth: stays in DEPTH_ATTACHMENT (just stage sync)
        let depth_barrier = ImageMemoryBarrier2::new(depth_image)
            .src_stage(PipelineStage2Flags::LATE_FRAGMENT_TESTS)
            .src_access(AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
            .dst_stage(PipelineStage2Flags::EARLY_FRAGMENT_TESTS)
            .dst_access(
                AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                    .union(AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE),
            )
            .old_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .subresource_range(depth_subresource);

        unsafe {
            DependencyInfo::new()
                .add_image_barrier(color_barrier)
                .add_image_barrier(depth_barrier)
                .build(|dep_info| {
                    device.cmd_pipeline_barrier2(cmd_buf, dep_info);
                });
        }
    }

    /// Transition color texture from color attachment to shader read.
    /// Call AFTER rendering so UI can sample the texture.
    pub fn transition_to_sample(&self, cmd_buf: vk::CommandBuffer, device: &ash::Device) {
        let color_image = self.color_image();

        let subresource = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        // Color: COLOR_ATTACHMENT -> SHADER_READ_ONLY
        let barrier = ImageMemoryBarrier2::new(color_image)
            .src_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
            .src_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage(PipelineStage2Flags::FRAGMENT_SHADER)
            .dst_access(AccessFlags2::SHADER_READ)
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .subresource_range(subresource);

        unsafe {
            DependencyInfo::new()
                .add_image_barrier(barrier)
                .build(|dep_info| {
                    device.cmd_pipeline_barrier2(cmd_buf, dep_info);
                });
        }
    }
}

// ============================================================================
// DepthFormat conversions
// ============================================================================

impl From<DepthFormat> for vk::Format {
    fn from(format: DepthFormat) -> Self {
        match format {
            DepthFormat::None => vk::Format::UNDEFINED,
            DepthFormat::D32Sfloat => vk::Format::D32_SFLOAT,
            DepthFormat::D32SfloatS8Uint => vk::Format::D32_SFLOAT_S8_UINT,
            DepthFormat::D24UnormS8Uint => vk::Format::D24_UNORM_S8_UINT,
        }
    }
}

impl From<DepthFormat> for ImageFormat {
    fn from(format: DepthFormat) -> Self {
        match format {
            DepthFormat::None => ImageFormat::R8G8B8A8Srgb, // Placeholder
            DepthFormat::D32Sfloat => ImageFormat::D32Sfloat,
            DepthFormat::D32SfloatS8Uint => ImageFormat::D32SfloatS8Uint,
            DepthFormat::D24UnormS8Uint => ImageFormat::D24UnormS8Uint,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_builder_defaults() {
        let builder = ViewportBuilder::new();
        assert_eq!(builder.width(), 512);
        assert_eq!(builder.height(), 512);
        assert_eq!(builder.get_depth_format(), DepthFormat::D32SfloatS8Uint);
        assert_eq!(builder.get_output_mode(), OutputMode::Offscreen);
    }

    #[test]
    fn test_viewport_builder_chain() {
        let builder = ViewportBuilder::new()
            .size(1024, 768)
            .with_depth(DepthFormat::D32Sfloat)
            .output_mode(OutputMode::DirectToSwapchain)
            .clear_color(0.5, 0.5, 0.5, 1.0)
            .label("test");

        assert_eq!(builder.width(), 1024);
        assert_eq!(builder.height(), 768);
        assert_eq!(builder.get_depth_format(), DepthFormat::D32Sfloat);
        assert_eq!(builder.get_output_mode(), OutputMode::DirectToSwapchain);
        assert_eq!(builder.get_clear_color(), [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(builder.get_label(), "test");
    }

    #[test]
    fn test_depth_format_conversion() {
        assert_eq!(vk::Format::from(DepthFormat::None), vk::Format::UNDEFINED);
        assert_eq!(
            vk::Format::from(DepthFormat::D32Sfloat),
            vk::Format::D32_SFLOAT
        );
        assert_eq!(
            vk::Format::from(DepthFormat::D32SfloatS8Uint),
            vk::Format::D32_SFLOAT_S8_UINT
        );
    }

    #[test]
    fn test_viewport_handle() {
        let handle = ViewportHandle::new(42);
        assert_eq!(handle.0, 42);

        let handle2 = ViewportHandle::new(42);
        assert_eq!(handle, handle2);

        let handle3 = ViewportHandle::new(43);
        assert_ne!(handle, handle3);
    }
}
