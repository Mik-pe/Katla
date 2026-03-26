//! Depth prepass template.
//!
//! Renders scene depth from camera's perspective using a depth-only pipeline.
//! The depth buffer is then reused by the geometry pass via `LoadOp::Load`.

use std::collections::HashMap;

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::PassType;
use super::super::resource::GraphResourceHandle;

/// Depth prepass template.
///
/// Renders only depth from the camera's perspective. No color output.
/// The geometry pass should follow with `depth_config(LoadOp::Load, ...)`.
#[derive(Debug, Clone)]
pub struct DepthPrepass {
    name: String,
    reads: Vec<String>,
}

impl DepthPrepass {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
        }
    }

    pub fn read(mut self, name: impl Into<String>) -> Self {
        self.reads.push(name.into());
        self
    }
}

impl PassBuilder for DepthPrepass {
    fn as_builder(self) -> InternalPassBuilder {
        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads,
            writes: Vec::new(),
            pipeline: None,
            tonemap_params: None,
            material: None,
            output_format: None,
            build_fn: Box::new(|_resource_map: &HashMap<String, GraphResourceHandle>| {
                Ok(Box::new(DepthPrepassData))
            }),
            uses_depth: true,
            depth_attachment: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct DepthPrepassData;

// DepthPrepass tests are covered by the builder trait test in builder.rs.
