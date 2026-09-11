//! UI render pass template.
//!
//! Renders 2D UI geometry with alpha blending.

use std::collections::{HashMap, HashSet};

use crate::handle::MaterialHandle;
use crate::render_graph::access::{
    ImageAccess, ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
};
use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::pass::{PassKind, PassType};
use crate::render_graph::resource::GraphResourceHandle;

/// UI render pass template.
///
/// Renders 2D UI geometry with alpha blending and optional clipping.
///
/// # Example
///
/// ```ignore
/// use katla_gfx::render_graph::{FrameGraph, GeometryPass, UIPass};
///
/// let graph = FrameGraph::builder()
///     .add_pass(GeometryPass::new("geometry")
///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///         .write_depth("depth", ImageFormat::D32Sfloat))
///     .add_pass(UIPass::new("ui")
///         .write("color"))  // Composited on top
///     .build()?;
///
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("ui").draw_ui(&ui_draw_list);
/// })?;
/// ```
#[derive(Debug, Clone)]
pub struct UIPass {
    /// Pass name for debugging.
    name: String,
    /// Color attachment output.
    color_output: Option<ColorOutput>,
    /// Resources read by this pass.
    reads: Vec<String>,
    /// UI material handle for rendering.
    material: Option<MaterialHandle>,
}

/// Describes a color attachment output for UI.
#[derive(Debug, Clone)]
struct ColorOutput {
    /// Resource name.
    name: String,
}

impl UIPass {
    /// Create a new UI pass.
    ///
    /// # Arguments
    ///
    /// * `name` - Pass name for debugging and execution context reference.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color_output: None,
            reads: Vec::new(),
            material: None,
        }
    }

    /// Set the UI material for this pass.
    ///
    /// # Arguments
    ///
    /// * `material` - Material handle for UI rendering.
    pub fn material(mut self, material: MaterialHandle) -> Self {
        self.material = Some(material);
        self
    }

    /// Write to a color attachment.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name for graph reference.
    pub fn write(mut self, name: impl Into<String>) -> Self {
        self.color_output = Some(ColorOutput { name: name.into() });
        self
    }

    /// Read from a resource (e.g., font atlas texture).
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

    /// Get the pass name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the read dependencies.
    pub fn reads(&self) -> &[String] {
        &self.reads
    }
}

impl PassBuilder for UIPass {
    fn as_builder(self) -> InternalPassBuilder {
        // Collect write resource names
        let writes: Vec<String> = self.color_output.iter().map(|o| o.name.clone()).collect();

        // UI alpha-composites over the existing target contents.
        let mut reads = self.reads.clone();
        for output in &writes {
            if !reads.contains(output) {
                reads.push(output.clone());
            }
        }

        // Hand-declared typed accesses: UI composites into its target
        // (read-write color attachment); declared reads (viewport textures,
        // font atlases) are sampled.
        let write_set = writes.iter().cloned().collect::<HashSet<_>>();
        let image_accesses = self
            .reads
            .iter()
            .filter(|name| !write_set.contains(*name))
            .map(|name| {
                super::named_image_access(
                    name.clone(),
                    ImageAccessMode::Read,
                    ImageUsage::Sampled,
                    ImagePipelineStage::FragmentShader,
                    ImageAccess::WHOLE_RESOURCE,
                )
            })
            .chain(writes.iter().map(|name| {
                super::named_image_access(
                    name.clone(),
                    ImageAccessMode::ReadWrite,
                    ImageUsage::ColorAttachment,
                    ImagePipelineStage::ColorAttachmentOutput,
                    ImageSubresourceRange::WHOLE_COLOR,
                )
            }))
            .collect();

        // Clone material handle
        let material = self.material;

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads,
            writes,
            image_accesses,
            pipeline: None,
            tonemap_params: None,
            overlay_params: None,
            material,
            output_format: None,
            build_fn: Box::new(
                move |_resource_map: &HashMap<String, GraphResourceHandle>| Ok(Box::new(())),
            ),
            uses_depth: false,
            // UI alpha-composites over the existing target contents.
            color_attachments: self
                .color_output
                .iter()
                .map(|o| (o.name.clone(), crate::render_pass::AttachmentOps::load()))
                .collect(),
            depth_attachment: None,
            kind: Some(PassKind::Ui),
            side_effect: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_pass_build_fn_resolution() {
        let pass = UIPass::new("ui").write("color").read("font_atlas");

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("color".to_string(), GraphResourceHandle::new(0));
        resource_map.insert("font_atlas".to_string(), GraphResourceHandle::new(1));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());
    }

    #[test]
    fn ui_output_declares_read_dependency_for_compositing() {
        let builder = UIPass::new("ui").write("backbuffer").as_builder();
        assert_eq!(builder.reads, vec!["backbuffer"]);
        assert_eq!(builder.writes, vec!["backbuffer"]);
    }

    #[test]
    fn ui_pass_declares_typed_accesses() {
        use crate::render_graph::access::{
            ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
            NamedImageAccess,
        };

        let builder = UIPass::new("ui")
            .write("backbuffer")
            .read("viewport_0")
            .as_builder();

        assert_eq!(
            builder.image_accesses,
            vec![
                NamedImageAccess {
                    resource: "viewport_0".to_string(),
                    mode: ImageAccessMode::Read,
                    usage: ImageUsage::Sampled,
                    stage: ImagePipelineStage::FragmentShader,
                    range: ImageAccess::WHOLE_RESOURCE,
                },
                NamedImageAccess {
                    resource: "backbuffer".to_string(),
                    mode: ImageAccessMode::ReadWrite,
                    usage: ImageUsage::ColorAttachment,
                    stage: ImagePipelineStage::ColorAttachmentOutput,
                    range: ImageSubresourceRange::WHOLE_COLOR,
                },
            ]
        );
    }

    #[test]
    fn test_ui_pass_build_fn_empty_resources() {
        let pass = UIPass::new("ui").write("color");
        let builder = pass.as_builder();
        let resource_map = HashMap::new();

        // UIPass build_fn doesn't validate resources
        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());
    }
}
