//! Copies retain the exact exported image and submission generation.

use std::collections::HashMap;

use objc2_metal::{MTL4CommandBuffer, MTLTexture};

use crate::backend::command::{GpuBlitEncoder, GpuCommandBuffer};
use crate::backend::resource::{GpuBuffer, GpuImage, GpuImageView};
use crate::error::RendererError;
use crate::render_graph::ResourceId;
use crate::renderer::texture_readback::{
    GraphTextureSource, TextureReadbackData, TextureReadbackRegion, TextureReadbackTicket,
};

use super::buffer::MetalBuffer;
use super::command_buffer::MetalCommandBuffer;
use super::context::MetalContext;
use super::texture::MetalTextureView;

struct PendingReadback {
    ticket: TextureReadbackTicket,
    _source: MetalTextureView,
    command: MetalCommandBuffer,
    buffer: MetalBuffer,
    format: crate::texture::ImageFormat,
    region: TextureReadbackRegion,
    row_bytes: usize,
    row_pitch: usize,
}

#[derive(Default)]
pub(crate) struct MetalTextureReadbacks {
    sources: HashMap<ResourceId, (GraphTextureSource, MetalTextureView)>,
    pending: HashMap<u64, PendingReadback>,
}

impl MetalTextureReadbacks {
    pub(crate) fn publish(
        &mut self,
        images: HashMap<ResourceId, MetalTextureView>,
        slot: usize,
        generation: u64,
    ) {
        self.sources = images
            .into_iter()
            .map(|(resource, view)| {
                let source = GraphTextureSource {
                    id: crate::renderer::texture_readback::fresh_readback_id(),
                    resource,
                    frame_slot: slot,
                    generation,
                    submission: generation,
                };
                (resource, (source, view))
            })
            .collect();
    }

    pub(crate) fn source(&self, resource: ResourceId) -> Option<GraphTextureSource> {
        self.sources.get(&resource).map(|(source, _)| *source)
    }

    pub(crate) fn queue(
        &mut self,
        context: &MetalContext,
        source: GraphTextureSource,
        region: TextureReadbackRegion,
    ) -> Result<TextureReadbackTicket, RendererError> {
        let view = self
            .sources
            .get(&source.resource)
            .filter(|(retained, _)| *retained == source)
            .map(|(_, view)| view.clone())
            .ok_or_else(|| {
                RendererError::InvalidOperation("Unknown or retired graph texture source".into())
            })?;
        let texture = view.image();
        let format = texture.format();
        if format.block_extent() != [1, 1]
            || region.size.width == 0
            || region.size.height == 0
            || region.mip_level as usize >= view.inner.mipmapLevelCount()
            || region.array_layer as usize >= view.inner.arrayLength()
        {
            return Err(RendererError::InvalidOperation(
                "Readback requires an uncompressed, in-bounds image region".into(),
            ));
        }
        let width = (view.inner.width() >> region.mip_level).max(1);
        let height = (view.inner.height() >> region.mip_level).max(1);
        if (region.origin[0] as usize)
            .checked_add(region.size.width as usize)
            .is_none_or(|end| end > width)
            || (region.origin[1] as usize)
                .checked_add(region.size.height as usize)
                .is_none_or(|end| end > height)
        {
            return Err(RendererError::InvalidOperation(
                "Readback region exceeds the exported image".into(),
            ));
        }
        let row_bytes = region.size.width as usize * format.bytes_per_pixel() as usize;
        let row_pitch = row_bytes.next_multiple_of(256);
        let byte_count = row_pitch
            .checked_mul(region.size.height as usize)
            .ok_or_else(|| {
                RendererError::InvalidOperation("Readback byte capacity overflow".into())
            })?;
        let buffer = context.create_buffer(byte_count as u64, true)?;
        let mut command = context.create_command_buffer();
        command
            .inner
            .setLabel(Some(&objc2_foundation::NSString::from_str(&format!(
                "texture_readback.slot.{}.generation.{}",
                source.frame_slot, source.generation
            ))));
        command.begin();
        let mut encoder = command.begin_blit_pass_with_label("exported_texture_readback");
        encoder.copy_texture_region_to_buffer(texture, region, &buffer, row_pitch);
        encoder.end_encoding();
        command.end();
        command.resources.check()?;
        command.submit(context);
        let ticket = TextureReadbackTicket {
            id: crate::renderer::texture_readback::fresh_readback_id(),
            source,
        };
        self.pending.insert(
            ticket.id,
            PendingReadback {
                ticket,
                _source: view,
                command,
                buffer,
                format,
                region,
                row_bytes,
                row_pitch,
            },
        );
        Ok(ticket)
    }

