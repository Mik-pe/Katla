//! Viewport render pass template for multi-viewport rendering.
//!
//! This module provides a pass template for rendering viewports to transient
//! textures that can be composited together in a CompositePass.

use std::collections::HashMap;

use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::pass::PassType;
use crate::render_graph::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

/// Viewport render pass template.
///
/// Renders a viewport to a transient texture. Multiple viewport passes can
/// be added to the same frame graph, each with independent camera uniforms
/// and draw lists.
///
/// # Features
///
/// - Writes to transient texture (e.g., "viewport_0", "viewport_1")
/// - Supports depth buffer (uses global depth buffer)
/// - Per-viewport camera uniforms and draw list
/// - Multiple viewports can coexist in same frame graph
///
/// # Example
///
/// ```ignore
/// use katla_gfx::render_graph::ViewportPass;
///
/// // Create two viewports for split-screen rendering
/// let left_viewport = ViewportPass::new("viewport_0")
///     .extent(960, 1080)
///     .format(ImageFormat::R16G16B16A16Sfloat)
///     .clear_color([0.1, 0.1, 0.15, 1.0]);
///
/// let right_viewport = ViewportPass::new("viewport_1")
///     .extent(960, 1080)
///     .format(ImageFormat::R16G16B16A16Sfloat)
///     .clear_color([0.15, 0.1, 0.1, 1.0]);
///
/// // Build frame graph with both viewports
/// let graph = FrameGraph::builder()
///     .create_resource(left_viewport.resource_desc().unwrap())
///     .create_resource(right_viewport.resource_desc().unwrap())
///     .add_pass(left_viewport)
///     .add_pass(right_viewport)
///     .build(&renderer)?;
/// ```
#[derive(Debug, Clone)]
pub struct ViewportPass {
    /// Pass name for debugging and referencing.
    name: String,
    /// Viewport extent (width, height).
    extent: Option<(u32, u32)>,
    /// Color attachment format.
    format: Option<ImageFormat>,
    /// Clear color for the viewport.
    clear_color: Option<[f32; 4]>,
    /// Load operation for color attachment.
    load_op: LoadOp,
    /// Store operation for color attachment.
    store_op: StoreOp,
    /// Resources read by this pass (e.g., shadow maps).
    reads: Vec<String>,
    /// Material handle for this pass (optional).
    material: Option<crate::handle::MaterialHandle>,
}

