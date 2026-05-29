//! Metal depth prepass subsystem.
//!
//! Renders depth-only from the camera's perspective to populate the depth buffer
//! before the main geometry pass for early-Z rejection.

use objc2::runtime::ProtocolObject;
use objc2_metal::MTLFunction;

use crate::backend::command::{
    DepthAttachmentInfo, GpuCommandBuffer, GpuRenderEncoder, IndexType, RenderPassInfo,
    ShaderStages,
};
use crate::error::RendererError;
use crate::handle::ResourceStorage;
use crate::pipeline::CompareOp;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

use super::buffer::MetalBuffer;
use super::context::MetalContext;
use super::metal_renderer::{MetalMaterial, MetalMesh};
use super::pipeline::MetalGraphicsPipeline;
use super::texture::MetalTextureView;

/// Metal depth prepass subsystem.
///
/// Creates a depth-only pipeline that renders all opaque geometry
/// to the depth buffer with reverse-Z depth testing (Greater).
pub(crate) struct MetalDepthPrepass {
    pipeline: Option<MetalGraphicsPipeline>,
    pipeline_skinned: Option<MetalGraphicsPipeline>,
}

impl MetalDepthPrepass {
    pub(crate) fn new() -> Self {
        Self {
            pipeline: None,
            pipeline_skinned: None,
        }
    }

    pub(crate) fn pipeline(&self) -> Option<&MetalGraphicsPipeline> {
        self.pipeline.as_ref()
    }

    pub(crate) fn pipeline_skinned(&self) -> Option<&MetalGraphicsPipeline> {
        self.pipeline_skinned.as_ref()
    }

    /// Create the depth-only prepass pipeline.
    pub(crate) fn create_pipeline(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
    ) -> Result<(), RendererError> {
        let pipeline = context.create_graphics_pipeline(
            vertex_function,
            None,
            &[],
            Some(objc2_metal::MTLPixelFormat::Depth32Float_Stencil8),
            true,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::Back,
            objc2_metal::MTLWinding::Clockwise,
        )?;

        self.pipeline = Some(pipeline);
        Ok(())
    }

    /// Create the skinned depth prepass pipeline.
    pub(crate) fn create_pipeline_skinned(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
    ) -> Result<(), RendererError> {
        let vd = super::context::pbr_skinned_vertex_descriptor();
        let pipeline = context.create_graphics_pipeline_with_vertex_descriptor(
            vertex_function,
            None,
            &[],
            Some(objc2_metal::MTLPixelFormat::Depth32Float_Stencil8),
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
}

/// Render the depth prepass.
///
/// Creates a depth-only render pass and draws all opaque geometry to populate the depth buffer.
/// Switches between non-skinned and skinned pipelines based on draw call skeleton state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_depth_prepass(
    cmd_buffer: &mut super::command_buffer::MetalCommandBuffer,
    depth_pipeline: &MetalGraphicsPipeline,
    depth_pipeline_skinned: Option<&MetalGraphicsPipeline>,
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
        color_attachments: vec![],
        depth_attachment: Some(DepthAttachmentInfo {
            view: depth_view.clone(),
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: ClearValue::DepthStencil {
                depth: 0.0,
                stencil: 0,
            },
            format: ImageFormat::D32SfloatS8Uint,
        }),
    };

    let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);

    encoder.bind_graphics_pipeline(depth_pipeline);
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

        let is_skinned = !draw.skeleton.is_none() && depth_pipeline_skinned.is_some();

        if is_skinned != current_is_skinned {
            if is_skinned {
                encoder.bind_graphics_pipeline(depth_pipeline_skinned.unwrap());
                encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
                encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);
            } else {
                encoder.bind_graphics_pipeline(depth_pipeline);
                encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
                encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);
            }
            current_is_skinned = is_skinned;
        }

        if is_skinned && let Some(skeleton_buf) = skeleton_buffers.get(draw.skeleton.index()) {
            encoder.bind_storage_buffer(skeleton_buf, 0, 2, stages);
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
    fn test_depth_prepass_creation() {
        let prepass = MetalDepthPrepass::new();
        assert!(prepass.pipeline.is_none());
        assert!(prepass.pipeline_skinned.is_none());
    }
}
