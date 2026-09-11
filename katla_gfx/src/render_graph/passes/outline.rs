use super::super::access::{
    ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
};
use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::{PassKind, PassType};
use crate::render_pass::{AttachmentOps, ClearValue, DepthStencilAttachmentOps, LoadOp, StoreOp};
use crate::texture::ImageFormat;

/// Outline pass template for stencil-based selection highlights.
///
/// Writes to an HDR color buffer and uses depth (loaded from the depth prepass).
/// Executed after geometry, before tonemapping.
#[derive(Debug, Clone)]
pub struct OutlinePass {
    name: String,
    writes: Vec<String>,
}

impl OutlinePass {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            writes: Vec::new(),
        }
    }

    /// Write the outline to an HDR color buffer (loaded, not cleared).
    pub fn write_color(mut self, name: impl Into<String>, _format: ImageFormat) -> Self {
        self.writes.push(name.into());
        self
    }
}

impl PassBuilder for OutlinePass {
    fn as_builder(self) -> InternalPassBuilder {
        let writes = self.writes.clone();
        let reads = writes.clone();

        // Outline shells blend over the existing HDR contents.
        let color_attachments = writes
            .iter()
            .map(|name| (name.clone(), AttachmentOps::load()))
            .collect();

        // Hand-declared typed accesses: outline shells blend into the HDR
        // target they load.
        let image_accesses = self
            .writes
            .iter()
            .map(|name| {
                super::named_image_access(
                    name.clone(),
                    ImageAccessMode::ReadWrite,
                    ImageUsage::ColorAttachment,
                    ImagePipelineStage::ColorAttachmentOutput,
                    ImageSubresourceRange::WHOLE_COLOR,
                )
            })
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
            material: None,
            output_format: Some(ImageFormat::R16G16B16A16Sfloat),
            build_fn: Box::new(|_| Ok(Box::new(()))),
            uses_depth: true,
            color_attachments,
            // Depth is reused from the scene; the stencil aspect is cleared
            // and stored so the sub-passes can mark and combine stencil bits.
            depth_attachment: Some(DepthStencilAttachmentOps {
                depth: AttachmentOps::clear(ClearValue::DepthStencil {
                    depth: 0.0,
                    stencil: 0,
                })
                .with_load(LoadOp::Load),
                stencil: AttachmentOps::clear(ClearValue::DepthStencil {
                    depth: 0.0,
                    stencil: 0,
                }),
            }),
            kind: Some(PassKind::Outline),
            side_effect: false,
        }
    }
}

/// Stencil indicator pass — writes 1.0 to an R8 texture where stencil == 2
/// (occluded parts of selected objects). Sampled by the tonemap shader to
/// apply the wallhack overlay tint entirely in-shader.
#[derive(Debug, Clone)]
pub struct StencilIndicatorPass {
    name: String,
    writes: Vec<String>,
}

impl StencilIndicatorPass {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            writes: Vec::new(),
        }
    }

    pub fn write_color(mut self, name: impl Into<String>, _format: ImageFormat) -> Self {
        self.writes.push(name.into());
        self
    }
}

impl PassBuilder for StencilIndicatorPass {
    fn as_builder(self) -> InternalPassBuilder {
        let writes = self.writes.clone();

        // The indicator mask is rebuilt every frame.
        let color_attachments = writes
            .iter()
            .map(|name| (name.clone(), AttachmentOps::clear(ClearValue::OPAQUE_BLACK)))
            .collect();

        // Hand-declared typed accesses: the indicator mask is a fresh color
        // attachment write each frame.
        let image_accesses = self
            .writes
            .iter()
            .map(|name| {
                super::named_image_access(
                    name.clone(),
                    ImageAccessMode::Write,
                    ImageUsage::ColorAttachment,
                    ImagePipelineStage::ColorAttachmentOutput,
                    ImageSubresourceRange::WHOLE_COLOR,
                )
            })
            .collect();

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: Vec::new(),
            writes,
            image_accesses,
            pipeline: None,
            tonemap_params: None,
            overlay_params: None,
            material: None,
            output_format: Some(ImageFormat::R8Unorm),
            build_fn: Box::new(|_| Ok(Box::new(()))),
            uses_depth: true,
            color_attachments,
            // Both aspects load the stencil state left by the outline pass;
            // neither is stored back.
            depth_attachment: Some(DepthStencilAttachmentOps {
                depth: AttachmentOps::clear(ClearValue::DepthStencil {
                    depth: 0.0,
                    stencil: 0,
                })
                .with_load(LoadOp::Load)
                .with_store(StoreOp::DontCare),
                stencil: AttachmentOps::clear(ClearValue::DepthStencil {
                    depth: 0.0,
                    stencil: 0,
                })
                .with_load(LoadOp::Load)
                .with_store(StoreOp::DontCare),
            }),
            kind: Some(PassKind::StencilIndicator),
            side_effect: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_reads_the_color_target_it_loads() {
        let builder = OutlinePass::new("outline")
            .write_color("hdr", ImageFormat::R16G16B16A16Sfloat)
            .as_builder();
        assert_eq!(builder.reads, vec!["hdr"]);
        assert_eq!(builder.writes, vec!["hdr"]);
        assert_eq!(builder.color_attachments[0].1.load, LoadOp::Load);
        assert_eq!(
            builder.depth_attachment.unwrap().stencil.load,
            LoadOp::Clear
        );
    }

    #[test]
    fn stencil_indicator_loads_stencil_and_discards_depth() {
        let builder = StencilIndicatorPass::new("indicator")
            .write_color("stencil_indicator", ImageFormat::R8Unorm)
            .as_builder();
        let ops = builder.depth_attachment.unwrap();
        assert_eq!(ops.depth.load, LoadOp::Load);
        assert_eq!(ops.depth.store, StoreOp::DontCare);
        assert_eq!(ops.stencil.load, LoadOp::Load);
        assert_eq!(ops.stencil.store, StoreOp::DontCare);
    }
}
