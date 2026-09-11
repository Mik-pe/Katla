//! Geometry render pass template.
//!
//! Renders 3D geometry with color outputs. Depth is handled automatically
//! using the global depth buffer.

use crate::render_graph::access::{
    ImageAccess, ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
};
use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::pass::{PassKind, PassType};
use crate::render_pass::{AttachmentOps, ClearValue, DepthStencilAttachmentOps, LoadOp};
use crate::texture::ImageFormat;

/// Geometry render pass template.
///
/// Renders 3D geometry with color outputs. Depth is implicit and uses
/// the global depth buffer managed by the renderer.
///
/// # Example
///
/// ```ignore
/// use katla_gfx::render_graph::GeometryPass;
/// use katla_gfx::texture::ImageFormat;
///
/// let geometry = GeometryPass::new("geometry")
///     .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///     .clear_color([0.1, 0.1, 0.15, 1.0]);
///
/// let graph = FrameGraph::builder()
///     .add_pass(geometry)
///     .build(&renderer)?;
///
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("geometry").draw_list(&draw_list);
/// })?;
/// ```
#[derive(Debug, Clone)]
pub struct GeometryPass {
    /// Pass name for debugging and referencing.
    name: String,
    /// Color attachment outputs.
    color_outputs: Vec<ColorOutput>,
    /// Resources read by this pass (e.g., shadow maps).
    reads: Vec<String>,
    /// Material handle for this pass (optional).
    material: Option<crate::handle::MaterialHandle>,
    /// Depth attachment configuration.
    depth_config: Option<DepthStencilAttachmentOps>,
}

/// Describes a color attachment output.
#[derive(Debug, Clone)]
struct ColorOutput {
    /// Resource name.
    name: String,
    /// Image format.
    format: ImageFormat,
    /// Declared attachment operations.
    ops: AttachmentOps,
}

impl GeometryPass {
    /// Create a new geometry pass.
    ///
    /// # Arguments
    ///
    /// * `name` - Pass name for debugging and execution context reference.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color_outputs: Vec::new(),
            reads: Vec::new(),
            material: None,
            depth_config: None,
        }
    }

    /// Add a color attachment output.
    ///
    /// By default, the attachment is cleared to opaque black and stored.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name for graph reference.
    /// * `format` - Image format for the color attachment.
    pub fn write_color(mut self, name: impl Into<String>, format: ImageFormat) -> Self {
        self.color_outputs.push(ColorOutput {
            name: name.into(),
            format,
            ops: AttachmentOps::clear(ClearValue::OPAQUE_BLACK),
        });
        self
    }

    /// Add a color attachment output with custom load/store operations.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name for graph reference.
    /// * `format` - Image format for the color attachment.
    /// * `ops` - How the attachment is loaded, stored, and cleared.
    pub fn write_color_ops(
        mut self,
        name: impl Into<String>,
        format: ImageFormat,
        ops: AttachmentOps,
    ) -> Self {
        self.color_outputs.push(ColorOutput {
            name: name.into(),
            format,
            ops,
        });
        self
    }

    /// Read from a resource (e.g., shadow map, previous frame).
    ///
    /// Can be called multiple times to add multiple read dependencies.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name to read from.
    pub fn read(mut self, name: impl Into<String>) -> Self {
        self.reads.push(name.into());
        self
    }

    /// Set clear color for the last added color attachment.
    ///
    /// Only applies if the load operation is Clear.
    ///
    /// # Arguments
    ///
    /// * `color` - RGBA clear color (values 0.0 - 1.0).
    pub fn clear_color(mut self, color: [f32; 4]) -> Self {
        if let Some(output) = self.color_outputs.last_mut() {
            output.ops.clear_value = ClearValue::Color(color);
        }
        self
    }

    /// Set the material for this pass.
    ///
    /// If the material was created with `ImageFormat::Auto`, it will be
    /// compiled on-demand for the format specified in `write_color()`.
    ///
    /// # Arguments
    ///
    /// * `material` - Material handle to use for this pass.
    pub fn material(mut self, material: crate::handle::MaterialHandle) -> Self {
        self.material = Some(material);
        self
    }

    /// Configure depth and stencil attachment operations.
    ///
    /// By default, depth is cleared to 0.0 and stored (reverse-Z far plane)
    /// and stencil is cleared. Use a Load depth op after a depth prepass to
    /// reuse depth.
    pub fn depth_config(mut self, depth: AttachmentOps, stencil: AttachmentOps) -> Self {
        self.depth_config = Some(DepthStencilAttachmentOps { depth, stencil });
        self
    }

    /// Get the pass name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the number of color outputs.
    pub fn color_output_count(&self) -> usize {
        self.color_outputs.len()
    }

    /// Get the read dependencies.
    pub fn reads(&self) -> &[String] {
        &self.reads
    }
}

