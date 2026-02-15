//! Vulkan renderer for UI draw lists.
//!
//! This module provides the bridge between `DrawList` and Vulkan rendering.
//! It manages the UI pipeline, vertex/index buffers, and texture atlas.
//!
//! # Status
//!
//! This is a skeleton implementation. Full implementation requires:
//! - UI-specific Vulkan pipeline with alpha blending
//! - Vertex/index buffer management
//! - Texture atlas for font glyphs
//! - Integration with katla_vulkan's render graph

use std::rc::Rc;

use katla_math::Vec2;
use katla_vulkan::VulkanContext;

use crate::draw_list::DrawList;

/// Renderer for UI draw lists.
///
/// This handles:
/// - Creating and managing the UI pipeline
/// - Uploading vertex/index data to GPU
/// - Rendering draw commands
///
/// # Example
///
/// ```ignore
/// let mut renderer = UiRenderer::new(context)?;
/// renderer.create_pipeline(color_format, None)?;
///
/// // In render loop:
/// renderer.render(command_buffer, draw_list, screen_size);
/// ```
pub struct UiRenderer {
    #[allow(dead_code)]
    context: Rc<VulkanContext>,
    // TODO: Add pipeline, buffers, texture atlas
    // These will use katla_vulkan types, not raw ash types
}

impl UiRenderer {
    /// Create a new UI renderer.
    pub fn new(context: Rc<VulkanContext>) -> Result<Self, UiRenderError> {
        Ok(Self { context })
    }

    /// Create or update the pipeline for a specific render format.
    ///
    /// This should be called once when the swapchain is created.
    pub fn create_pipeline(
        &mut self,
        _color_format: katla_vulkan::ImageFormat,
        _depth_format: Option<katla_vulkan::ImageFormat>,
    ) -> Result<(), UiRenderError> {
        // TODO: Create UI pipeline with:
        // - Alpha blending enabled
        // - No depth testing (UI is always on top)
        // - Dynamic viewport/scissor
        // - Vertex format matching UiVertex
        Ok(())
    }

    /// Render a draw list to the command buffer.
    ///
    /// # Arguments
    /// * `_cmd` - Command buffer to record into (placeholder for now)
    /// * `draw_list` - The draw list to render
    /// * `screen_size` - Current screen/viewport size
    pub fn render(
        &mut self,
        _cmd: katla_vulkan::CommandBuffer,
        draw_list: &DrawList,
        screen_size: Vec2,
    ) {
        if draw_list.is_empty() {
            return;
        }

        // TODO: Implement actual rendering:
        // 1. Update vertex/index buffers
        // 2. Bind pipeline
        // 3. Set viewport/scissor
        // 4. Draw each command with appropriate clip rect

        // Placeholder: Just log stats
        log::debug!(
            "UiRenderer: {} vertices, {} indices, {} commands, screen {:?}",
            draw_list.vertex_count(),
            draw_list.index_count(),
            draw_list.command_count(),
            screen_size
        );
    }

    /// Clean up resources.
    pub fn destroy(&mut self) {
        // TODO: Clean up pipeline, buffers, texture atlas
    }
}

impl Drop for UiRenderer {
    fn drop(&mut self) {
        self.destroy();
    }
}

/// Errors that can occur in UI rendering.
#[derive(Debug, Clone)]
pub enum UiRenderError {
    /// Vulkan error occurred.
    VulkanError(String),
    /// Failed to create pipeline.
    PipelineCreationFailed(String),
    /// Failed to allocate buffers.
    BufferAllocationFailed(String),
    /// Failed to create texture atlas.
    TextureAtlasFailed(String),
}

impl std::fmt::Display for UiRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiRenderError::VulkanError(msg) => write!(f, "Vulkan error: {}", msg),
            UiRenderError::PipelineCreationFailed(msg) => {
                write!(f, "Pipeline creation failed: {}", msg)
            }
            UiRenderError::BufferAllocationFailed(msg) => {
                write!(f, "Buffer allocation failed: {}", msg)
            }
            UiRenderError::TextureAtlasFailed(msg) => {
                write!(f, "Texture atlas creation failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for UiRenderError {}
