//! Depth prepass template.
//!
//! Renders scene depth from camera's perspective and optionally outputs object IDs
//! to a R32Uint texture for GPU-based entity picking.
//! The depth buffer is then reused by the geometry pass via `LoadOp::Load`.

use std::collections::HashMap;

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::error::RenderGraphError;
use super::super::pass::PassType;
use super::super::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
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

/// Internal data for a depth prepass with optional color output.
pub(crate) struct DepthPrepassData {
    pub(crate) colors: Vec<(
        GraphResourceHandle,
        ImageFormat,
        LoadOp,
        StoreOp,
        ClearValue,
    )>,
}

impl PassBuilder for DepthPrepass {
    fn as_builder(self) -> InternalPassBuilder {
        let writes = self.writes.clone();
        let reads = self.reads.clone();
        let has_writes = !writes.is_empty();
        let build_writes = writes.clone();

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads,
            writes,
            pipeline: None,
            tonemap_params: None,
            material: None,
            output_format: if has_writes {
                Some(ImageFormat::R32Uint)
            } else {
                None
            },
            build_fn: Box::new(move |resource_map: &HashMap<String, GraphResourceHandle>| {
                let colors: Vec<(
                    GraphResourceHandle,
                    ImageFormat,
                    LoadOp,
                    StoreOp,
                    ClearValue,
                )> = build_writes
                    .iter()
                    .map(|output_name| {
                        let handle = resource_map.get(output_name).copied().ok_or_else(|| {
                            RenderGraphError::ResourceNotFound(output_name.clone())
                        })?;
                        Ok((
                            handle,
                            ImageFormat::R32Uint,
                            LoadOp::Clear,
                            StoreOp::Store,
                            ClearValue::Color([0.0, 0.0, 0.0, 0.0]),
                        ))
                    })
                    .collect::<Result<Vec<_>, RenderGraphError>>()?;

                Ok(Box::new(DepthPrepassData { colors }))
            }),
            uses_depth: true,
            depth_attachment: None,
        }
    }
}