    pub(crate) fn wait_pending(&self) -> Result<(), RendererError> {
        let mut failure = None;
        for pending in self.pending.values() {
            if let Err(error) = pending.command.wait_until_completed()
                && failure.is_none()
            {
                failure = Some(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    pub(crate) fn poll(
        &mut self,
        ticket: TextureReadbackTicket,
    ) -> Result<Option<TextureReadbackData>, RendererError> {
        let pending = self
            .pending
            .get(&ticket.id)
            .filter(|pending| pending.ticket == ticket)
            .ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Unknown or already consumed texture readback ticket".into(),
                )
            })?;
        if !pending.command.completion.is_complete() {
            return Ok(None);
        }
        let pending = self
            .pending
            .remove(&ticket.id)
            .ok_or_else(|| RendererError::InvalidOperation("Readback ticket was retired".into()))?;
        pending.command.wait_until_completed()?;
        let mut bytes = Vec::with_capacity(pending.row_bytes * pending.region.size.height as usize);
        for row in 0..pending.region.size.height as usize {
            let source = unsafe {
                std::slice::from_raw_parts(
                    pending.buffer.map().add(row * pending.row_pitch),
                    pending.row_bytes,
                )
            };
            bytes.extend_from_slice(source);
        }
        Ok(Some(TextureReadbackData {
            format: pending.format,
            size: pending.region.size,
            bytes,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::metal_renderer::MetalRenderer;
    use super::*;
    use crate::GpuRenderer;
    use crate::render_graph::{FrameGraphBuilder, PassType, SimplePass};
    use crate::render_pass::{AttachmentOps, ClearValue};

    #[test]
    fn test_device_drain_completes_pending_texture_readbacks_before_single_poll() {
        let mut renderer = MetalRenderer::new(MetalContext::init_headless().unwrap()).unwrap();
        let mut graph = FrameGraphBuilder::new()
            .export_resource("backbuffer")
            .add_pass(
                SimplePass::new("clear", PassType::Graphics)
                    .without_depth()
                    .write("backbuffer")
                    .attachment(
                        "backbuffer",
                        AttachmentOps::clear(ClearValue::Color([0., 1., 0., 1.])),
                    ),
            )
            .build::<MetalRenderer>()
            .unwrap();
        let frame = super::super::test_support::acquire(&mut renderer, 8);
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        renderer.present(frame).unwrap();
        let source = renderer
            .graph_texture_source(graph.resource_id("backbuffer").unwrap())
            .unwrap();
        let tickets = (0..3)
            .map(|_| {
                renderer
                    .queue_texture_readback(source, TextureReadbackRegion::pixel(4, 4))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        renderer.wait_for_device();
        assert!(
            renderer
                .texture_readbacks
                .pending
                .values()
                .all(|pending| pending.command.completion.is_complete())
        );
        for ticket in tickets {
            let data = renderer
                .poll_texture_readback(ticket)
                .unwrap()
                .expect("device drain completed the readback command");
            assert_eq!(data.bytes, [0, 255, 0, 255]);
        }
        assert!(renderer.texture_readbacks.pending.is_empty());
    }
}
