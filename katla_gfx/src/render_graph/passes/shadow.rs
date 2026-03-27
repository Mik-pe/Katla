//! Shadow mapping pass template for directional (CSM) shadow mapping.

use std::collections::HashMap;

use crate::texture::ImageFormat;

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::{PassKind, PassType};
use super::super::resource::GraphResourceHandle;

/// Shadow mapping pass template for directional light cascaded shadow maps.
///
/// # Example
///
/// ```ignore
/// let shadows = ShadowPass::new("shadows")
///     .write_depth("shadow_atlas", ImageFormat::D32Sfloat)
///     .resolution(4096, 4096);
///
/// let graph = FrameGraph::builder()
///     .add_pass(shadows)
///     .add_pass(GeometryPass::new("geometry")
///         .read("shadow_atlas")
///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///         .write_depth("depth", ImageFormat::D32Sfloat))
///     .build(&renderer)?;
/// ```
pub struct ShadowPass {
    name: String,
    depth_output: Option<(String, ImageFormat)>,
    resolution: (u32, u32),
}

impl ShadowPass {
    /// Create a new shadow pass.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            depth_output: None,
            resolution: (4096, 4096),
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
            build_fn: Box::new(
                move |_resource_map: &HashMap<String, GraphResourceHandle>| Ok(Box::new(())),
            ),
            uses_depth: true,
            depth_attachment: None,
            kind: Some(PassKind::Shadow),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_pass_default_resolution() {
        let pass = ShadowPass::new("shadows");
        assert_eq!(pass.resolution, (4096, 4096));
    }

    #[test]
    fn test_shadow_pass_as_builder_reads_writes() {
        let pass = ShadowPass::new("shadows")
            .write_depth("shadow_map", ImageFormat::D32Sfloat)
            .resolution(2048, 2048);

        let builder = pass.as_builder();

        assert_eq!(builder.name, "shadows");
        assert!(builder.reads.is_empty());
        assert_eq!(builder.writes, vec!["shadow_map"]);
    }
}
