//! Fullscreen graphics pass template.
//!
//! Declares sampled inputs and color outputs; explicit pass bindings select shaders and constants.

use std::collections::{HashMap, HashSet};

use crate::texture::ImageFormat;

use super::super::access::{
    ImageAccess, ImageSubresourceRange, ResourceAccessMode, ResourceAccessStage,
    ResourceAccessUsage,
};
use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::{PassKind, PassType};
use super::super::resource::GraphResourceHandle;
use crate::render_graph::BACKBUFFER_NAME;

/// Fullscreen graphics pass template.
///
/// Declares sampled inputs and color outputs; explicit pass bindings select shaders and constants.
///
pub struct FullscreenPass {
    name: String,
    reads: Vec<String>,
    writes: Vec<(String, ImageFormat)>,
}

impl FullscreenPass {
    /// Create a new fullscreen pass.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
            writes: Vec::new(),
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
                    ResourceAccessMode::Read,
                    ResourceAccessUsage::Sampled,
                    ResourceAccessStage::FragmentShader,
                    ImageAccess::WHOLE_RESOURCE,
                )
            })
            .chain(self.writes.iter().map(|(name, _)| {
                super::named_image_access(
                    name.clone(),
                    ResourceAccessMode::Write,
                    ResourceAccessUsage::ColorAttachment,
                    ResourceAccessStage::ColorAttachmentOutput,
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
            buffer_accesses: Vec::new(),
            material: None,
            output_format: None,
            build_fn: Box::new(
                move |_resource_map: &HashMap<String, GraphResourceHandle>| Ok(Box::new(())),
            ),
            uses_depth: false,
            depth_target: None,
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

/// Wallhack overlay pass — applies tint to occluded selected objects.
///
/// Reads the LDR tonemap output and the stencil indicator R8 mask, then
/// writes the composited result. This is a fullscreen pass that runs after
/// tonemapping to keep the tonemap shader pure (HDR->LDR only).
pub struct OverlayPass {
    name: String,
    reads: Vec<String>,
    writes: Vec<(String, ImageFormat)>,
}

impl OverlayPass {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
            writes: Vec::new(),
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
                    ResourceAccessMode::Read,
                    ResourceAccessUsage::Sampled,
                    ResourceAccessStage::FragmentShader,
                    ImageAccess::WHOLE_RESOURCE,
                )
            })
            .chain(self.writes.iter().map(|(name, _)| {
                super::named_image_access(
                    name.clone(),
                    ResourceAccessMode::ReadWrite,
                    ResourceAccessUsage::ColorAttachment,
                    ResourceAccessStage::ColorAttachmentOutput,
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
            buffer_accesses: Vec::new(),
            material: None,
            output_format: None,
            build_fn: Box::new(
                move |_resource_map: &HashMap<String, GraphResourceHandle>| Ok(Box::new(())),
            ),
            uses_depth: false,
            depth_target: None,
            color_attachments: self
                .writes
                .iter()
                .map(|(name, _)| (name.clone(), crate::render_pass::AttachmentOps::load()))
                .collect(),
            depth_attachment: None,
            kind: Some(PassKind::Fullscreen),
            side_effect: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fullscreen_pass_build_fn_with_resources() {
        let pass = FullscreenPass::new("tone_map")
            .read("hdr_color")
            .write("ldr_output", ImageFormat::R8G8B8A8Srgb);

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
            ImageSubresourceRange, NamedImageAccess, ResourceAccessMode, ResourceAccessStage,
            ResourceAccessUsage,
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
                    mode: ResourceAccessMode::Read,
                    usage: ResourceAccessUsage::Sampled,
                    stage: ResourceAccessStage::FragmentShader,
                    range: ImageAccess::WHOLE_RESOURCE,
                },
                NamedImageAccess {
                    resource: "viewport_0".to_string(),
                    mode: ResourceAccessMode::Write,
                    usage: ResourceAccessUsage::ColorAttachment,
                    stage: ResourceAccessStage::ColorAttachmentOutput,
                    range: ImageSubresourceRange::WHOLE_COLOR,
                },
            ]
        );
    }

    #[test]
    fn overlay_pass_declares_a_read_write_access_for_its_target() {
        use crate::render_graph::access::{
            ImageSubresourceRange, NamedImageAccess, ResourceAccessMode, ResourceAccessStage,
            ResourceAccessUsage,
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
                    mode: ResourceAccessMode::Read,
                    usage: ResourceAccessUsage::Sampled,
                    stage: ResourceAccessStage::FragmentShader,
                    range: ImageAccess::WHOLE_RESOURCE,
                },
                NamedImageAccess {
                    resource: "viewport_0".to_string(),
                    mode: ResourceAccessMode::ReadWrite,
                    usage: ResourceAccessUsage::ColorAttachment,
                    stage: ResourceAccessStage::ColorAttachmentOutput,
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
}
