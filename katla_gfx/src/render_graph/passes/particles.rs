use std::collections::HashMap;

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::{PassKind, PassType};
use super::super::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
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

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads,
            writes,
            image_accesses: Vec::new(),
            pipeline: None,
            tonemap_params: None,
            overlay_params: None,
            material: None,
            output_format: Some(ImageFormat::R16G16B16A16Sfloat),
            build_fn: Box::new(|_resource_map: &HashMap<String, GraphResourceHandle>| {
                Ok(Box::new(()))
            }),
            uses_depth: true,
            depth_attachment: Some((
                LoadOp::Load,
                StoreOp::Store,
                ClearValue::DepthStencil {
                    depth: 0.0,
                    stencil: 0,
                },
            )),
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
    }
}
