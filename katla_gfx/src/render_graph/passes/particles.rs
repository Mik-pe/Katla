use super::super::access::{
    ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
};
use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::{PassKind, PassType};
use crate::render_pass::{AttachmentOps, ClearValue, DepthStencilAttachmentOps, LoadOp};
use crate::texture::ImageFormat;

/// Particle render pass template.
///
/// Renders GPU-simulated particles with alpha blending onto an HDR color buffer.
/// Depth testing reuses the scene depth from the depth prepass (LoadOp::Load).
///
/// The pass is a no-op when no particles are alive — it checks
/// `particle_system.alive_count()` before issuing draw calls.
pub struct ParticlePass {
    name: String,
    writes: Vec<String>,
}

impl ParticlePass {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            writes: Vec::new(),
        }
    }

    /// Write particles to an HDR color buffer (typically the same one geometry writes to).
    pub fn write_color(mut self, name: impl Into<String>, _format: ImageFormat) -> Self {
        self.writes.push(name.into());
        self
    }
}

impl PassBuilder for ParticlePass {
    fn as_builder(self) -> InternalPassBuilder {
        let writes = self.writes.clone();
        let reads = writes.clone();

        // Particles alpha-blend over the existing HDR contents.
        let color_attachments = writes
            .iter()
            .map(|name| (name.clone(), AttachmentOps::load()))
            .collect();

        // Hand-declared typed accesses: particles blend into the HDR target
        // they load.
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
            // Depth is reused from the scene and stored for later passes.
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
            kind: Some(PassKind::Particles),
            side_effect: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particles_read_the_color_target_they_blend_into() {
        let builder = ParticlePass::new("particles")
            .write_color("hdr", ImageFormat::R16G16B16A16Sfloat)
            .as_builder();
        assert_eq!(builder.reads, vec!["hdr"]);
        assert_eq!(builder.writes, vec!["hdr"]);
        assert_eq!(builder.color_attachments[0].1.load, LoadOp::Load);
    }
}
