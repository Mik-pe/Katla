//! UI render pass template.
//!
//! Renders 2D UI geometry with alpha blending.

use std::collections::HashMap;

use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::pass::PassType;
use crate::render_graph::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

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
}

/// Describes a color attachment output for UI.
#[derive(Debug, Clone)]
struct ColorOutput {
    /// Resource name.
    name: String,
    /// Image format.
    format: ImageFormat,
    /// Load operation.
    load_op: LoadOp,
    /// Store operation.
    store_op: StoreOp,
    /// Clear value.
    clear_value: ClearValue,
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
        }
    }

    /// Write to a color attachment.
    ///
    /// By default, the attachment uses LoadOp (preserves existing content)
    /// and StoreOp::Store.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name for graph reference.
    /// * `format` - Image format for the color attachment.
    pub fn write(mut self, name: impl Into<String>) -> Self {
        self.color_output = Some(ColorOutput {
            name: name.into(),
            format: ImageFormat::B8G8R8A8Srgb, // Standard swapchain format
            load_op: LoadOp::Load,
            store_op: StoreOp::Store,
            clear_value: ClearValue::TRANSPARENT_BLACK,
        });
        self
    }

    /// Write to a color attachment with custom format and operations.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name for graph reference.
    /// * `format` - Image format for the color attachment.
    /// * `load_op` - How the attachment is loaded.
    /// * `store_op` - How the attachment is stored.
    /// * `clear_value` - Clear value if load_op is Clear.
    pub fn write_with(
        mut self,
        name: impl Into<String>,
        format: ImageFormat,
        load_op: LoadOp,
        store_op: StoreOp,
        clear_value: ClearValue,
    ) -> Self {
        self.color_output = Some(ColorOutput {
            name: name.into(),
            format,
            load_op,
            store_op,
            clear_value,
        });
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
pub(crate) struct UIPassData {
    /// Color attachment with resolved handle.
    pub(crate) color: Option<(
        GraphResourceHandle,
        ImageFormat,
        LoadOp,
        StoreOp,
        ClearValue,
    )>,
    /// Read dependencies with resolved handles.
    pub(crate) reads: Vec<GraphResourceHandle>,
}

impl PassBuilder for UIPass {
    fn as_builder(self) -> InternalPassBuilder {
        // Collect write resource names
        let writes: Vec<String> = self.color_output.iter().map(|o| o.name.clone()).collect();

        // Clone reads for the builder
        let reads = self.reads.clone();

        // Store pass data for the build function
        let color_output = self.color_output;

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads,
            writes,
            pipeline: None,
            tonemap_params: None,
            material: None,
            output_format: None,
            build_fn: Box::new(move |resource_map: &HashMap<String, GraphResourceHandle>| {
                // Resolve color output name to handle
                let color = if let Some(output) = color_output {
                    let handle = resource_map
                        .get(&output.name)
                        .copied()
                        .ok_or_else(|| RenderGraphError::ResourceNotFound(output.name.clone()))?;
                    Some((
                        handle,
                        output.format,
                        output.load_op,
                        output.store_op,
                        output.clear_value,
                    ))
                } else {
                    None
                };

                // Resolve read names to handles
                let resolved_reads = self
                    .reads
                    .iter()
                    .map(|name| {
                        resource_map
                            .get(name)
                            .copied()
                            .ok_or_else(|| RenderGraphError::ResourceNotFound(name.clone()))
                    })
                    .collect::<Result<Vec<_>, RenderGraphError>>()?;

                Ok(Box::new(UIPassData {
                    color,
                    reads: resolved_reads,
                }))
            }),
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
    fn test_ui_pass_builder_trait() {
        let pass = UIPass::new("ui").write("color").read("font_atlas");

        let builder = pass.as_builder();

        assert_eq!(builder.name, "ui");
        assert_eq!(builder.pass_type, PassType::Graphics);
        assert_eq!(builder.reads, vec!["font_atlas"]);
        assert_eq!(builder.writes, vec!["color"]);
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
        let pass_data = data.downcast_ref::<UIPassData>().unwrap();

        assert!(pass_data.color.is_some());
        assert_eq!(pass_data.reads.len(), 1);
    }

    #[test]
    fn test_ui_pass_build_fn_missing_resource() {
        let pass = UIPass::new("ui").write("color");

        let builder = pass.as_builder();

        let resource_map = HashMap::new(); // Empty - no resources

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_err());

        match result {
            Err(RenderGraphError::ResourceNotFound(name)) => assert_eq!(name, "color"),
            _ => panic!("Expected ResourceNotFound error"),
        }
    }
}
