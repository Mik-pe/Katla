//! Geometry render pass template.
//!
//! Renders 3D geometry with color and depth outputs.

use std::collections::HashMap;

use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

/// Geometry render pass template.
///
/// Renders 3D geometry with optional depth pre-pass.
///
/// # Example
///
/// ```ignore
/// use katla_gfx::render_graph::GeometryPass;
/// use katla_gfx::texture::ImageFormat;
///
/// let geometry = GeometryPass::new("geometry")
///     .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///     .write_depth("depth", ImageFormat::D32Sfloat)
///     .clear_color([0.1, 0.1, 0.15, 1.0])
///     .clear_depth(1.0);
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
    /// Depth attachment output (optional).
    depth_output: Option<DepthOutput>,
    /// Resources read by this pass (e.g., shadow maps).
    reads: Vec<String>,
}

/// Describes a color attachment output.
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
    /// Clear value (used if load_op is Clear).
    clear_value: ClearValue,
}

/// Describes a depth attachment output.
#[derive(Debug, Clone)]
struct DepthOutput {
    /// Resource name.
    name: String,
    /// Image format.
    format: ImageFormat,
    /// Load operation.
    load_op: LoadOp,
    /// Store operation.
    store_op: StoreOp,
    /// Clear value (used if load_op is Clear).
    clear_value: ClearValue,
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
            depth_output: None,
            reads: Vec::new(),
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
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: ClearValue::OPAQUE_BLACK,
        });
        self
    }

    /// Add a color attachment output with custom load/store operations.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name for graph reference.
    /// * `format` - Image format for the color attachment.
    /// * `load_op` - How the attachment is loaded.
    /// * `store_op` - How the attachment is stored.
    /// * `clear_value` - Clear value if load_op is Clear.
    pub fn write_color_with(
        mut self,
        name: impl Into<String>,
        format: ImageFormat,
        load_op: LoadOp,
        store_op: StoreOp,
        clear_value: ClearValue,
    ) -> Self {
        self.color_outputs.push(ColorOutput {
            name: name.into(),
            format,
            load_op,
            store_op,
            clear_value,
        });
        self
    }

    /// Set the depth attachment output.
    ///
    /// By default, the depth attachment is cleared to 1.0 and stored.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name for graph reference.
    /// * `format` - Image format for the depth attachment (e.g., D32Sfloat).
    pub fn write_depth(mut self, name: impl Into<String>, format: ImageFormat) -> Self {
        self.depth_output = Some(DepthOutput {
            name: name.into(),
            format,
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: ClearValue::DEFAULT_DEPTH,
        });
        self
    }

    /// Set the depth attachment output with custom load/store operations.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name for graph reference.
    /// * `format` - Image format for the depth attachment.
    /// * `load_op` - How the attachment is loaded.
    /// * `store_op` - How the attachment is stored.
    /// * `clear_value` - Clear value if load_op is Clear.
    pub fn write_depth_with(
        mut self,
        name: impl Into<String>,
        format: ImageFormat,
        load_op: LoadOp,
        store_op: StoreOp,
        clear_value: ClearValue,
    ) -> Self {
        self.depth_output = Some(DepthOutput {
            name: name.into(),
            format,
            load_op,
            store_op,
            clear_value,
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
            output.clear_value = ClearValue::Color(color);
        }
        self
    }

    /// Set clear depth value for the depth attachment.
    ///
    /// Only applies if the load operation is Clear.
    ///
    /// # Arguments
    ///
    /// * `depth` - Depth clear value (0.0 - 1.0, typically 1.0).
    pub fn clear_depth(mut self, depth: f32) -> Self {
        if let Some(output) = self.depth_output.as_mut() {
            output.clear_value = ClearValue::DepthStencil { depth, stencil: 0 };
        }
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

    /// Check if a depth output is configured.
    pub fn has_depth_output(&self) -> bool {
        self.depth_output.is_some()
    }

    /// Get the read dependencies.
    pub fn reads(&self) -> &[String] {
        &self.reads
    }
}

/// Internal data for a geometry pass after name resolution.
#[derive(Debug)]
pub(crate) struct GeometryPassData {
    /// Color attachments with resolved handles.
    pub(crate) colors: Vec<(
        GraphResourceHandle,
        ImageFormat,
        LoadOp,
        StoreOp,
        ClearValue,
    )>,
    /// Depth attachment with resolved handle (optional).
    pub(crate) depth: Option<(
        GraphResourceHandle,
        ImageFormat,
        LoadOp,
        StoreOp,
        ClearValue,
    )>,
    /// Read dependencies with resolved handles.
    pub(crate) reads: Vec<GraphResourceHandle>,
}

impl PassBuilder for GeometryPass {
    fn as_builder(self) -> InternalPassBuilder {
        // Collect write resource names
        let writes: Vec<String> = self
            .color_outputs
            .iter()
            .map(|o| o.name.clone())
            .chain(self.depth_output.iter().map(|o| o.name.clone()))
            .collect();

        // Clone reads for the builder
        let reads = self.reads.clone();

        // Store pass data for the build function
        let color_outputs = self.color_outputs;
        let depth_output = self.depth_output;

        InternalPassBuilder {
            name: self.name,
            reads,
            writes,
            build_fn: Box::new(move |resource_map: &HashMap<String, GraphResourceHandle>| {
                // Resolve color output names to handles
                let colors: Vec<(
                    GraphResourceHandle,
                    ImageFormat,
                    LoadOp,
                    StoreOp,
                    ClearValue,
                )> = color_outputs
                    .iter()
                    .map(|output| {
                        let handle = resource_map.get(&output.name).copied().ok_or_else(|| {
                            RenderGraphError::ResourceNotFound(output.name.clone())
                        })?;
                        Ok((
                            handle,
                            output.format,
                            output.load_op,
                            output.store_op,
                            output.clear_value,
                        ))
                    })
                    .collect::<Result<Vec<_>, RenderGraphError>>()?;

                // Resolve depth output name to handle
                let depth = if let Some(output) = depth_output {
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

                Ok(Box::new(GeometryPassData {
                    colors,
                    depth,
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
    fn test_geometry_pass_new() {
        let pass = GeometryPass::new("test_geometry");
        assert_eq!(pass.name(), "test_geometry");
        assert_eq!(pass.color_output_count(), 0);
        assert!(!pass.has_depth_output());
        assert!(pass.reads().is_empty());
    }

    #[test]
    fn test_geometry_pass_write_color() {
        let pass = GeometryPass::new("test").write_color("color", ImageFormat::R16G16B16A16Sfloat);

        assert_eq!(pass.color_output_count(), 1);
        assert!(!pass.has_depth_output());
    }

    #[test]
    fn test_geometry_pass_write_multiple_colors() {
        let pass = GeometryPass::new("test")
            .write_color("color0", ImageFormat::R16G16B16A16Sfloat)
            .write_color("color1", ImageFormat::R8G8B8A8Srgb);

        assert_eq!(pass.color_output_count(), 2);
    }

    #[test]
    fn test_geometry_pass_write_depth() {
        let pass = GeometryPass::new("test").write_depth("depth", ImageFormat::D32Sfloat);

        assert_eq!(pass.color_output_count(), 0);
        assert!(pass.has_depth_output());
    }

    #[test]
    fn test_geometry_pass_full_setup() {
        let pass = GeometryPass::new("geometry")
            .write_color("color", ImageFormat::R16G16B16A16Sfloat)
            .write_depth("depth", ImageFormat::D32Sfloat)
            .read("shadow_map")
            .read("previous_frame")
            .clear_color([0.1, 0.2, 0.3, 1.0])
            .clear_depth(1.0);

        assert_eq!(pass.name(), "geometry");
        assert_eq!(pass.color_output_count(), 1);
        assert!(pass.has_depth_output());
        assert_eq!(pass.reads().len(), 2);
        assert_eq!(pass.reads()[0], "shadow_map");
        assert_eq!(pass.reads()[1], "previous_frame");
    }

    #[test]
    fn test_geometry_pass_custom_load_store() {
        let pass = GeometryPass::new("test")
            .write_color_with(
                "color",
                ImageFormat::R16G16B16A16Sfloat,
                LoadOp::Load,
                StoreOp::DontCare,
                ClearValue::OPAQUE_BLACK,
            )
            .write_depth_with(
                "depth",
                ImageFormat::D32Sfloat,
                LoadOp::Load,
                StoreOp::Store,
                ClearValue::DEFAULT_DEPTH,
            );

        assert_eq!(pass.color_output_count(), 1);
        assert!(pass.has_depth_output());
    }

    #[test]
    fn test_geometry_pass_builder_trait() {
        let pass = GeometryPass::new("geometry")
            .write_color("color", ImageFormat::R16G16B16A16Sfloat)
            .write_depth("depth", ImageFormat::D32Sfloat)
            .read("shadow_map");

        let builder = pass.as_builder();

        assert_eq!(builder.name, "geometry");
        assert_eq!(builder.reads, vec!["shadow_map"]);
        assert_eq!(builder.writes, vec!["color", "depth"]);
    }

    #[test]
    fn test_geometry_pass_build_fn_resolution() {
        let pass = GeometryPass::new("geometry")
            .write_color("color", ImageFormat::R16G16B16A16Sfloat)
            .write_depth("depth", ImageFormat::D32Sfloat)
            .read("shadow_map");

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("color".to_string(), GraphResourceHandle::new(0));
        resource_map.insert("depth".to_string(), GraphResourceHandle::new(1));
        resource_map.insert("shadow_map".to_string(), GraphResourceHandle::new(2));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());

        let data = result.unwrap();
        let pass_data = data.downcast_ref::<GeometryPassData>().unwrap();

        assert_eq!(pass_data.colors.len(), 1);
        assert!(pass_data.depth.is_some());
        assert_eq!(pass_data.reads.len(), 1);
    }

    #[test]
    fn test_geometry_pass_build_fn_missing_resource() {
        let pass =
            GeometryPass::new("geometry").write_color("color", ImageFormat::R16G16B16A16Sfloat);

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
