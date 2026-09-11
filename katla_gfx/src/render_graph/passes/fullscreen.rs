//! Fullscreen/compute pass template.
//!
//! Post-processing, lighting, and compute-like work.

use std::collections::{HashMap, HashSet};

use crate::handle::PipelineHandle;
use crate::texture::ImageFormat;

use super::super::access::{
    ImageAccess, ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
};
use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::{PassKind, PassType};
use super::super::resource::GraphResourceHandle;
use crate::render_graph::BACKBUFFER_NAME;

/// Tonemapping operators for fullscreen post-processing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TonemapOperator {
    /// ACES Filmic - cinematic look with good highlight rolloff
    #[default]
    Aces = 0,
    /// Reinhard - simple, preserves colors well
    Reinhard = 1,
    /// TonyMcMapface - popular, good balance of contrast and highlights
    TonyMcMapface = 2,
    /// Linear - no tonemapping, just gamma correction
    Linear = 3,
}

/// Tonemap parameters for fullscreen passes.
#[derive(Clone, Copy, Debug)]
pub struct TonemapParams {
    /// HDR exposure multiplier
    pub exposure: f32,
    /// Gamma correction value (typically 2.2)
    pub gamma: f32,
    /// Tonemapping operator
    pub mode: TonemapOperator,
    /// Bindless texture index for HDR source (None = not a tonemap pass)
    pub hdr_texture_index: Option<u32>,
}

impl Default for TonemapParams {
    fn default() -> Self {
        Self {
            exposure: 1.0,
            gamma: 2.2,
            mode: TonemapOperator::Aces,
            hdr_texture_index: None,
        }
    }
}

/// Fullscreen/compute pass template.
///
/// Post-processing, lighting, and compute-like work.
///
/// # Example
///
/// ```ignore
/// let tone_map = FullscreenPass::new("tone_map")
///     .read("hdr_color")
///     .write("ldr_output", ImageFormat::R8G8B8A8Srgb)
///     .pipeline(tone_map_pipeline);
///
/// let graph = FrameGraph::builder()
///     .add_pass(tone_map)
///     .build(&renderer)?;
///
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("tone_map").dispatch();
/// })?;
/// ```
pub struct FullscreenPass {
    name: String,
    reads: Vec<String>,
    writes: Vec<(String, ImageFormat)>,
    pipeline: Option<PipelineHandle>,
    tonemap_params: Option<TonemapParams>,
}

impl FullscreenPass {
    /// Create a new fullscreen pass.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
            writes: Vec::new(),
            pipeline: None,
            tonemap_params: None,
        }
    }

    /// Read from a resource (can call multiple times).
    pub fn read(mut self, name: impl Into<String>) -> Self {
        self.reads.push(name.into());
        self
    }

    /// Write to an output resource.
    pub fn write(mut self, name: impl Into<String>, format: ImageFormat) -> Self {
        self.writes.push((name.into(), format));
        self
    }

    /// Write directly to the backbuffer (swapchain).
    ///
    /// This is the final output that presents to the screen.
    pub fn write_backbuffer(mut self) -> Self {
        self.writes
            .push((BACKBUFFER_NAME.to_string(), ImageFormat::B8G8R8A8Srgb));
        self
    }

    /// Set the graphics pipeline.
    pub fn pipeline(mut self, pipeline: PipelineHandle) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    /// Configure tonemap parameters for HDR->LDR conversion.
    ///
    /// When set, the pass will automatically configure object[0] in the storage buffer
    /// with these parameters. The tonemap shader reads from `objects[0]` to get:
    /// - `texture_indices.x` = HDR texture bindless slot
    /// - Custom exposure, gamma, and mode values
    ///
    /// # Arguments
    /// * `params` - Tonemap configuration
    ///
    /// # Example
    /// ```ignore
    /// FullscreenPass::new("tonemap")
    ///     .read("hdr_color")
    ///     .write_backbuffer()
    ///     .pipeline(pipeline)
    ///     .tonemap(TonemapParams {
    ///         exposure: 1.2,
    ///         gamma: 2.2,
    ///         mode: TonemapOperator::Aces,
    ///         hdr_texture_index: Some(hdr_slot),
    ///     });
    /// ```
    pub fn tonemap(mut self, params: TonemapParams) -> Self {
        self.tonemap_params = Some(params);
        self
    }
}