impl PassBuilder for GeometryPass {
    fn as_builder(self) -> InternalPassBuilder {
        // Collect write resource names (color only - depth is implicit).
        let writes: Vec<String> = self.color_outputs.iter().map(|o| o.name.clone()).collect();

        // Loading an output preserves its previous contents and is therefore
        // a read-before-write dependency for ordering and pass liveness.
        let mut reads = self.reads.clone();
        for output in &self.color_outputs {
            if output.ops.load == LoadOp::Load && !reads.contains(&output.name) {
                reads.push(output.name.clone());
            }
        }

        let color_attachments = self
            .color_outputs
            .iter()
            .map(|o| (o.name.clone(), o.ops))
            .collect();

        // Hand-declared typed accesses: every color output is an attachment
        // (read-write when its contents are loaded), every other read is a
        // sampled access.
        let image_accesses = self
            .reads
            .iter()
            .map(|name| {
                super::named_image_access(
                    name.clone(),
                    ImageAccessMode::Read,
                    ImageUsage::Sampled,
                    ImagePipelineStage::FragmentShader,
                    ImageAccess::WHOLE_RESOURCE,
                )
            })
            .chain(self.color_outputs.iter().map(|output| {
                super::named_image_access(
                    output.name.clone(),
                    if output.ops.load == LoadOp::Load {
                        ImageAccessMode::ReadWrite
                    } else {
                        ImageAccessMode::Write
                    },
                    ImageUsage::ColorAttachment,
                    ImagePipelineStage::ColorAttachmentOutput,
                    ImageSubresourceRange::WHOLE_COLOR,
                )
            }))
            .collect();

        // Extract output format from first color attachment (for material format inference).
        //
        // Note: When using `ImageFormat::Auto` materials with multiple render targets (MRT),
        // only the first color attachment's format is used for compilation. Mixed-format MRT
        // is not supported with Auto materials - use explicit format materials for that case.
        let output_format = self.color_outputs.first().map(|o| o.format);

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads,
            writes,
            image_accesses,
            pipeline: None,
            tonemap_params: None,
            overlay_params: None,
            material: self.material,
            output_format,
            build_fn: Box::new(|_| Ok(Box::new(()))),
            uses_depth: true,
            color_attachments,
            depth_attachment: self.depth_config,
            kind: Some(PassKind::Geometry),
            side_effect: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_pass::StoreOp;

    #[test]
    fn write_color_declares_clear_ops() {
        let builder = GeometryPass::new("geometry")
            .write_color("color", ImageFormat::R16G16B16A16Sfloat)
            .as_builder();

        assert_eq!(builder.writes, vec!["color"]);
        let (name, ops) = &builder.color_attachments[0];
        assert_eq!(name, "color");
        assert_eq!(ops.load, LoadOp::Clear);
        assert_eq!(ops.store, StoreOp::Store);
        assert_eq!(ops.clear_value, ClearValue::OPAQUE_BLACK);
    }

    #[test]
    fn write_color_ops_preserves_declared_ops() {
        let builder = GeometryPass::new("geometry")
            .write_color_ops(
                "color",
                ImageFormat::R16G16B16A16Sfloat,
                AttachmentOps::load(),
            )
            .as_builder();

        assert_eq!(builder.color_attachments[0].1.load, LoadOp::Load);
        assert!(builder.reads.contains(&"color".to_string()));
    }

    #[test]
    fn test_geometry_pass_multiple_color_outputs() {
        let pass = GeometryPass::new("geometry")
            .write_color("albedo", ImageFormat::R8G8B8A8Srgb)
            .write_color("normals", ImageFormat::R16G16B16A16Sfloat);

        let builder = pass.as_builder();
        assert_eq!(builder.color_attachments.len(), 2);
        assert_eq!(builder.writes, vec!["albedo", "normals"]);
    }

    #[test]
    fn load_color_attachment_declares_read_dependency() {
        let builder = GeometryPass::new("geometry")
            .write_color_ops(
                "color",
                ImageFormat::R16G16B16A16Sfloat,
                AttachmentOps::load(),
            )
            .as_builder();

        assert_eq!(builder.reads, vec!["color"]);
        assert_eq!(builder.writes, vec!["color"]);
    }

    #[test]
    fn geometry_pass_declares_typed_accesses() {
        use crate::render_graph::access::{
            ImageAccessMode, ImageAspects, ImageUsage, NamedImageAccess,
        };

        let builder = GeometryPass::new("geometry")
            .write_color("color", ImageFormat::R16G16B16A16Sfloat)
            .write_color_ops(
                "blended",
                ImageFormat::R16G16B16A16Sfloat,
                AttachmentOps::load(),
            )
            .read("shadow_atlas")
            .as_builder();

        assert_eq!(
            builder.image_accesses,
            vec![
                NamedImageAccess {
                    resource: "shadow_atlas".to_string(),
                    mode: ImageAccessMode::Read,
                    usage: ImageUsage::Sampled,
                    stage: crate::render_graph::access::ImagePipelineStage::FragmentShader,
                    range: ImageAccess::WHOLE_RESOURCE,
                },
                NamedImageAccess {
                    resource: "color".to_string(),
                    mode: ImageAccessMode::Write,
                    usage: ImageUsage::ColorAttachment,
                    stage: crate::render_graph::access::ImagePipelineStage::ColorAttachmentOutput,
                    range: ImageSubresourceRange::WHOLE_COLOR,
                },
                NamedImageAccess {
                    resource: "blended".to_string(),
                    mode: ImageAccessMode::ReadWrite,
                    usage: ImageUsage::ColorAttachment,
                    stage: crate::render_graph::access::ImagePipelineStage::ColorAttachmentOutput,
                    range: ImageSubresourceRange::WHOLE_COLOR,
                },
            ]
        );
        assert_eq!(
            builder.image_accesses[0].range.aspects,
            ImageAspects::ALL,
            "generic sampled reads must cover every aspect: sampling a depth atlas hazards against the depth write"
        );
    }

    #[test]
    fn test_geometry_pass_material_propagates() {
        let material = crate::handle::MaterialHandle::from_raw(42, 0);
        let pass = GeometryPass::new("test")
            .write_color("color", ImageFormat::R16G16B16A16Sfloat)
            .material(material);

        let builder = pass.as_builder();
        assert_eq!(builder.material, Some(material));
        assert_eq!(builder.output_format, Some(ImageFormat::R16G16B16A16Sfloat));
    }
}
