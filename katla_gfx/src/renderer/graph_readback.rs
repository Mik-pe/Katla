use std::rc::Rc;

use ash::vk;

use super::VulkanRenderer;
use super::texture_readback::{
    GraphTextureSource, TextureReadbackData, TextureReadbackRegion, TextureReadbackTicket,
};
use crate::render_graph::transient_buffer::VulkanGraphBuffer;
use crate::render_graph::{
    BufferDesc, BufferMemoryPolicy, BufferUsages, FrameGraph, ResourceId, TransientTexture,
};
use crate::vulkan::commandbuffer::CommandBuffer;
use crate::{RendererError, Size2D};

#[derive(Clone)]
pub(crate) struct VulkanTextureExport {
    pub(crate) source: GraphTextureSource,
    image: vk::Image,
    format: crate::texture::ImageFormat,
    extent: Size2D,
    layout: vk::ImageLayout,
    retained: Option<TransientTexture>,
    retained_import: Option<Rc<crate::vulkan::texture::Texture>>,
}

impl VulkanTextureExport {
    pub(crate) fn owns_image(&self) -> bool {
        self.retained.is_some() || self.retained_import.is_some()
    }
}

pub(crate) struct VulkanTextureReadback {
    ticket: TextureReadbackTicket,
    size: Size2D,
    source: VulkanTextureExport,
    buffer: VulkanGraphBuffer,
    _command: CommandBuffer,
    fence: vk::Fence,
    context: Rc<crate::vulkan::context::VulkanContext>,
}

impl Drop for VulkanTextureReadback {
    fn drop(&mut self) {
        unsafe {
            let _ = self
                .context
                .device
                .wait_for_fences(&[self.fence], true, u64::MAX);
            self.context.device.destroy_fence(self.fence, None);
        }
    }
}