impl PassBuilder for FullscreenPass {
    fn as_builder(self) -> InternalPassBuilder {
        let writes: Vec<String> = self.writes.iter().map(|(n, _)| n.clone()).collect();

        // Hand-declared typed accesses: inputs are sampled, outputs are
        // color attachment writes.
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
            .chain(self.writes.iter().map(|(name, _)| {
                super::named_image_access(
                    name.clone(),
                    ImageAccessMode::Write,
                    ImageUsage::ColorAttachment,
                    ImagePipelineStage::ColorAttachmentOutput,
                    ImageSubresourceRange::WHOLE_COLOR,
                )
            }))
            .collect();

        // Fullscreen draws cover the whole target, but the historical canvas
        // clear is preserved: the first writer leaves [0.1, 0.1, 0.1, 1.0]
        // where nothing was drawn.
        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads.clone(),
            writes,
            image_accesses,
            pipeline: self.pipeline,
            tonemap_params: self.tonemap_params,
            overlay_params: None,
            material: None,
            output_format: None,
            build_fn: Box::new(
                move |_resource_map: &HashMap<String, GraphResourceHandle>| Ok(Box::new(())),
            ),
            uses_depth: false,
            color_attachments: self
                .writes
                .iter()
                .map(|(name, _)| {
                    (
                        name.clone(),
                        crate::render_pass::AttachmentOps::clear(
                            crate::render_pass::ClearValue::color(0.1, 0.1, 0.1, 1.0),
                        ),
                    )
                })
                .collect(),
            depth_attachment: None,
            kind: Some(PassKind::Fullscreen),
            side_effect: false,
        }
    }
}

/// Overlay parameters for the wallhack overlay pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct OverlayParams {
    /// Bindless texture index for LDR source (tonemap output).
    pub ldr_texture_index: Option<u32>,
    /// Bindless texture index for stencil indicator R8 mask.
    pub stencil_indicator_index: Option<u32>,
}

/// Wallhack overlay pass — applies tint to occluded selected objects.
///
/// Reads the LDR tonemap output and the stencil indicator R8 mask, then
/// writes the composited result. This is a fullscreen pass that runs after
/// tonemapping to keep the tonemap shader pure (HDR->LDR only).
pub struct OverlayPass {
    name: String,
    reads: Vec<String>,
    writes: Vec<(String, ImageFormat)>,
    pipeline: Option<PipelineHandle>,
    overlay_params: Option<OverlayParams>,
}

impl OverlayPass {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
            writes: Vec::new(),
            pipeline: None,
            overlay_params: None,
        }
    }

    pub fn read(mut self, name: impl Into<String>) -> Self {
        self.reads.push(name.into());
        self
    }

    pub fn write(mut self, name: impl Into<String>, format: ImageFormat) -> Self {
        self.writes.push((name.into(), format));
        self
    }

    pub fn pipeline(mut self, pipeline: PipelineHandle) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    pub fn overlay(mut self, params: OverlayParams) -> Self {
        self.overlay_params = Some(params);
        self
    }
}

impl PassBuilder for OverlayPass {
    fn as_builder(self) -> InternalPassBuilder {
        let writes: Vec<String> = self.writes.iter().map(|(n, _)| n.clone()).collect();

        // Hand-declared typed accesses: the overlay blends into its target
        // (read-write color attachment); extra reads are sampled.
        let write_set = writes.iter().cloned().collect::<HashSet<_>>();
        let image_accesses = self
            .reads
            .iter()
            .filter(|name| !write_set.contains(*name))
            .map(|name| {
                super::named_image_access(
                    name.clone(),
                    ImageAccessMode::Read,
                    ImageUsage::Sampled,
                    ImagePipelineStage::FragmentShader,
                    ImageAccess::WHOLE_RESOURCE,
                )
            })
            .chain(self.writes.iter().map(|(name, _)| {
                super::named_image_access(
                    name.clone(),
                    ImageAccessMode::ReadWrite,
                    ImageUsage::ColorAttachment,
                    ImagePipelineStage::ColorAttachmentOutput,
                    ImageSubresourceRange::WHOLE_COLOR,
                )
            }))
            .collect();

        // The overlay composites over the tonemapped contents of its target.
        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads.clone(),
            writes,
            image_accesses,
            pipeline: self.pipeline,
            tonemap_params: None,
            material: None,
            output_format: None,
            build_fn: Box::new(
                move |_resource_map: &HashMap<String, GraphResourceHandle>| Ok(Box::new(())),
            ),
            uses_depth: false,
            color_attachments: self
                .writes
                .iter()
                .map(|(name, _)| (name.clone(), crate::render_pass::AttachmentOps::load()))
                .collect(),
            depth_attachment: None,
            kind: Some(PassKind::Fullscreen),
            side_effect: false,
            overlay_params: self.overlay_params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tonemap_params_default() {
        let params = TonemapParams::default();
        assert_eq!(params.exposure, 1.0);
        assert_eq!(params.gamma, 2.2);
        assert_eq!(params.mode, TonemapOperator::Aces);
        assert!(params.hdr_texture_index.is_none());
    }

    #[test]
    fn test_fullscreen_pass_build_fn_with_resources() {
        let pipeline = PipelineHandle::from_raw(42, 0);
        let pass = FullscreenPass::new("tone_map")
            .read("hdr_color")
            .write("ldr_output", ImageFormat::R8G8B8A8Srgb)
            .pipeline(pipeline);

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("hdr_color".to_string(), GraphResourceHandle::new(0));
        resource_map.insert("ldr_output".to_string(), GraphResourceHandle::new(1));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());
    }