impl ViewportPass {
    /// Create a new viewport pass.
    ///
    /// # Arguments
    ///
    /// * `name` - Pass name for debugging and execution context reference.
    ///           This name is also used as the output texture name (e.g., "viewport_0").
    ///
    /// # Example
    /// ```ignore
    /// let viewport = ViewportPass::new("viewport_0");
    /// ```
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            extent: None,
            format: None,
            clear_color: None,
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            reads: Vec::new(),
            material: None,
        }
    }

    /// Set the viewport extent in pixels.
    ///
    /// This determines the resolution of the transient texture.
    ///
    /// # Arguments
    ///
    /// * `width` - Width in pixels.
    /// * `height` - Height in pixels.
    ///
    /// # Example
    /// ```ignore
    /// let viewport = ViewportPass::new("viewport_0")
    ///     .extent(960, 1080);  // Split-screen left half
    /// ```
    pub fn extent(mut self, width: u32, height: u32) -> Self {
        self.extent = Some((width, height));
        self
    }

    /// Set the color attachment format.
    ///
    /// Defaults to `ImageFormat::R16G16B16A16Sfloat` (HDR) if not set.
    ///
    /// # Arguments
    ///
    /// * `format` - Image format for the color attachment.
    ///
    /// # Example
    /// ```ignore
    /// let viewport = ViewportPass::new("viewport_0")
    ///     .format(ImageFormat::R8G8B8A8Srgb);  // LDR output
    /// ```
    pub fn format(mut self, format: ImageFormat) -> Self {
        self.format = Some(format);
        self
    }

    /// Set the clear color.
    ///
    /// Only applies if load operation is Clear (default).
    ///
    /// # Arguments
    ///
    /// * `color` - RGBA clear color (values 0.0 - 1.0).
    ///
    /// # Example
    /// ```ignore
    /// let viewport = ViewportPass::new("viewport_0")
    ///     .clear_color([0.1, 0.2, 0.3, 1.0]);
    /// ```
    pub fn clear_color(mut self, color: [f32; 4]) -> Self {
        self.clear_color = Some(color);
        self
    }

    /// Set custom load/store operations for the color attachment.
    ///
    /// Default is LoadOp::Clear, StoreOp::Store.
    ///
    /// # Arguments
    ///
    /// * `load_op` - How the attachment is loaded.
    /// * `store_op` - How the attachment is stored.
    ///
    /// # Example
    /// ```ignore
    /// let viewport = ViewportPass::new("viewport_0")
    ///     .load_store_ops(LoadOp::Load, StoreOp::Store);
    /// ```
    pub fn load_store_ops(mut self, load_op: LoadOp, store_op: StoreOp) -> Self {
        self.load_op = load_op;
        self.store_op = store_op;
        self
    }

    /// Read from a resource (e.g., shadow map, previous frame).
    ///
    /// Can be called multiple times to add multiple read dependencies.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name to read from.
    ///
    /// # Example
    /// ```ignore
    /// let viewport = ViewportPass::new("viewport_0")
    ///     .read("shadow_map")
    ///     .read("environment_map");
    /// ```
    pub fn read(mut self, name: impl Into<String>) -> Self {
        self.reads.push(name.into());
        self
    }

    /// Set the material for this pass.
    ///
    /// If the material was created with `ImageFormat::Auto`, it will be
    /// compiled on-demand for the format specified in `format()`.
    ///
    /// # Arguments
    ///
    /// * `material` - Material handle to use for this pass.
    ///
    /// # Example
    /// ```ignore
    /// let viewport = ViewportPass::new("viewport_0")
    ///     .material(geometry_material);
    /// ```
    pub fn material(mut self, material: crate::handle::MaterialHandle) -> Self {
        self.material = Some(material);
        self
    }

    /// Get the resource descriptor for this viewport's transient texture.
    ///
    /// Returns `None` if extent or format is not set.
    ///
    /// This descriptor must be added to the frame graph builder via
    /// `FrameGraphBuilder::create_resource()` before building the graph.
    ///
    /// # Example
    /// ```ignore
    /// let viewport = ViewportPass::new("viewport_0")
    ///     .extent(960, 1080)
    ///     .format(ImageFormat::R16G16B16A16Sfloat);
    ///
    /// let resource_desc = viewport.resource_desc().unwrap();
    ///
    /// let graph = FrameGraph::builder()
    ///     .create_resource(resource_desc)
    ///     .add_pass(viewport)
    ///     .build(&renderer)?;
    /// ```
    pub fn resource_desc(&self) -> Option<crate::render_graph::resource::GraphResourceDesc> {
        let (width, height) = self.extent?;
        let format = self.format?;

        Some(crate::render_graph::resource::GraphResourceDesc {
            name: self.name.clone(),
            resource_type: crate::render_graph::resource::GraphResourceType::ColorAttachment {
                clear_value: self.clear_color,
            },
            format,
            width,
            height,
        })
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

/// Internal data for a viewport pass after name resolution.
#[derive(Debug)]
pub(crate) struct ViewportPassData {
    /// Color attachment with resolved handle.
    #[allow(dead_code)]
    pub(crate) color: (
        GraphResourceHandle,
        ImageFormat,
        LoadOp,
        StoreOp,
        ClearValue,
    ),
}

impl PassBuilder for ViewportPass {
    fn as_builder(self) -> InternalPassBuilder {
        // Write to the transient texture (named after the pass)
        let writes = vec![self.name.clone()];

        // Clone reads for the builder
        let reads = self.reads.clone();

        // Store pass data for the build function
        let name = self.name.clone();
        let format = self.format;
        let clear_color = self.clear_color;
        let load_op = self.load_op;
        let store_op = self.store_op;
        let material = self.material;

        // Use the specified format for material compilation
        let output_format = format;

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
                // Resolve color output name to handle
                let handle = resource_map.get(&name).copied().ok_or_else(|| {
                    RenderGraphError::ResourceNotFound(format!(
                        "Viewport texture '{}' not found in resource map. Did you forget to call create_resource()?",
                        name
                    ))
                })?;

                // Determine format (default to HDR if not specified)
                let format = format.unwrap_or(ImageFormat::R16G16B16A16Sfloat);

                // Determine clear value
                let clear_value = match (load_op, clear_color) {
                    (LoadOp::Clear, Some(color)) => ClearValue::Color(color),
                    (LoadOp::Clear, None) => ClearValue::OPAQUE_BLACK,
                    (LoadOp::Load | LoadOp::DontCare, _) => ClearValue::OPAQUE_BLACK, // Not used when loading
                };

                Ok(Box::new(ViewportPassData {
                    color: (handle, format, load_op, store_op, clear_value),
                }))
            }),
            uses_depth: true, // Viewports use the global depth buffer
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_pass_new() {
        let pass = ViewportPass::new("viewport_0");
        assert_eq!(pass.name(), "viewport_0");
        assert!(pass.extent.is_none());
        assert!(pass.format.is_none());
        assert!(pass.clear_color.is_none());
        assert!(pass.reads().is_empty());
    }

    #[test]
    fn test_viewport_pass_full_setup() {
        let pass = ViewportPass::new("viewport_0")
            .extent(960, 1080)
            .format(ImageFormat::R16G16B16A16Sfloat)
            .clear_color([0.1, 0.2, 0.3, 1.0])
            .read("shadow_map")
            .read("environment_map");

        assert_eq!(pass.name(), "viewport_0");
        assert_eq!(pass.extent, Some((960, 1080)));
        assert_eq!(pass.format, Some(ImageFormat::R16G16B16A16Sfloat));
        assert_eq!(pass.clear_color, Some([0.1, 0.2, 0.3, 1.0]));
        assert_eq!(pass.reads().len(), 2);
        assert_eq!(pass.reads()[0], "shadow_map");
        assert_eq!(pass.reads()[1], "environment_map");
    }

    #[test]
    fn test_viewport_pass_resource_desc() {
        let pass = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R8G8B8A8Srgb)
            .clear_color([0.5, 0.5, 0.5, 1.0]);

        let desc = pass.resource_desc();
        assert!(desc.is_some());

        let desc = desc.unwrap();
        assert_eq!(desc.name, "viewport_0");
        assert_eq!(desc.width, 512);
        assert_eq!(desc.height, 512);
        assert_eq!(desc.format, ImageFormat::R8G8B8A8Srgb);

        match desc.resource_type {
            crate::render_graph::resource::GraphResourceType::ColorAttachment { clear_value } => {
                assert_eq!(clear_value, Some([0.5, 0.5, 0.5, 1.0]));
            }
            _ => panic!("Expected ColorAttachment resource type"),
        }
    }

    #[test]
    fn test_viewport_pass_resource_desc_missing_extent() {
        let pass = ViewportPass::new("viewport_0").format(ImageFormat::R8G8B8A8Srgb);

        assert!(pass.resource_desc().is_none());
    }

    #[test]
    fn test_viewport_pass_resource_desc_missing_format() {
        let pass = ViewportPass::new("viewport_0").extent(512, 512);

        assert!(pass.resource_desc().is_none());
    }

    #[test]
    fn test_viewport_pass_load_store_ops() {
        let pass = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R16G16B16A16Sfloat)
            .load_store_ops(LoadOp::Load, StoreOp::DontCare);

        assert_eq!(pass.load_op, LoadOp::Load);
        assert_eq!(pass.store_op, StoreOp::DontCare);
    }

    #[test]
    fn test_viewport_pass_material() {
        let material = crate::handle::MaterialHandle::new(42);
        let pass = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R16G16B16A16Sfloat)
            .material(material);

        assert!(pass.material.is_some());
        assert_eq!(pass.material.unwrap().index(), 42);
    }

    #[test]
    fn test_viewport_pass_builder_trait() {
        let pass = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R16G16B16A16Sfloat)
            .read("shadow_map");

        let builder = pass.as_builder();

        assert_eq!(builder.name, "viewport_0");
        assert_eq!(builder.pass_type, PassType::Graphics);
        assert_eq!(builder.reads, vec!["shadow_map"]);
        assert_eq!(builder.writes, vec!["viewport_0"]);
        assert!(builder.uses_depth);
    }

    #[test]
    fn test_viewport_pass_builder_trait_multiple_viewports() {
        let pass0 = ViewportPass::new("viewport_0")
            .extent(960, 1080)
            .format(ImageFormat::R16G16B16A16Sfloat);

        let pass1 = ViewportPass::new("viewport_1")
            .extent(960, 1080)
            .format(ImageFormat::R16G16B16A16Sfloat);

        let builder0 = pass0.as_builder();
        let builder1 = pass1.as_builder();

        assert_eq!(builder0.name, "viewport_0");
        assert_eq!(builder0.writes, vec!["viewport_0"]);

        assert_eq!(builder1.name, "viewport_1");
        assert_eq!(builder1.writes, vec!["viewport_1"]);
    }

    #[test]
    fn test_viewport_pass_build_fn_resolution() {
        let pass = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R16G16B16A16Sfloat)
            .clear_color([0.2, 0.3, 0.4, 1.0]);

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("viewport_0".to_string(), GraphResourceHandle::new(0));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());

        let data = result.unwrap();
        let pass_data = data.downcast_ref::<ViewportPassData>().unwrap();

        assert_eq!(pass_data.color.0.index(), 0);
        assert_eq!(pass_data.color.1, ImageFormat::R16G16B16A16Sfloat);
        assert_eq!(pass_data.color.2, LoadOp::Clear);
        assert_eq!(pass_data.color.3, StoreOp::Store);

        match pass_data.color.4 {
            ClearValue::Color(color) => assert_eq!(color, [0.2, 0.3, 0.4, 1.0]),
            _ => panic!("Expected Color clear value"),
        }
    }

    #[test]
    fn test_viewport_pass_build_fn_missing_resource() {
        let pass = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R16G16B16A16Sfloat);

        let builder = pass.as_builder();

        let resource_map = HashMap::new(); // Empty - no resources

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_err());

        match result {
            Err(RenderGraphError::ResourceNotFound(msg)) => {
                assert!(msg.contains("viewport_0"));
                assert!(msg.contains("create_resource"));
            }
            _ => panic!("Expected ResourceNotFound error"),
        }
    }

    #[test]
    fn test_viewport_pass_default_format() {
        let pass = ViewportPass::new("viewport_0").extent(512, 512);

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("viewport_0".to_string(), GraphResourceHandle::new(0));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());

        let data = result.unwrap();
        let pass_data = data.downcast_ref::<ViewportPassData>().unwrap();

        // Should default to HDR format
        assert_eq!(pass_data.color.1, ImageFormat::R16G16B16A16Sfloat);
    }

    #[test]
    fn test_multiple_viewport_passes_unique_names() {
        let left = ViewportPass::new("viewport_0")
            .extent(960, 1080)
            .format(ImageFormat::R16G16B16A16Sfloat);

        let right = ViewportPass::new("viewport_1")
            .extent(960, 1080)
            .format(ImageFormat::R16G16B16A16Sfloat);

        // Each viewport should have unique resource descriptors
        let left_desc = left.resource_desc().unwrap();
        let right_desc = right.resource_desc().unwrap();

        assert_eq!(left_desc.name, "viewport_0");
        assert_eq!(right_desc.name, "viewport_1");

        // Both should write to different resources
        let left_builder = left.as_builder();
        let right_builder = right.as_builder();

        assert_ne!(left_builder.writes, right_builder.writes);
    }
}
