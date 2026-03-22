//! Pass types for render graph execution.

use crate::render_graph::ViewportRect;
use crate::render_graph::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

/// Type of render pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PassType {
    /// Graphics pass (rendering to attachments).
    #[default]
    Graphics,
    /// Compute pass (GPU compute work).
    Compute,
}

/// Internal pass descriptor.
pub struct PassDesc {
    /// Human-readable name for debugging.
    pub name: String,
    /// Names of resources this pass reads from.
    pub reads: Vec<String>,
    /// Names of resources this pass writes to.
    pub writes: Vec<String>,
    /// Pass type (graphics, compute, transfer).
    pub pass_type: PassType,
    /// Optional pipeline handle (for fullscreen/compute passes).
    pub pipeline: Option<crate::handle::PipelineHandle>,
    /// Optional tonemap parameters (for HDR tonemapping passes).
    pub tonemap_params: Option<crate::render_graph::passes::TonemapParams>,
    /// Optional material handle (for geometry passes).
    pub material: Option<crate::handle::MaterialHandle>,
    /// Output color format (for material format inference).
    pub output_format: Option<crate::texture::ImageFormat>,
    /// Color attachment load/store ops for each write target.
    pub color_attachments: Vec<(String, ImageFormat, LoadOp, StoreOp, ClearValue)>,
    /// Whether this pass uses depth testing (default true for graphics passes).
    pub uses_depth: bool,
    /// Depth attachment load/store/clear configuration.
    /// When None, defaults to (Clear, Store, depth=0.0) for reverse-Z.
    pub depth_attachment: Option<(LoadOp, StoreOp, ClearValue)>,
    /// Compositing pass data: viewport textures with rectangles.
    /// Set for CompositePass, None for other pass types.
    pub compositing_viewports: Option<Vec<(GraphResourceHandle, ViewportRect)>>,
}

impl PassDesc {
    /// Create a new pass descriptor.
    pub fn new(
        name: impl Into<String>,
        pass_type: PassType,
        reads: Vec<String>,
        writes: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            reads,
            writes,
            pass_type,
            pipeline: None,
            tonemap_params: None,
            material: None,
            output_format: None,
            color_attachments: Vec::new(),
            uses_depth: true,
            depth_attachment: None,
            compositing_viewports: None,
        }
    }

    /// Set the pipeline for this pass.
    pub fn with_pipeline(mut self, pipeline: crate::handle::PipelineHandle) -> Self {
        self.pipeline = Some(pipeline);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_type_default() {
        assert_eq!(PassType::default(), PassType::Graphics);
    }

    #[test]
    fn test_pass_type_equality() {
        assert_eq!(PassType::Graphics, PassType::Graphics);
    }

    #[test]
    fn test_pass_desc_new() {
        let reads = vec!["input".to_string()];
        let writes = vec!["output".to_string()];
        let desc = PassDesc::new(
            "test_pass",
            PassType::Graphics,
            reads.clone(),
            writes.clone(),
        );

        assert_eq!(desc.name, "test_pass");
        assert_eq!(desc.pass_type, PassType::Graphics);
        assert_eq!(desc.reads, vec!["input"]);
        assert_eq!(desc.writes, vec!["output"]);
    }

    #[test]
    fn test_pass_desc_new_default() {
        let desc = PassDesc::new("test", PassType::default(), vec![], vec![]);
        assert_eq!(desc.name, "test");
        assert_eq!(desc.pass_type, PassType::Graphics);
    }
}
