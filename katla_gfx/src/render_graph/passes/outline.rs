use std::collections::HashMap;

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::error::RenderGraphError;
use super::super::pass::PassType;
use super::super::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
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

    /// Write the outline to an HDR color buffer.
    pub fn write_color(mut self, name: impl Into<String>, _format: ImageFormat) -> Self {
        self.writes.push(name.into());
        self
    }
}

#[allow(dead_code)]
pub(crate) struct OutlinePassData {
    pub(crate) colors: Vec<(
        GraphResourceHandle,
        ImageFormat,
        LoadOp,
        StoreOp,
        ClearValue,
    )>,
}

impl PassBuilder for OutlinePass {
    fn as_builder(self) -> InternalPassBuilder {
        let writes = self.writes.clone();
        let build_writes = writes.clone();

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: Vec::new(),
            writes,
            pipeline: None,
            tonemap_params: None,
            material: None,
            output_format: Some(ImageFormat::R16G16B16A16Sfloat),
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
                            ImageFormat::R16G16B16A16Sfloat,
                            LoadOp::Load,
                            StoreOp::Store,
                            ClearValue::Color([0.0, 0.0, 0.0, 1.0]),
                        ))
                    })
                    .collect::<Result<Vec<_>, RenderGraphError>>()?;

                Ok(Box::new(OutlinePassData { colors }))
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

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: Vec::new(),
            writes,
            pipeline: None,
            tonemap_params: None,
            material: None,
            output_format: Some(ImageFormat::R8Unorm),
            build_fn: Box::new(
                move |_resource_map: &HashMap<String, GraphResourceHandle>| Ok(Box::new(())),
            ),
            uses_depth: true,
            depth_attachment: Some((
                LoadOp::Load,
                StoreOp::DontCare,
                ClearValue::DepthStencil {
                    depth: 0.0,
                    stencil: 0,
                },
            )),
        }
    }
}
