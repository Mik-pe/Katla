//! UI render pass template.
//!
//! Renders 2D UI geometry with alpha blending.

use std::collections::HashMap;

use crate::handle::MaterialHandle;
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

        // Clone reads for the builder
        let reads = self.reads.clone();

        // Clone material handle
        let material = self.material;

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads,
            writes,
            pipeline: None,
            tonemap_params: None,
            overlay_params: None,
            material,
            output_format: None,
            build_fn: Box::new(
                move |_resource_map: &HashMap<String, GraphResourceHandle>| Ok(Box::new(())),
            ),
            uses_depth: false,
            depth_attachment: None,
            kind: Some(PassKind::Ui),
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
    fn test_ui_pass_build_fn_empty_resources() {
        let pass = UIPass::new("ui").write("color");
        let builder = pass.as_builder();
        let resource_map = HashMap::new();

        // UIPass build_fn doesn't validate resources
        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());
    }
}
