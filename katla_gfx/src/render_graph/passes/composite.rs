//! Compositing pass template for multi-viewport rendering.
//!
//! This module provides a pass template for compositing multiple viewport
//! textures onto a final output. Supports up to 8 simultaneous viewports
//! with configurable positioning via viewport rectangles.

use std::collections::HashMap;

use crate::handle::MaterialHandle;
use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame_graph::BACKBUFFER_NAME;
use crate::render_graph::pass::{PassKind, PassType};
use crate::render_graph::resource::GraphResourceHandle;

/// Viewport rectangle for positioning viewport outputs on screen.
///
/// Represents a rectangle in screen space where a viewport will be displayed.
/// Uses min/max representation for efficient shader access (pre-computed
/// x+width and y+height).
///
/// # Example
///
/// ```ignore
/// // Fullscreen viewport (1920x1080)
/// let rect = ViewportRect::new(0.0, 0.0, 1920.0, 1080.0);
///
/// // Split-screen left (960x1080)
/// let left = ViewportRect::new(0.0, 0.0, 960.0, 1080.0);
///
/// // Split-screen right (960x1080, starting at x=960)
/// let right = ViewportRect::new(960.0, 0.0, 1920.0, 1080.0);
///
/// // Picture-in-picture (300x200, in top-right corner)
/// let pip = ViewportRect::new(1620.0, 880.0, 1920.0, 1080.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportRect {
    /// Left edge in pixels (x)
    pub x: f32,
    /// Top edge in pixels (y)
    pub y: f32,
    /// Right edge in pixels (x + width)
    pub z: f32,
    /// Bottom edge in pixels (y + height)
    pub w: f32,
}

impl ViewportRect {
    /// Create a new viewport rectangle from min/max coordinates.
    ///
    /// # Arguments
    /// * `x` - Left edge in pixels
    /// * `y` - Top edge in pixels
    /// * `z` - Right edge in pixels (x + width)
    /// * `w` - Bottom edge in pixels (y + height)
    ///
    /// # Example
    /// ```ignore
    /// // Fullscreen (1920x1080)
    /// let rect = ViewportRect::new(0.0, 0.0, 1920.0, 1080.0);
    ///
    /// // Split-screen left half
    /// let left = ViewportRect::new(0.0, 0.0, 960.0, 1080.0);
    /// ```
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Create a viewport rectangle from origin and size.
    ///
    /// # Arguments
    /// * `origin_x` - Left edge in pixels
    /// * `origin_y` - Top edge in pixels
    /// * `width` - Width in pixels
    /// * `height` - Height in pixels
    ///
    /// # Example
    /// ```ignore
    /// // Fullscreen from origin and size
    /// let rect = ViewportRect::from_origin_size(0.0, 0.0, 1920.0, 1080.0);
    /// ```
    pub fn from_origin_size(origin_x: f32, origin_y: f32, width: f32, height: f32) -> Self {
        Self {
            x: origin_x,
            y: origin_y,
            z: origin_x + width,
            w: origin_y + height,
        }
    }

    /// Convert to shader-ready array [x, y, z, w].
    ///
    /// This format is optimized for shader access with pre-computed
    /// right and bottom edges.
    ///
    /// # Example
    /// ```ignore
    /// let rect = ViewportRect::new(0.0, 0.0, 960.0, 1080.0);
    /// let shader_array = rect.to_array(); // [0.0, 0.0, 960.0, 1080.0]
    /// ```
    pub fn to_array(&self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }

    /// Get the width of the viewport rectangle.
    pub fn width(&self) -> f32 {
        self.z - self.x
    }

    /// Get the height of the viewport rectangle.
    pub fn height(&self) -> f32 {
        self.w - self.y
    }
}

