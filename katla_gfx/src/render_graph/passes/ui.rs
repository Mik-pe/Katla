//! UI render pass template.
//!
//! Renders 2D UI geometry with alpha blending.

use std::collections::HashMap;

use crate::handle::MaterialHandle;
use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::pass::PassType;
use crate::render_graph::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

#[allow(dead_code)]

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

    /// Write to a color attachment.
    ///
    /// Alias for write() method for API consistency.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name for graph reference.
    /// * `format` - Image format (unused, kept for API compatibility).
    /// * `load_op` - Load operation (unused, kept for API compatibility).
    /// * `store_op` - Store operation (unused, kept for API compatibility).
    /// * `clear_value` - Clear value (unused, kept for API compatibility).
    #[allow(clippy::too_many_arguments)]
    pub fn write_with(
        mut self,
        name: impl Into<String>,
        _format: ImageFormat,
        _load_op: LoadOp,
        _store_op: StoreOp,
        _clear_value: ClearValue,
    ) -> Self {
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

/// Internal data for a UI pass after name resolution.
#[derive(Debug)]
pub(crate) struct UIPassData;

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
            material,
            output_format: None,
            build_fn: Box::new(
                move |_resource_map: &HashMap<String, GraphResourceHandle>| {
                    // UI pass data is currently unused but kept for future extensibility
                    Ok(Box::new(UIPassData))
                },
            ),
            uses_depth: false, // UI passes don't use depth testing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_pass_new() {
        let pass = UIPass::new("test_ui");
        assert_eq!(pass.name(), "test_ui");
        assert!(pass.color_output.is_none());
        assert!(pass.reads().is_empty());
    }

    #[test]
    fn test_ui_pass_write() {
        let pass = UIPass::new("test").write("color");
        assert!(pass.color_output.is_some());
    }

    #[test]
    fn test_ui_pass_full_setup() {
        let pass = UIPass::new("ui").write("color").read("font_atlas");

        assert_eq!(pass.name(), "ui");
        assert!(pass.color_output.is_some());
        assert_eq!(pass.reads().len(), 1);
        assert_eq!(pass.reads()[0], "font_atlas");
    }

    #[test]
    fn test_ui_pass_material() {
        let material = MaterialHandle::new(0);
        let pass = UIPass::new("ui").write("color").material(material);

        assert_eq!(pass.material, Some(material));
    }

    #[test]
    fn test_ui_pass_builder_trait() {
        let pass = UIPass::new("ui").write("color").read("font_atlas");

        let builder = pass.as_builder();

        assert_eq!(builder.name, "ui");
        assert_eq!(builder.pass_type, PassType::Graphics);
        assert_eq!(builder.reads, vec!["font_atlas"]);
        assert_eq!(builder.writes, vec!["color"]);
    }

    #[test]
    fn test_ui_pass_builder_with_material() {
        let material = MaterialHandle::new(42);
        let pass = UIPass::new("ui").write("color").material(material);

        let builder = pass.as_builder();

        assert_eq!(builder.material, Some(material));
    }

    #[test]
    fn test_ui_pass_build_fn_resolution() {
        let pass = UIPass::new("ui").write("color").read("font_atlas");

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("color".to_string(), GraphResourceHandle::new(0));
        resource_map.insert("font_atlas".to_string(), GraphResourceHandle::new(1));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());

        let data = result.unwrap();
        // UIPassData is a unit struct, just verify downcast works
        assert!(data.downcast_ref::<UIPassData>().is_some());
    }

    #[test]
    fn test_ui_pass_build_fn_empty_resources() {
        let pass = UIPass::new("ui").write("color");

        let builder = pass.as_builder();

        let resource_map = HashMap::new(); // Empty - no resources

        // UIPass build_fn doesn't validate resources, it just returns UIPassData
        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());
    }
}
