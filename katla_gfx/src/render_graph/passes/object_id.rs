//! Object-ID picking pass template.
//!
//! Renders each mesh with a flat color encoding its instance index into a R32Uint texture.
//! Used for GPU-based entity picking via pixel readback.

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::{PassKind, PassType};
use crate::render_pass::{
    AttachmentOps, ClearValue, DepthStencilAttachmentOps, LoadOp, StoreOp,
};
use crate::texture::ImageFormat;

/// Object-ID picking pass template.
///
/// Renders each object with a unique flat color encoding its instance index.
/// The output is a R32Uint texture where each pixel contains the 1-based
/// instance index of the closest visible object. Pixel value 0 means no object.
///
/// Uses depth testing with LoadOp::Load to reuse depth from the depth prepass.
#[derive(Debug, Clone)]
pub struct ObjectIdPass {
    name: String,
    reads: Vec<String>,
    writes: Vec<String>,
    depth_config: Option<DepthStencilAttachmentOps>,
}

impl ObjectIdPass {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
            writes: Vec::new(),
            depth_config: None,
        }
    }

    /// Write object IDs to a named resource.
    pub fn write(mut self, name: impl Into<String>) -> Self {
        self.writes.push(name.into());
        self
    }

    /// Read from a resource (e.g., depth buffer dependency).
    pub fn read(mut self, name: impl Into<String>) -> Self {
        self.reads.push(name.into());
        self
    }

    /// Configure depth attachment (default: LoadOp::Load to reuse depth prepass).
    pub fn depth_config(mut self, depth: AttachmentOps, stencil: AttachmentOps) -> Self {
        self.depth_config = Some(DepthStencilAttachmentOps { depth, stencil });
        self
    }
}

impl PassBuilder for ObjectIdPass {
    fn as_builder(self) -> InternalPassBuilder {
        // Object-ID targets are cleared to 0 (no object) and stored.
        let color_attachments = self
            .writes
            .iter()
            .map(|name| (name.clone(), AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK)))
            .collect();

        // Default depth contract: load the depth prepass result, discard it.
        let depth_attachment = self.depth_config.unwrap_or(DepthStencilAttachmentOps {
            depth: AttachmentOps::clear(ClearValue::DepthStencil {
                depth: 0.0,
                stencil: 0,
            })
            .with_load(LoadOp::Load)
            .with_store(StoreOp::DontCare),
            stencil: AttachmentOps::dont_care(),
        });

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads,
            writes: self.writes,
            image_accesses: Vec::new(),
            pipeline: None,
            tonemap_params: None,
            overlay_params: None,
            material: None,
            output_format: Some(ImageFormat::R32Uint),
            build_fn: Box::new(|_| Ok(Box::new(()))),
            uses_depth: true,
            color_attachments,
            depth_attachment: Some(depth_attachment),
            kind: Some(PassKind::ObjectId),
            side_effect: false,
        }
    }
}
