//! Fixed render pipeline builder.
//!
//! This module provides a builder API for constructing fixed render pipelines.

use ash::vk;
use std::collections::HashMap;
use std::rc::Rc;

use katla_gfx::render_pass::{PassExecutor, RenderPass};
use katla_gfx::texture::ImageFormat;
use katla_gfx::VulkanContext;

use super::fixed::{AttachmentConfig, AttachmentSize, FixedPipeline};

/// Error type for pipeline creation.
#[derive(Debug, Clone)]
pub enum PipelineError {
    /// No passes were added to the pipeline.
    NoPasses,
    /// Failed to create attachment.
    AttachmentCreationFailed { name: String, reason: String },
    /// Invalid attachment configuration.
    InvalidConfig { reason: String },
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPasses => write!(f, "No render passes were added to the pipeline"),
            Self::AttachmentCreationFailed { name, reason } => {
                write!(f, "Failed to create attachment '{}': {}", name, reason)
            }
            Self::InvalidConfig { reason } => write!(f, "Invalid configuration: {}", reason),
        }
    }
}

impl std::error::Error for PipelineError {}

/// Builder for creating fixed render pipelines.
///
/// This builder collects render passes and attachment configurations,
/// then constructs a `FixedPipeline` that can execute the passes.
///
/// # Example
///
/// ```ignore
/// use katla_app::rendering::pipeline::{FixedPipeline, AttachmentSize};
/// use katla_gfx::render_pass::passes::{GeometryPass, TonemapPass, UIPass};
/// use katla_gfx::texture::ImageFormat;
///
/// let pipeline = FixedPipeline::builder()
///     .pass(GeometryPass::new()
///         .output_color("color", ImageFormat::R16G16B16A16Sfloat)
///         .output_depth("depth", ImageFormat::D32Sfloat))
///     .pass(TonemapPass::new()
///         .input("color")
///         .output("ldr", ImageFormat::B8G8R8A8Srgb))
///     .pass(UIPass::new()
///         .background("ldr")
///         .load_background(true))
///     .build(context)?;
/// ```
pub struct FixedPipelineBuilder {
    passes: Vec<Box<dyn RenderPass>>,
    attachments: HashMap<String, AttachmentConfig>,
}

impl FixedPipelineBuilder {
    /// Create a new pipeline builder.
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            attachments: HashMap::new(),
        }
    }

    /// Add a render pass to the pipeline.
    ///
    /// Passes are executed in the order they are added.
    ///
    /// # Arguments
    /// * `pass` - The render pass to add
    pub fn pass<P: RenderPass + 'static>(mut self, pass: P) -> Self {
        self.passes.push(Box::new(pass));
        self
    }

    /// Add a render pass using a mutable reference (for chaining).
    ///
    /// This is useful when you need to configure a pass before adding it.
    pub fn pass_mut<P: RenderPass + 'static>(&mut self, pass: P) -> &mut Self {
        self.passes.push(Box::new(pass));
        self
    }

    /// Add a boxed render pass.
    pub fn pass_boxed(mut self, pass: Box<dyn RenderPass>) -> Self {
        self.passes.push(pass);
        self
    }

    /// Configure a color attachment.
    ///
    /// # Arguments
    /// * `name` - Unique name for the attachment
    /// * `format` - Image format
    /// * `size` - Size specification
    pub fn attachment(
        mut self,
        name: impl Into<String>,
        format: ImageFormat,
        size: AttachmentSize,
    ) -> Self {
        let config = AttachmentConfig::new(format, size);
        self.attachments.insert(name.into(), config);
        self
    }

    /// Configure a depth attachment.
    ///
    /// # Arguments
    /// * `name` - Unique name for the attachment
    /// * `format` - Depth format (e.g., D32Sfloat)
    /// * `size` - Size specification
    pub fn depth_attachment(
        mut self,
        name: impl Into<String>,
        format: ImageFormat,
        size: AttachmentSize,
    ) -> Self {
        let config = AttachmentConfig::depth(format, size);
        self.attachments.insert(name.into(), config);
        self
    }

    /// Get the number of passes added.
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    /// Get the number of attachments configured.
    pub fn attachment_count(&self) -> usize {
        self.attachments.len()
    }

    /// Check if an attachment exists.
    pub fn has_attachment(&self, name: &str) -> bool {
        self.attachments.contains_key(name)
    }

    /// Build the fixed pipeline.
    ///
    /// # Arguments
    /// * `context` - Vulkan context for resource creation
    ///
    /// # Returns
    /// A `FixedPipeline` ready for rendering, or an error.
    pub fn build(self, context: Rc<VulkanContext>) -> Result<FixedPipeline, PipelineError> {
        if self.passes.is_empty() {
            return Err(PipelineError::NoPasses);
        }

        // Create the pass executor
        let executor = PassExecutor::new(context);

        // Default extent (will be set on first render)
        let current_extent = vk::Extent2D {
            width: 1,
            height: 1,
        };

        Ok(FixedPipeline::new(
            self.passes,
            executor,
            self.attachments,
            current_extent,
        ))
    }

    /// Build the pipeline with an initial extent.
    ///
    /// This is equivalent to `build()` followed by `resize()`.
    pub fn build_with_extent(
        self,
        context: Rc<VulkanContext>,
        extent: vk::Extent2D,
    ) -> Result<FixedPipeline, PipelineError> {
        let mut pipeline = self.build(context)?;
        pipeline.resize(extent);
        Ok(pipeline)
    }
}

impl Default for FixedPipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_new() {
        let builder = FixedPipelineBuilder::new();
        assert_eq!(builder.pass_count(), 00);
        assert_eq!(builder.attachment_count(), 0);
    }

    #[test]
    fn test_builder_default() {
        let builder = FixedPipelineBuilder::default();
        assert_eq!(builder.pass_count(), 0);
    }

    #[test]
    fn test_builder_attachment() {
        let builder = FixedPipelineBuilder::new().attachment(
            "color",
            ImageFormat::R8G8B8A8Srgb,
            AttachmentSize::SwapchainRelative(1.0),
        );

        assert!(builder.has_attachment("color"));
        assert_eq!(builder.attachment_count(), 1);
    }

    #[test]
    fn test_pipeline_error_display() {
        let err = PipelineError::NoPasses;
        assert_eq!(
            format!("{}", err),
            "No render passes were added to the pipeline"
        );

        let err = PipelineError::AttachmentCreationFailed {
            name: "test".to_string(),
            reason: "failed".to_string(),
        };
        assert_eq!(
            format!("{}", err),
            "Failed to create attachment 'test': failed"
        );
    }

    #[test]
    fn test_pipeline_error_invalid_config() {
        let err = PipelineError::InvalidConfig {
            reason: "test".to_string(),
        };
        assert_eq!(format!("{}", err), "Invalid configuration: test");
    }
}