/// Compositing pass template for multi-viewport rendering.
///
/// Composites multiple viewport textures onto a final output target.
/// Each viewport has a texture resource and a rectangle specifying where
/// it should be positioned on screen.
///
/// # Features
///
/// - Up to 8 simultaneous viewports (limited by CompositingDescriptorSet)
/// - Per-viewport positioning via rectangles
/// - Alpha blending for overlapping viewports
/// - Writes to backbuffer or transient texture
///
/// # Example
///
/// ```ignore
/// use katla_gfx::render_graph::CompositePass;
///
/// // Split-screen layout with 2 viewports
/// let composite = CompositePass::new("composite")
///     .viewport("viewport_0", ViewportRect::from_origin_size(0.0, 0.0, 960.0, 1080.0))
///     .viewport("viewport_1", ViewportRect::from_origin_size(960.0, 0.0, 960.0, 1080.0))
///     .write_backbuffer()
///     .material(compositing_material);
///
/// let graph = FrameGraph::builder()
///     .add_pass(composite)
///     .build(&renderer)?;
/// ```
///
/// # Shader Requirements
///
/// The compositing pass requires a shader that:
/// - Samples from viewportTextures array (set 2, binding 0)
/// - Uses viewport rectangles for positioning
/// - Implements alpha blending for overlapping viewports
///
/// See `resources/shaders/composite.wgsl` for the reference implementation.
pub struct CompositePass {
    /// Pass name for debugging and referencing.
    name: String,
    /// Viewport inputs: (texture name, rectangle on screen).
    viewports: Vec<(String, ViewportRect)>,
    /// Output resource name (backbuffer or transient texture).
    output: Option<String>,
    /// Material handle for the compositing shader.
    material: Option<MaterialHandle>,
}

impl CompositePass {
    /// Create a new compositing pass.
    ///
    /// # Arguments
    /// * `name` - Pass name for debugging and execution context reference.
    ///
    /// # Example
    /// ```ignore
    /// let composite = CompositePass::new("composite");
    /// ```
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            viewports: Vec::new(),
            output: None,
            material: None,
        }
    }

    /// Add a viewport input with positioning rectangle.
    ///
    /// Can be called multiple times to add multiple viewports (max 8).
    /// Viewports are composited in order, with later viewports on top
    /// (higher depth).
    ///
    /// # Arguments
    /// * `texture_name` - Name of the viewport texture resource to read from.
    /// * `rect` - Rectangle on screen where this viewport should be displayed.
    ///
    /// # Example
    /// ```ignore
    /// let composite = CompositePass::new("composite")
    ///     .viewport("viewport_0", ViewportRect::from_origin_size(0.0, 0.0, 960.0, 1080.0))
    ///     .viewport("viewport_1", ViewportRect::from_origin_size(960.0, 0.0, 960.0, 1080.0));
    /// ```
    pub fn viewport(mut self, texture_name: impl Into<String>, rect: ViewportRect) -> Self {
        self.viewports.push((texture_name.into(), rect));
        self
    }

    /// Set the output target (backbuffer or transient texture).
    ///
    /// If not called, the pass must write to the backbuffer by default.
    ///
    /// # Arguments
    /// * `target` - Name of the output resource.
    ///
    /// # Example
    /// ```ignore
    /// let composite = CompositePass::new("composite")
    ///     .viewport("viewport_0", ViewportRect::fullscreen())
    ///     .write("intermediate_output")
    ///     .material(material);
    /// ```
    pub fn write(mut self, target: impl Into<String>) -> Self {
        self.output = Some(target.into());
        self
    }

    /// Write directly to the backbuffer (swapchain).
    ///
    /// This is the final output that presents to the screen. This is the
    /// most common output target for compositing passes.
    ///
    /// # Example
    /// ```ignore
    /// let composite = CompositePass::new("composite")
    ///     .viewport("viewport_0", ViewportRect::fullscreen())
    ///     .write_backbuffer()
    ///     .material(material);
    /// ```
    pub fn write_backbuffer(self) -> Self {
        self.write(BACKBUFFER_NAME)
    }

    /// Set the material for the compositing shader.
    ///
    /// The material must use a shader that samples from viewportTextures
    /// (set 2, binding 0) and implements the compositing logic.
    ///
    /// # Arguments
    /// * `material` - Material handle to use for this pass.
    ///
    /// # Example
    /// ```ignore
    /// let composite = CompositePass::new("composite")
    ///     .viewport("viewport_0", ViewportRect::fullscreen())
    ///     .write_backbuffer()
    ///     .material(compositing_material);
    /// ```
    pub fn material(mut self, material: MaterialHandle) -> Self {
        self.material = Some(material);
        self
    }
}

