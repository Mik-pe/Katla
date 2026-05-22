//! Metal GPU picking subsystem.
//!
//! Renders instance indices to an R32Uint texture, then reads back a single
//! pixel via a blit encoder + Shared buffer for CPU-side entity resolution.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandEncoder, MTLOrigin, MTLPixelFormat, MTLSize,
};

use crate::backend::command::{
    ColorAttachmentInfo, DepthAttachmentInfo, GpuCommandBuffer, GpuRenderEncoder, IndexType,
    RenderPassInfo, ShaderStages,
};
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::handle::ResourceStorage;
use crate::pipeline::CompareOp;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};

use super::buffer::MetalBuffer;
use super::context::MetalContext;
use super::metal_renderer::{MetalMaterial, MetalMesh};
use super::pipeline::MetalGraphicsPipeline;
use super::texture::MetalTextureView;

/// Pending picking readback on Metal.
struct PendingPick {
    /// Frame number when the pick was triggered.
    frame: usize,
    /// Command buffer used for the blit copy.
    command_buffer: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    /// Shared buffer containing the single u32 pixel readback.
    readback_buffer: MetalBuffer,
    /// Whether the result has been consumed.
    resolved: bool,
}

/// Metal picking subsystem.
///
/// Owns the object-ID texture, picking pipelines, and readback state.
pub(crate) struct MetalPickingSubsystem {
    /// R32Uint texture for rendering object IDs.
    object_id_texture: Option<MetalTextureView>,
    /// Picking pipeline for static meshes.
    pipeline: Option<MetalGraphicsPipeline>,
    /// Picking pipeline for skinned meshes.
    pipeline_skinned: Option<MetalGraphicsPipeline>,
    /// Pending pixel readback.
    pending: Option<PendingPick>,
    /// Cached texture dimensions.
    texture_width: u32,
    texture_height: u32,
}

impl MetalPickingSubsystem {
    pub(crate) fn new() -> Self {
        Self {
            object_id_texture: None,
            pipeline: None,
            pipeline_skinned: None,
            pending: None,
            texture_width: 0,
            texture_height: 0,
        }
    }

    pub(crate) fn pipeline(&self) -> Option<&MetalGraphicsPipeline> {
        self.pipeline.as_ref()
    }

    pub(crate) fn pipeline_skinned(&self) -> Option<&MetalGraphicsPipeline> {
        self.pipeline_skinned.as_ref()
    }

    pub(crate) fn object_id_texture(&self) -> Option<&MetalTextureView> {
        self.object_id_texture.as_ref()
    }

    /// Create or recreate the object-ID texture.
    pub(crate) fn resize(
        &mut self,
        context: &MetalContext,
        width: u32,
        height: u32,
    ) -> Result<(), RendererError> {
        if width == self.texture_width && height == self.texture_height {
            return Ok(());
        }

        let desc = TextureDescriptor::new(width, height, ImageFormat::R32Uint)
            .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);

        let (_texture, view) = context.create_texture(&desc)?;

        self.object_id_texture = Some(view);
        self.texture_width = width;
        self.texture_height = height;
        Ok(())
    }

    /// Create the static picking pipeline.
    pub(crate) fn create_pipeline(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn objc2_metal::MTLFunction>,
    ) -> Result<(), RendererError> {
        let pipeline = context.create_graphics_pipeline(
            vertex_function,
            None,
            &[MTLPixelFormat::R32Uint],
            Some(MTLPixelFormat::Depth32Float_Stencil8),
            true,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::Back,
            objc2_metal::MTLWinding::Clockwise,
        )?;

        self.pipeline = Some(pipeline);
        Ok(())
    }

    /// Create the skinned picking pipeline.
    pub(crate) fn create_pipeline_skinned(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn objc2_metal::MTLFunction>,
    ) -> Result<(), RendererError> {
        let vd = super::context::pbr_skinned_vertex_descriptor();
        let pipeline = context.create_graphics_pipeline_with_vertex_descriptor(
            vertex_function,
            None,
            &[MTLPixelFormat::R32Uint],
            Some(MTLPixelFormat::Depth32Float_Stencil8),
            true,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::Back,
            objc2_metal::MTLWinding::Clockwise,
            Some(&vd),
            false,
        )?;

        self.pipeline_skinned = Some(pipeline);
        Ok(())
    }

    /// Queue a single-pixel readback from the object-ID texture.
    pub(crate) fn queue_picking_readback(
        &mut self,
        context: &MetalContext,
        frame: usize,
        x: u32,
        y: u32,
    ) -> Result<(), RendererError> {
        // Check if previous pick is still pending (not yet resolved)
        if self.pending.is_some() {
            // Resolve the old one first — non-blocking check
            let completed = self
                .pending
                .as_ref()
                .map(|p| {
                    p.command_buffer.status() == objc2_metal::MTLCommandBufferStatus::Completed
                })
                .unwrap_or(true);
            if !completed {
                log::debug!("Previous picking readback still in flight, skipping new pick");
                return Ok(());
            }
            // Consume and discard old result
            self.pending = None;
        }

        let id_texture = self.object_id_texture.as_ref().ok_or_else(|| {
            RendererError::InvalidOperation("Object-ID texture not created".into())
        })?;

        // Create a small Shared buffer for the 4-byte pixel readback
        let readback_buffer = context.create_buffer(4, true).map_err(|e| {
            RendererError::InvalidOperation(format!(
                "Failed to create picking readback buffer: {}",
                e
            ))
        })?;

        let cmd_buffer = context.create_command_buffer();
        let label = NSString::from_str("picking_readback");
        unsafe { cmd_buffer.inner.setLabel(Some(&label)) };

        // Use blit encoder to copy a single pixel from GPU-private texture to Shared buffer
        let blit_encoder = cmd_buffer
            .inner
            .blitCommandEncoder()
            .expect("Failed to create blit encoder for picking readback");

        unsafe {
            blit_encoder.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                &id_texture.inner,
                0,
                0,
                MTLOrigin {
                    x: x as usize,
                    y: y as usize,
                    z: 0,
                },
                MTLSize {
                    width: 1,
                    height: 1,
                    depth: 1,
                },
                &readback_buffer.inner,
                0,
                4,
                4,
            );
        }

        blit_encoder.endEncoding();
        cmd_buffer.inner.commit();

        self.pending = Some(PendingPick {
            frame,
            command_buffer: cmd_buffer.inner,
            readback_buffer,
            resolved: false,
        });

        Ok(())
    }

    /// Check if a pending picking readback is complete.
    ///
    /// Returns `Some((frame, instance_index))` if ready, `None` if not.
    pub(crate) fn check_picking_readback(&mut self) -> Option<(usize, u32)> {
        let pending = self.pending.as_mut()?;

        if pending.resolved {
            return None;
        }

        let status = pending.command_buffer.status();
        if status != objc2_metal::MTLCommandBufferStatus::Completed {
            return None;
        }

        // Read the u32 from the shared buffer
        let ptr = pending.readback_buffer.map() as *const u32;
        let value = unsafe { std::ptr::read(ptr) };
        pending.readback_buffer.unmap();

        let frame = pending.frame;
        pending.resolved = true;
        self.pending = None;

        Some((frame, value))
    }

    pub(crate) fn has_pending_readback(&self) -> bool {
        self.pending.is_some()
    }
}