impl VulkanRenderer {
    pub(crate) fn prepare_texture_exports(&mut self, graph: &FrameGraph<Self>, image_index: usize) {
        self.pending_texture_exports.clear();
        for &resource in &graph.exported_resources {
            let mut retained_import = None;
            let (image, format, extent, layout, retained) =
                if graph.resource_id(crate::render_graph::BACKBUFFER_NAME) == Some(resource) {
                    (
                        self.frame_context.swapchain_images[image_index].vk(),
                        crate::texture::ImageFormat::B8G8R8A8Srgb,
                        self.swapchain_extent(),
                        if self.frame_context.swapchain.is_some() {
                            vk::ImageLayout::PRESENT_SRC_KHR
                        } else {
                            vk::ImageLayout::TRANSFER_SRC_OPTIMAL
                        },
                        None,
                    )
                } else if let Some(texture) =
                    graph.transient_texture_by_id(resource, self.current_frame())
                {
                    let Ok(format) = texture.format.try_into() else {
                        continue;
                    };
                    (
                        texture.image,
                        format,
                        Size2D::new(texture.extent.width, texture.extent.height),
                        texture.current_layout(),
                        Some(texture.clone()),
                    )
                } else if let Some(texture) = graph
                    .imported_images
                    .get(&resource)
                    .and_then(|handle| self.texture_manager.get_texture_rc(*handle))
                {
                    let final_state = graph
                        .final_image_sync_ops()
                        .iter()
                        .rev()
                        .find(|op| op.resource == resource)
                        .map(|op| op.after)
                        .or_else(|| {
                            graph.execution_order().iter().rev().find_map(|&pass| {
                                graph
                                    .image_sync_ops(pass)
                                    .iter()
                                    .rev()
                                    .find(|op| op.resource == resource)
                                    .map(|op| op.after)
                            })
                        });
                    let layout = final_state
                        .map(crate::render_graph::state_layout)
                        .unwrap_or(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
                    let resolved = (
                        texture.image().vk(),
                        texture.format(),
                        Size2D::new(texture.width, texture.height),
                        layout,
                        None,
                    );
                    retained_import = Some(texture);
                    resolved
                } else {
                    continue;
                };
            let source = GraphTextureSource {
                id: super::texture_readback::fresh_readback_id(),
                resource,
                frame_slot: self.current_frame(),
                generation: self.frame_generation.saturating_sub(1),
                submission: self.swap_data.frame_counter() + 1,
            };
            self.pending_texture_exports.push(VulkanTextureExport {
                source,
                image,
                format,
                extent,
                layout,
                retained,
                retained_import,
            });
        }
    }

    pub(crate) fn commit_texture_exports(&mut self) {
        for export in self.pending_texture_exports.drain(..) {
            self.committed_texture_exports
                .retain(|_, previous| previous.image != export.image);
            self.committed_texture_exports
                .insert(export.source.resource, export);
        }
    }

    pub(crate) fn graph_texture_source(&self, resource: ResourceId) -> Option<GraphTextureSource> {
        self.committed_texture_exports
            .get(&resource)
            .map(|export| export.source)
    }

    pub(crate) fn queue_texture_readback(
        &mut self,
        source: GraphTextureSource,
        region: TextureReadbackRegion,
    ) -> Result<TextureReadbackTicket, RendererError> {
        let export = self
            .committed_texture_exports
            .get(&source.resource)
            .filter(|export| export.source == source)
            .ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Readback source is stale, uncommitted or belongs to another renderer".into(),
                )
            })?
            .clone();
        if export.retained.is_none()
            && export.retained_import.is_none()
            && !self
                .frame_context
                .swapchain_images
                .iter()
                .any(|image| image.vk() == export.image)
        {
            return Err(RendererError::InvalidOperation(
                "Readback output surface has been replaced".into(),
            ));
        }
        if region.mip_level != 0
            || region.array_layer != 0
            || export.format.is_depth_stencil()
            || export.format.block_extent() != [1, 1]
        {
            return Err(RendererError::UnsupportedFeature("Vulkan graph readback supports an uncompressed color region in its single mip and layer".into()));
        }
        if region.size.width == 0
            || region.size.height == 0
            || region.origin[0]
                .checked_add(region.size.width)
                .is_none_or(|end| end > export.extent.width)
            || region.origin[1]
                .checked_add(region.size.height)
                .is_none_or(|end| end > export.extent.height)
        {
            return Err(RendererError::InvalidOperation(
                "Readback region exceeds its retained image".into(),
            ));
        }
        let bytes = u64::from(region.size.width)
            .checked_mul(u64::from(region.size.height))
            .and_then(|pixels| pixels.checked_mul(u64::from(export.format.bytes_per_pixel())))
            .ok_or_else(|| {
                RendererError::InvalidOperation("Readback byte size overflows".into())
            })?;
        let desc = BufferDesc::new(
            bytes,
            BufferUsages::READBACK | BufferUsages::TRANSFER_DESTINATION,
            BufferMemoryPolicy::Readback,
        );
        let buffer =
            <Self as crate::render_graph::RenderGraphBackend>::create_transient_buffer(self, desc)
                .map_err(RendererError::RenderGraphError)?;
        let command = CommandBuffer::new(&self.context.gfx_cmdpool)?;
        command.begin_single_time_command()?;
        crate::barrier::ImageBarrier::transition(
            &command.vk_command_buffer(),
            &self.context.device,
            export.image,
            export.layout,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let copy = vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D {
                x: region.origin[0] as i32,
                y: region.origin[1] as i32,
                z: 0,
            })
            .image_extent(vk::Extent3D {
                width: region.size.width,
                height: region.size.height,
                depth: 1,
            });
        unsafe {
            self.context.device.cmd_copy_image_to_buffer(
                command.vk_command_buffer(),
                export.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                buffer.vk_buffer(),
                &[copy],
            );
        }
        let host_barrier = vk::BufferMemoryBarrier2::default()
            .buffer(buffer.vk_buffer())
            .offset(0)
            .size(bytes)
            .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_access_mask(vk::AccessFlags2::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
        unsafe {
            self.context.device.cmd_pipeline_barrier2(
                command.vk_command_buffer(),
                &vk::DependencyInfo::default().buffer_memory_barriers(&[host_barrier]),
            );
        }
        crate::barrier::ImageBarrier::transition(
            &command.vk_command_buffer(),
            &self.context.device,
            export.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            export.layout,
        );
        command.end_single_time_command()?;
        let fence = unsafe {
            self.context
                .device
                .create_fence(&vk::FenceCreateInfo::default(), None)
        }
        .map_err(|error| {
            RendererError::VulkanError("Readback fence allocation failed".into(), error)
        })?;
        if let Err(error) = self.context.gfx_queue.submit(&[&command], &[], &[], fence) {
            unsafe {
                self.context.device.destroy_fence(fence, None);
            }
            return Err(error);
        }
        let ticket = TextureReadbackTicket {
            id: super::texture_readback::fresh_readback_id(),
            source,
        };
        self.texture_readbacks.insert(
            ticket.id,
            VulkanTextureReadback {
                ticket,
                size: region.size,
                source: export,
                buffer,
                _command: command,
                fence,
                context: self.context.clone(),
            },
        );
        Ok(ticket)
    }

    pub(crate) fn poll_texture_readback(
        &mut self,
        ticket: TextureReadbackTicket,
    ) -> Result<Option<TextureReadbackData>, RendererError> {
        let readback = self
            .texture_readbacks
            .get(&ticket.id)
            .filter(|readback| readback.ticket == ticket)
            .ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Unknown, forged or already consumed readback ticket".into(),
                )
            })?;
        if !unsafe { self.context.device.get_fence_status(readback.fence) }.map_err(|error| {
            RendererError::VulkanError("Readback completion failed".into(), error)
        })? {
            return Ok(None);
        }
        let bytes = readback
            .buffer
            .read_range(crate::render_graph::BufferByteRange::new(
                0,
                readback.buffer.size(),
            ))?;
        let data = TextureReadbackData {
            format: readback.source.format,
            size: readback.size,
            bytes,
        };
        self.texture_readbacks.remove(&ticket.id);
        Ok(Some(data))
    }
}
