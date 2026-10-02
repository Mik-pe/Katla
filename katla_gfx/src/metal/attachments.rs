//! Resolve compiled attachment identities before creating native encoders.

use super::execution_plan::MetalPassRecord;
use super::{MetalBackend, metal_renderer::MetalRenderer, texture::MetalTextureView};
use crate::backend::command::{ColorAttachmentInfo, DepthAttachmentInfo, RenderPassInfo};
use crate::error::RendererError;
use crate::render_graph::{FrameGraph, ResourceId};
use crate::texture::ImageFormat;
use objc2_metal::MTLTexture;

pub(crate) struct ResolvedMetalAttachments {
    pub(crate) info: RenderPassInfo<MetalBackend>,
    pub(crate) color_targets: Vec<String>,
    pub(crate) depth_target: Option<String>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl ResolvedMetalAttachments {
    pub(crate) fn resolve(
        record: &MetalPassRecord,
        graph: &FrameGraph<MetalRenderer>,
        drawable: &MetalTextureView,
        slot: usize,
        renderer: &MetalRenderer,
    ) -> Result<Self, RendererError> {
        let resolve = |resource: ResourceId| -> Result<MetalTextureView, RendererError> {
            if graph.resource_name(resource) == Some("backbuffer") {
                return Ok(drawable.clone());
            }
            if let Some(handle) = graph.imported_images.get(&resource) {
                return renderer
                    .textures
                    .get(*handle)
                    .map(|entry| entry._view.clone())
                    .ok_or_else(|| {
                        RendererError::InvalidOperation(format!(
                            "Imported attachment {} references a destroyed texture",
                            resource.0
                        ))
                    });
            }
            graph
                .transient_texture_by_id(resource, slot)
                .map(|texture| texture.view.clone())
                .ok_or_else(|| {
                    RendererError::InvalidOperation(format!(
                        "Metal pass '{}' cannot resolve attachment {} in frame slot {slot}",
                        record.name, resource.0
                    ))
                })
        };
        let mut color_targets = Vec::new();
        let mut extent = None;
        let mut validate =
            |view: &MetalTextureView, format: ImageFormat| -> Result<(), RendererError> {
                if view.inner.pixelFormat() != super::format::to_mtl_pixel_format(format) {
                    return Err(RendererError::InvalidOperation(format!(
                        "Metal pass '{}' attachment format does not match its declaration",
                        record.name
                    )));
                }
                if !view
                    .inner
                    .usage()
                    .contains(objc2_metal::MTLTextureUsage::RenderTarget)
                {
                    return Err(RendererError::InvalidOperation(format!(
                        "Metal pass '{}' attachment lacks render-target usage",
                        record.name
                    )));
                }
                let dimensions = (view.inner.width() as u32, view.inner.height() as u32);
                if dimensions.0 == 0
                    || dimensions.1 == 0
                    || extent.is_some_and(|previous| previous != dimensions)
                {
                    return Err(RendererError::InvalidOperation(format!(
                        "Metal pass '{}' has incompatible attachment extents",
                        record.name
                    )));
                }
                extent = Some(dimensions);
                Ok(())
            };
        let mut colors = Vec::new();
        for attachment in &record.color_attachments {
            if matches!(
                attachment.format,
                ImageFormat::D32Sfloat | ImageFormat::D32SfloatS8Uint | ImageFormat::D24UnormS8Uint
            ) {
                return Err(RendererError::InvalidOperation(format!(
                    "Metal pass '{}' has a depth image as a color target",
                    record.name
                )));
            }
            let view = resolve(attachment.resource)?;
            validate(&view, attachment.format)?;
            color_targets.push(
                graph
                    .resource_name(attachment.resource)
                    .unwrap_or("?")
                    .to_string(),
            );
            colors.push(ColorAttachmentInfo {
                view,
                load_op: attachment.load_op,
                store_op: attachment.store_op,
                clear_value: attachment.clear_value,
            });
        }
        let depth_attachment = record
            .depth_attachment
            .map(|attachment| {
                let view = resolve(attachment.resource)?;
                validate(&view, attachment.format)?;
                Ok::<_, RendererError>(DepthAttachmentInfo {
                    view,
                    format: attachment.format,
                    load_op: attachment.load_op,
                    store_op: attachment.store_op,
                    clear_value: attachment.clear_value,
                    stencil_ops: attachment.stencil_ops,
                })
            })
            .transpose()?;
        let (width, height) = extent.ok_or_else(|| {
            RendererError::InvalidOperation(format!(
                "Metal pass '{}' declares no render attachments",
                record.name
            ))
        })?;
        Ok(Self {
            info: RenderPassInfo {
                color_attachments: colors,
                depth_attachment,
                debug_label: None,
            },
            color_targets,
            depth_target: record.depth_attachment.and_then(|attachment| {
                graph.resource_name(attachment.resource).map(str::to_string)
            }),
            width,
            height,
        })
    }
}