    #[test]
    fn fullscreen_pass_declares_typed_accesses() {
        use crate::render_graph::access::{
            ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
            NamedImageAccess,
        };

        let builder = FullscreenPass::new("tonemap")
            .read("hdr_color")
            .write("viewport_0", ImageFormat::B8G8R8A8Srgb)
            .as_builder();

        assert_eq!(
            builder.image_accesses,
            vec![
                NamedImageAccess {
                    resource: "hdr_color".to_string(),
                    mode: ImageAccessMode::Read,
                    usage: ImageUsage::Sampled,
                    stage: ImagePipelineStage::FragmentShader,
                    range: ImageAccess::WHOLE_RESOURCE,
                },
                NamedImageAccess {
                    resource: "viewport_0".to_string(),
                    mode: ImageAccessMode::Write,
                    usage: ImageUsage::ColorAttachment,
                    stage: ImagePipelineStage::ColorAttachmentOutput,
                    range: ImageSubresourceRange::WHOLE_COLOR,
                },
            ]
        );
    }

    #[test]
    fn overlay_pass_declares_a_read_write_access_for_its_target() {
        use crate::render_graph::access::{
            ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
            NamedImageAccess,
        };

        let builder = OverlayPass::new("overlay")
            .read("viewport_0")
            .read("stencil_indicator")
            .write("viewport_0", ImageFormat::B8G8R8A8Srgb)
            .as_builder();

        // The overlay target is declared once as a read-write attachment;
        // its extra input stays a sampled read.
        assert_eq!(
            builder.image_accesses,
            vec![
                NamedImageAccess {
                    resource: "stencil_indicator".to_string(),
                    mode: ImageAccessMode::Read,
                    usage: ImageUsage::Sampled,
                    stage: ImagePipelineStage::FragmentShader,
                    range: ImageAccess::WHOLE_RESOURCE,
                },
                NamedImageAccess {
                    resource: "viewport_0".to_string(),
                    mode: ImageAccessMode::ReadWrite,
                    usage: ImageUsage::ColorAttachment,
                    stage: ImagePipelineStage::ColorAttachmentOutput,
                    range: ImageSubresourceRange::WHOLE_COLOR,
                },
            ]
        );
    }

    #[test]
    fn test_fullscreen_pass_build_fn_empty_resources() {
        let pass = FullscreenPass::new("tone_map")
            .read("hdr_color")
            .write("ldr_output", ImageFormat::R8G8B8A8Srgb);

        let builder = pass.as_builder();
        let resource_map = HashMap::new();

        // FullscreenPass build_fn doesn't validate resources
        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());
    }

    #[test]
    fn test_fullscreen_pass_tonemap_params_propagate() {
        let params = TonemapParams {
            exposure: 1.5,
            gamma: 2.4,
            mode: TonemapOperator::Reinhard,
            hdr_texture_index: Some(5),
        };
        let pass = FullscreenPass::new("tonemap")
            .read("hdr_color")
            .write_backbuffer()
            .pipeline(PipelineHandle::from_raw(1, 0))
            .tonemap(params);

        assert!(pass.tonemap_params.is_some());
        let tonemap = pass.tonemap_params.unwrap();
        assert_eq!(tonemap.exposure, 1.5);
        assert_eq!(tonemap.gamma, 2.4);
        assert_eq!(tonemap.mode, TonemapOperator::Reinhard);
        assert_eq!(tonemap.hdr_texture_index, Some(5));
    }
}
