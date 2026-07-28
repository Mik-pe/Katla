//! Object-ID picking pass template.
//!
//! Renders each mesh with a flat color encoding its instance index into a R32Uint texture.
//! Used for GPU-based entity picking via pixel readback.

use std::collections::HashMap;

use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::pass::{PassKind, PassType};
use crate::render_graph::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
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
    depth_config: Option<(LoadOp, StoreOp, ClearValue)>,
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
    pub fn depth_config(
        mut self,
        load_op: LoadOp,
        store_op: StoreOp,
        clear_value: ClearValue,
    ) -> Self {
        self.depth_config = Some((load_op, store_op, clear_value));
        self
    }
}

impl PassBuilder for ObjectIdPass {
    fn as_builder(self) -> InternalPassBuilder {
        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads,
            writes: self.writes,
            pipeline: None,
            tonemap_params: None,
            overlay_params: None,
            material: None,
            output_format: Some(ImageFormat::R32Uint),
            build_fn: Box::new(|_resource_map: &HashMap<String, GraphResourceHandle>| {
                Ok(Box::new(()))
            }),
            uses_depth: true,
            depth_attachment: self.depth_config,
            kind: Some(PassKind::ObjectId),
        }
    }
}


