//! Depth prepass template.
//!
//! Renders scene depth from camera's perspective and optionally outputs object IDs
//! to a R32Uint texture for GPU-based entity picking.
//! The depth buffer is then reused by the geometry pass via `LoadOp::Load`.

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::{PassKind, PassType};
use crate::render_pass::{AttachmentOps, ClearValue};
use crate::texture::ImageFormat;

/// Depth prepass template.
///
/// Renders depth from the camera's perspective and optionally outputs object IDs
/// to a R32Uint texture for GPU-based entity picking.
/// The depth buffer is then reused by the geometry pass via `LoadOp::Load`.
#[derive(Debug, Clone)]
pub struct DepthPrepass {
    name: String,
    reads: Vec<String>,
    writes: Vec<String>,
}

impl DepthPrepass {
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

    /// Write object IDs to a named R32Uint resource for GPU picking.
    pub fn write_object_id(mut self, name: impl Into<String>) -> Self {
        self.writes.push(name.into());
        self
    }
}

impl PassBuilder for DepthPrepass {
    fn as_builder(self) -> InternalPassBuilder {
        let writes = self.writes.clone();
        let has_writes = !writes.is_empty();

        // Object-ID targets are cleared to 0 (no object) and stored.
        let color_attachments = writes
            .iter()
            .map(|name| {
                (
                    name.clone(),
                    AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK),
                )
            })
            .collect();

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads,
            writes,
            image_accesses: Vec::new(),
            pipeline: None,
            tonemap_params: None,
            overlay_params: None,
            material: None,
            output_format: if has_writes {
                Some(ImageFormat::R32Uint)
            } else {
                None
            },
            build_fn: Box::new(|_| Ok(Box::new(()))),
            uses_depth: true,
            color_attachments,
            depth_attachment: None,
            kind: Some(PassKind::DepthPrepass),
            side_effect: false,
        }
    }
}
