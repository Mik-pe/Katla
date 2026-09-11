//! Viewport render pass template for multi-viewport rendering.
//!
//! This module provides a pass template for rendering viewports to transient
//! textures that can be composited together in a CompositePass.

use crate::render_graph::access::{
    ImageAccess, ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
};
use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::pass::{PassKind, PassType};
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
    ///   This name is also used as the output texture name (e.g., "viewport_0").
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
            tracks_swapchain_size: true,
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

impl PassBuilder for ViewportPass {
    fn as_builder(self) -> InternalPassBuilder {
        // Write to the transient texture (named after the pass)
        let writes = vec![self.name.clone()];

        // Loading the target preserves its previous contents.
        let mut reads = self.reads.clone();
        if self.load_op == LoadOp::Load && !reads.contains(&self.name) {
            reads.push(self.name.clone());
        }

        let material = self.material;

        // Use the specified format for material compilation
        let output_format = self.format;

        // Declared attachment ops for the viewport target.
        let clear_value = match (self.load_op, self.clear_color) {
            (LoadOp::Clear, Some(color)) => ClearValue::Color(color),
            (LoadOp::Clear, None) => ClearValue::OPAQUE_BLACK,
            (LoadOp::Load | LoadOp::DontCare, _) => ClearValue::OPAQUE_BLACK, // Not used when loading
        };
        let color_attachments = vec![(
            writes[0].clone(),
            crate::render_pass::AttachmentOps {
                load: self.load_op,
                store: self.store_op,
                clear_value,
            },
        )];

        // Hand-declared typed accesses: declared reads are sampled; the
        // viewport target is an attachment (read-write when its contents
        // are loaded).
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
            .chain(std::iter::once(super::named_image_access(
                self.name.clone(),
                if self.load_op == LoadOp::Load {
                    ImageAccessMode::ReadWrite
                } else {
                    ImageAccessMode::Write
                },
                ImageUsage::ColorAttachment,
                ImagePipelineStage::ColorAttachmentOutput,
                ImageSubresourceRange::WHOLE_COLOR,
            )))
            .collect();

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
            output_format,
            build_fn: Box::new(|_| Ok(Box::new(()))),
            uses_depth: true, // Viewports use the global depth buffer
            color_attachments,
            depth_attachment: None,
            kind: Some(PassKind::Geometry),
            side_effect: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn viewport_declares_ops_for_its_target() {
        let pass = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R16G16B16A16Sfloat)
            .clear_color([0.2, 0.3, 0.4, 1.0]);

        let builder = pass.as_builder();

        assert_eq!(builder.writes, vec!["viewport_0"]);
        let (name, ops) = &builder.color_attachments[0];
        assert_eq!(name, "viewport_0");
        assert_eq!(ops.load, LoadOp::Clear);
        assert_eq!(ops.store, StoreOp::Store);
        assert_eq!(ops.clear_value, ClearValue::Color([0.2, 0.3, 0.4, 1.0]));
    }

    #[test]
    fn viewport_load_declares_read_dependency() {
        let builder = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R16G16B16A16Sfloat)
            .load_store_ops(LoadOp::Load, StoreOp::Store)
            .as_builder();
        assert_eq!(builder.reads, vec!["viewport_0"]);
        assert_eq!(builder.writes, vec!["viewport_0"]);
        assert_eq!(builder.color_attachments[0].1.load, LoadOp::Load);
    }

    #[test]
    fn viewport_pass_declares_typed_accesses() {
        use crate::render_graph::access::{
            ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
            NamedImageAccess,
        };

        let cleared = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R16G16B16A16Sfloat)
            .as_builder();
        assert_eq!(
            cleared.image_accesses,
            vec![NamedImageAccess {
                resource: "viewport_0".to_string(),
                mode: ImageAccessMode::Write,
                usage: ImageUsage::ColorAttachment,
                stage: ImagePipelineStage::ColorAttachmentOutput,
                range: ImageSubresourceRange::WHOLE_COLOR,
            }]
        );

        let loaded = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R16G16B16A16Sfloat)
            .load_store_ops(LoadOp::Load, StoreOp::Store)
            .as_builder();
        assert_eq!(
            loaded.image_accesses,
            vec![NamedImageAccess {
                resource: "viewport_0".to_string(),
                mode: ImageAccessMode::ReadWrite,
                usage: ImageUsage::ColorAttachment,
                stage: ImagePipelineStage::ColorAttachmentOutput,
                range: ImageSubresourceRange::WHOLE_COLOR,
            }]
        );
    }

    #[test]
    fn test_viewport_pass_load_store_ops_propagate() {
        let pass = ViewportPass::new("viewport_0")
            .extent(512, 512)
            .format(ImageFormat::R16G16B16A16Sfloat)
            .load_store_ops(LoadOp::Load, StoreOp::DontCare);

        let builder = pass.as_builder();

        let (name, ops) = &builder.color_attachments[0];
        assert_eq!(name, "viewport_0");
        assert_eq!(ops.load, LoadOp::Load);
        assert_eq!(ops.store, StoreOp::DontCare);
    }
}
