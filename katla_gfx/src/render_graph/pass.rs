//! Pass types for render graph execution.

use crate::render_graph::ViewportRect;
use crate::render_graph::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

/// Callback for custom compute dispatch logic.
///
/// Receives mutable access to the frame (and thus the renderer),
/// the command buffer, and the pipeline handle assigned to the pass.
pub type ComputeFn = Box<
    dyn Fn(
        &mut super::frame::Frame,
        &crate::vulkan::commandbuffer::CommandBuffer,
        crate::handle::PipelineHandle,
    ) -> Result<(), super::error::RenderGraphError>,
>;

/// Type of render pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PassType {
    /// Graphics pass (rendering to attachments).
    #[default]
    Graphics,
    /// Compute pass (GPU compute work).
    Compute,
}

/// Semantic kind of a render pass, used for dispatch routing.
///
/// Set at build time by each pass template. Eliminates structural heuristics
/// (checking `material.is_none() && pipeline.is_none()`) in the execution loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassKind {
    /// Depth prepass — renders depth and optional object IDs.
    DepthPrepass,
    /// Shadow mapping — renders shadow depth into atlas.
    Shadow,
    /// Geometry — renders 3D scene geometry with material.
    Geometry,
    /// Particles — renders GPU particles with alpha blending.
    Particles,
    /// Outline — stencil-based selection highlight.
    Outline,
    /// Stencil indicator — writes R8 mask where stencil==2.
    StencilIndicator,
    /// Fullscreen — post-processing (tonemap, etc.) with pipeline.
    Fullscreen,
    /// Compositing — multi-viewport compositing.
    Compositing,
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
    /// Optional overlay parameters (for wallhack overlay passes).
    pub overlay_params: Option<crate::render_graph::passes::OverlayParams>,
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
    /// Optional compute dispatch callback for compute passes.
    /// When set, this closure is called instead of a generic dispatch.
    pub compute_fn: Option<ComputeFn>,

    /// Semantic kind of this pass, used for dispatch routing.
    /// Set at build time by each pass template.
    pub kind: Option<PassKind>,
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
            overlay_params: None,
            material: None,
            output_format: None,
            color_attachments: Vec::new(),
            uses_depth: true,
            depth_attachment: None,
            compositing_viewports: None,
            compute_fn: None,
            kind: None,
        }
    }

    /// Set the pipeline for this pass.
    pub fn with_pipeline(mut self, pipeline: crate::handle::PipelineHandle) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    /// Attach a compute dispatch callback to this pass.
    pub fn with_compute_fn(
        mut self,
        f: impl Fn(
            &mut super::frame::Frame,
            &crate::vulkan::commandbuffer::CommandBuffer,
            crate::handle::PipelineHandle,
        ) -> Result<(), super::error::RenderGraphError>
        + 'static,
    ) -> Self {
        self.compute_fn = Some(Box::new(f));
        self
    }

    /// Check if this pass writes to a specific resource by name (no allocation).
    #[inline]
    pub fn writes_to(&self, name: &str) -> bool {
        self.writes.iter().any(|w| w == name)
    }

    /// Check if this pass reads from a specific resource by name (no allocation).
    #[inline]
    pub fn reads_from(&self, name: &str) -> bool {
        self.reads.iter().any(|r| r == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_desc_with_pipeline() {
        let desc = PassDesc::new("test", PassType::Graphics, vec![], vec!["out".to_string()])
            .with_pipeline(crate::handle::PipelineHandle::new(42));

        assert_eq!(desc.pipeline.unwrap().index(), 42);
    }

    #[test]
    fn test_pass_desc_defaults() {
        let desc = PassDesc::new(
            "test",
            PassType::Graphics,
            vec!["r".to_string()],
            vec!["w".to_string()],
        );
        assert!(desc.pipeline.is_none());
        assert!(desc.tonemap_params.is_none());
        assert!(desc.material.is_none());
        assert!(desc.output_format.is_none());
        assert!(desc.color_attachments.is_empty());
        assert!(desc.depth_attachment.is_none());
        assert!(desc.compositing_viewports.is_none());
        assert!(desc.compute_fn.is_none());
        assert!(desc.kind.is_none());
        assert!(desc.uses_depth);
    }
}