/// Render the object-ID picking pass.
///
/// Draws all objects encoding instance_index + 1 into the R32Uint texture.
/// Reuses depth from the depth prepass for correct occlusion.
pub(crate) fn render_object_id_pass(
    cmd_buffer: &mut super::command_buffer::MetalCommandBuffer,
    picking_pipeline: &MetalGraphicsPipeline,
    picking_skinned_pipeline: Option<&MetalGraphicsPipeline>,
    object_id_view: &MetalTextureView,
    depth_view: &MetalTextureView,
    width: u32,
    height: u32,
    frame_uniform_buffer: &MetalBuffer,
    object_storage_buffer: &MetalBuffer,
    meshes: &ResourceStorage<MetalMesh>,
    materials: &ResourceStorage<MetalMaterial>,
    draw_list: &crate::renderer::types::DrawList,
    skeleton_buffers: &ResourceStorage<MetalBuffer>,
) {
    let render_pass_info = RenderPassInfo {
        color_attachments: vec![ColorAttachmentInfo {
            view: object_id_view.clone(),
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: ClearValue::TRANSPARENT_BLACK,
        }],
        depth_attachment: Some(DepthAttachmentInfo {
            view: depth_view.clone(),
            load_op: LoadOp::Load,
            store_op: StoreOp::DontCare,
            clear_value: ClearValue::DEFAULT_DEPTH,
            format: ImageFormat::D32SfloatS8Uint,
        }),
    };

    let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);

    encoder.bind_graphics_pipeline(picking_pipeline);
    encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);

    let stages = ShaderStages::VERTEX_FRAGMENT;
    encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
    encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);

    let mut current_is_skinned = false;

    for draw in &draw_list.draws {
        let Some(mesh) = meshes.get(draw.mesh.index()) else {
            continue;
        };
        let Some(material) = materials.get(draw.material.index()) else {
            continue;
        };
        let Some(ref _pipeline) = material.pipeline else {
            continue;
        };

        let is_skinned = !draw.skeleton.is_none() && picking_skinned_pipeline.is_some();

        if is_skinned != current_is_skinned {
            if is_skinned {
                encoder.bind_graphics_pipeline(picking_skinned_pipeline.unwrap());
                encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
                encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);
            } else {
                encoder.bind_graphics_pipeline(picking_pipeline);
                encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
                encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);
            }
            current_is_skinned = is_skinned;
        }

        if is_skinned {
            if let Some(skeleton_buf) = skeleton_buffers.get(draw.skeleton.index()) {
                encoder.bind_storage_buffer(skeleton_buf, 0, 2, stages);
            }
        }

        encoder.bind_vertex_buffer(&mesh.vertex_buffer, 0, 10);
        encoder.bind_index_buffer(&mesh.index_buffer, 0, IndexType::Uint32);
        encoder.draw_indexed(mesh.index_count, 1, 0, 0, draw.instance_index);
    }

    encoder.end_encoding();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_picking_subsystem_creation() {
        let subsystem = MetalPickingSubsystem::new();
        assert!(subsystem.pipeline.is_none());
        assert!(subsystem.pipeline_skinned.is_none());
        assert!(subsystem.object_id_texture.is_none());
        assert!(!subsystem.has_pending_readback());
    }
}