/// Internal data for a compositing pass after name resolution.
#[derive(Debug)]
pub struct CompositePassData {
    /// Viewport textures with resolved handles and rectangles.
    pub viewports: Vec<(GraphResourceHandle, ViewportRect)>,
}

impl PassBuilder for CompositePass {
    fn as_builder(self) -> InternalPassBuilder {
        // Collect read dependencies (viewport textures)
        let reads: Vec<String> = self
            .viewports
            .iter()
            .map(|(name, _)| name.clone())
            .collect();

        // Determine write target (backbuffer if not specified)
        let writes = match self.output {
            Some(target) => vec![target],
            None => vec![BACKBUFFER_NAME.to_string()],
        };

        // Store pass data for the build function
        let viewports = self.viewports;
        let material = self.material;

        // Output format for material compilation (backbuffer is sRGB)
        let output_format = Some(crate::texture::ImageFormat::B8G8R8A8Srgb);

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads,
            writes,
            pipeline: None,
            tonemap_params: None,
            overlay_params: None,
            material,
            output_format,
            build_fn: Box::new(move |resource_map: &HashMap<String, GraphResourceHandle>| {
                // Resolve viewport texture names to handles
                let viewports: Vec<(GraphResourceHandle, ViewportRect)> = viewports
                    .iter()
                    .map(|(name, rect)| {
                        let handle = resource_map
                            .get(name)
                            .copied()
                            .ok_or_else(|| RenderGraphError::ResourceNotFound(name.clone()))?;
                        Ok((handle, *rect))
                    })
                    .collect::<Result<Vec<_>, RenderGraphError>>()?;

                Ok(Box::new(CompositePassData { viewports }))
            }),
            uses_depth: false, // Compositing is a fullscreen pass, no depth needed
            depth_attachment: None,
            kind: Some(PassKind::Compositing),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_rect_from_origin_size() {
        let rect = ViewportRect::from_origin_size(100.0, 200.0, 300.0, 400.0);
        assert_eq!(rect.x, 100.0);
        assert_eq!(rect.y, 200.0);
        assert_eq!(rect.z, 400.0); // 100 + 300
        assert_eq!(rect.w, 600.0); // 200 + 400
    }

    #[test]
    fn test_viewport_rect_width_height() {
        let rect = ViewportRect::new(10.0, 20.0, 960.0, 1080.0);
        assert_eq!(rect.width(), 960.0 - 10.0);
        assert_eq!(rect.height(), 1080.0 - 20.0);
    }

    #[test]
    fn test_composite_pass_build_fn_resolution() {
        let rect = ViewportRect::new(0.0, 0.0, 960.0, 1080.0);
        let pass = CompositePass::new("composite")
            .viewport("viewport_0", rect)
            .write_backbuffer();

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("viewport_0".to_string(), GraphResourceHandle::new(0));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());

        let data = result.unwrap();
        let pass_data = data.downcast_ref::<CompositePassData>().unwrap();
        assert_eq!(pass_data.viewports.len(), 1);
        assert_eq!(pass_data.viewports[0].0.index(), 0);
        assert_eq!(pass_data.viewports[0].1, rect);
    }

    #[test]
    fn test_composite_pass_build_fn_missing_resource() {
        let rect = ViewportRect::new(0.0, 0.0, 960.0, 1080.0);
        let pass = CompositePass::new("composite")
            .viewport("viewport_0", rect)
            .write_backbuffer();

        let builder = pass.as_builder();
        let resource_map = HashMap::new();

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_err());

        match result {
            Err(RenderGraphError::ResourceNotFound(name)) => assert_eq!(name, "viewport_0"),
            _ => panic!("Expected ResourceNotFound error"),
        }
    }

    #[test]
    fn test_composite_pass_multiple_viewports() {
        let left = ViewportRect::new(0.0, 0.0, 960.0, 1080.0);
        let right = ViewportRect::new(960.0, 0.0, 1920.0, 1080.0);

        let pass = CompositePass::new("composite")
            .viewport("viewport_0", left)
            .viewport("viewport_1", right)
            .write_backbuffer();

        let builder = pass.as_builder();
        assert_eq!(builder.reads.len(), 2);
        assert_eq!(builder.reads[0], "viewport_0");
        assert_eq!(builder.reads[1], "viewport_1");
    }
}
