//! Shadow mapping pass template.
//!
//! Directional and point light shadow mapping.

use std::collections::HashMap;

use crate::texture::ImageFormat;

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::PassType;
use super::super::resource::GraphResourceHandle;

/// Type of light for shadow mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightType {
    /// Directional light (sun-like, parallel rays).
    Directional,
    /// Point light (omnidirectional, cube map shadows).
    Point,
    /// Spot light (cone-shaped).
    Spot,
}

/// Shadow mapping pass template.
///
/// Directional and point light shadow mapping.
///
/// # Example
///
/// ```ignore
/// let shadows = ShadowPass::new("shadows")
///     .write_depth("shadow_map", ImageFormat::D32Sfloat)
///     .resolution(2048, 2048)
///     .light_type(LightType::Directional);
///
/// let graph = FrameGraph::builder()
///     .add_pass(shadows)
///     .add_pass(GeometryPass::new("geometry")
///         .read("shadow_map")
///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///         .write_depth("depth", ImageFormat::D32Sfloat))
///     .build(&renderer)?;
///
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("shadows")
///         .light_direction([0.3, 1.0, 0.2])
///         .draw_list(&shadow_casters);
///
///     ctx.pass("geometry").draw_list(&main_geometry);
/// })?;
/// ```
pub struct ShadowPass {
    name: String,
    depth_output: Option<(String, ImageFormat)>,
    resolution: (u32, u32),
    light_type: LightType,
}

impl ShadowPass {
    /// Create a new shadow pass.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            depth_output: None,
            resolution: (1024, 1024),
            light_type: LightType::Directional,
        }
    }

    /// Set the depth output (shadow map).
    pub fn write_depth(mut self, name: impl Into<String>, format: ImageFormat) -> Self {
        self.depth_output = Some((name.into(), format));
        self
    }

    /// Set shadow map resolution.
    pub fn resolution(mut self, width: u32, height: u32) -> Self {
        self.resolution = (width, height);
        self
    }

    /// Set the light type.
    pub fn light_type(mut self, ty: LightType) -> Self {
        self.light_type = ty;
        self
    }
}

impl PassBuilder for ShadowPass {
    fn as_builder(self) -> InternalPassBuilder {
        let writes: Vec<String> = self.depth_output.iter().map(|(n, _)| n.clone()).collect();

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: Vec::new(),
            writes,
            pipeline: None,
            tonemap_params: None,
            material: None,
            output_format: None,
            build_fn: Box::new(move |_resource_map: &HashMap<String, GraphResourceHandle>| {
                // Shadow pass data is currently unused but kept for future extensibility
                Ok(Box::new(ShadowPassData))
            }),
        }
    }
}

/// Internal data for a shadow pass.
#[derive(Debug)]
pub(crate) struct ShadowPassData;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_pass_new() {
        let pass = ShadowPass::new("shadows");
        assert_eq!(pass.name, "shadows");
        assert!(pass.depth_output.is_none());
        assert_eq!(pass.resolution, (1024, 1024));
        assert_eq!(pass.light_type, LightType::Directional);
    }

    #[test]
    fn test_shadow_pass_write_depth() {
        let pass = ShadowPass::new("shadows").write_depth("shadow_map", ImageFormat::D32Sfloat);

        assert!(pass.depth_output.is_some());
        let (name, format) = pass.depth_output.unwrap();
        assert_eq!(name, "shadow_map");
        assert_eq!(format, ImageFormat::D32Sfloat);
    }

    #[test]
    fn test_shadow_pass_resolution() {
        let pass = ShadowPass::new("shadows").resolution(2048, 2048);
        assert_eq!(pass.resolution, (2048, 2048));
    }

    #[test]
    fn test_shadow_pass_light_type() {
        let pass = ShadowPass::new("shadows").light_type(LightType::Point);
        assert_eq!(pass.light_type, LightType::Point);

        let pass = ShadowPass::new("shadows").light_type(LightType::Spot);
        assert_eq!(pass.light_type, LightType::Spot);
    }

    #[test]
    fn test_shadow_pass_as_builder() {
        let pass = ShadowPass::new("shadows")
            .write_depth("shadow_map", ImageFormat::D32Sfloat)
            .resolution(2048, 2048)
            .light_type(LightType::Directional);

        let builder = pass.as_builder();

        assert_eq!(builder.name, "shadows");
        assert_eq!(builder.pass_type, PassType::Graphics);
        assert!(builder.reads.is_empty());
        assert_eq!(builder.writes, vec!["shadow_map"]);
    }

    #[test]
    fn test_shadow_pass_build_fn() {
        let pass = ShadowPass::new("shadows")
            .write_depth("shadow_map", ImageFormat::D32Sfloat)
            .resolution(2048, 2048)
            .light_type(LightType::Point);

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("shadow_map".to_string(), GraphResourceHandle::new(0));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());
    }

    #[test]
    fn test_shadow_pass_build_fn_missing_resource() {
        let pass = ShadowPass::new("shadows").write_depth("shadow_map", ImageFormat::D32Sfloat);

        let builder = pass.as_builder();

        let resource_map = HashMap::new();
        // Missing "shadow_map"

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_err());
    }

    #[test]
    fn test_light_type_variants() {
        assert_eq!(LightType::Directional, LightType::Directional);
        assert_ne!(LightType::Directional, LightType::Point);
        assert_ne!(LightType::Point, LightType::Spot);
    }
}
