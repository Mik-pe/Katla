//! Geometry render pass template.
//!
//! Renders 3D geometry with color outputs. Depth is handled automatically
//! using the global depth buffer.

use std::collections::HashMap;

use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::pass::PassType;
use crate::render_graph::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
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
    depth_config: Option<(LoadOp, StoreOp, ClearValue)>,
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

    /// Configure depth attachment load/store operations.
    ///
    /// By default, depth is cleared to 0.0 (reverse-Z far plane).
    /// Use `depth_load(LoadOp::Load, ...)` after a depth prepass to reuse depth.
    pub fn depth_config(
        mut self,
        load_op: LoadOp,
        store_op: StoreOp,
        clear_value: ClearValue,
    ) -> Self {
        self.depth_config = Some((load_op, store_op, clear_value));
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
}

impl PassBuilder for GeometryPass {
    fn as_builder(self) -> InternalPassBuilder {
        // Collect write resource names (color only - depth is implicit)
        let writes: Vec<String> = self.color_outputs.iter().map(|o| o.name.clone()).collect();

        // Clone reads for the builder
        let reads = self.reads.clone();

        // Store pass data for the build function
        let color_outputs = self.color_outputs;
        let material = self.material;
        let depth_config = self.depth_config;

        // Extract output format from first color attachment (for material format inference).
        //
        // Note: When using `ImageFormat::Auto` materials with multiple render targets (MRT),
        // only the first color attachment's format is used for compilation. Mixed-format MRT
        // is not supported with Auto materials - use explicit format materials for that case.
        let output_format = color_outputs.first().map(|o| o.format);

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads,
            writes,
            pipeline: None,
            tonemap_params: None,
            material,
            output_format,
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

                Ok(Box::new(GeometryPassData { colors }))
            }),
            uses_depth: true,
            depth_attachment: depth_config,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geometry_pass_build_fn_resolution() {
        let pass = GeometryPass::new("geometry")
            .write_color("color", ImageFormat::R16G16B16A16Sfloat)
            .read("shadow_map");

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("color".to_string(), GraphResourceHandle::new(0));
        resource_map.insert("shadow_map".to_string(), GraphResourceHandle::new(1));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());

        let data = result.unwrap();
        let pass_data = data.downcast_ref::<GeometryPassData>().unwrap();
        assert_eq!(pass_data.colors.len(), 1);
    }

    #[test]
    fn test_geometry_pass_build_fn_missing_resource() {
        let pass =
            GeometryPass::new("geometry").write_color("color", ImageFormat::R16G16B16A16Sfloat);

        let builder = pass.as_builder();
        let resource_map = HashMap::new();

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_err());

        match result {
            Err(RenderGraphError::ResourceNotFound(name)) => assert_eq!(name, "color"),
            _ => panic!("Expected ResourceNotFound error"),
        }
    }

    #[test]
    fn test_geometry_pass_multiple_color_outputs() {
        let pass = GeometryPass::new("geometry")
            .write_color("albedo", ImageFormat::R8G8B8A8Srgb)
            .write_color("normals", ImageFormat::R16G16B16A16Sfloat);

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("albedo".to_string(), GraphResourceHandle::new(0));
        resource_map.insert("normals".to_string(), GraphResourceHandle::new(1));

        let result = (builder.build_fn)(&resource_map).unwrap();
        let pass_data = result.downcast_ref::<GeometryPassData>().unwrap();
        assert_eq!(pass_data.colors.len(), 2);
    }

    #[test]
    fn test_geometry_pass_material_propagates() {
        let material = crate::handle::MaterialHandle::new(42);
        let pass = GeometryPass::new("test")
            .write_color("color", ImageFormat::R16G16B16A16Sfloat)
            .material(material);

        let builder = pass.as_builder();
        assert_eq!(builder.material, Some(material));
        assert_eq!(builder.output_format, Some(ImageFormat::R16G16B16A16Sfloat));
    }
}
