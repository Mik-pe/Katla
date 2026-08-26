//! Metal depth prepass subsystem.
//!
//! Renders depth-only from the camera's perspective to populate the depth buffer
//! before the main geometry pass for early-Z rejection.

use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLFunction, MTLRenderCommandEncoder};

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
    pipeline_billboard: Option<MetalGraphicsPipeline>,
}

impl MetalDepthPrepass {
    pub(crate) fn new() -> Self {
        Self {
            pipeline: None,
            pipeline_skinned: None,
            pipeline_billboard: None,
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

    /// Create the billboard depth prepass pipeline.
    ///
    /// Uses the PBR vertex descriptor with an alpha-discard fragment so
    /// transparent billboard texels never write depth. Double-sided, no
    /// culling, writes depth.
    pub(crate) fn create_pipeline_billboard(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
        fragment_function: Option<&ProtocolObject<dyn MTLFunction>>,
    ) -> Result<(), RendererError> {
        let pipeline = context.create_graphics_pipeline(
            vertex_function,
            fragment_function,
            &[],
            Some(objc2_metal::MTLPixelFormat::Depth32Float_Stencil8),
            true,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::None,
            objc2_metal::MTLWinding::Clockwise,
        )?;

        self.pipeline_billboard = Some(pipeline);
        Ok(())
    }

    pub(crate) fn pipeline_billboard(&self) -> Option<&MetalGraphicsPipeline> {
        self.pipeline_billboard.as_ref()
    }
}

/// Render the depth prepass.
///
/// Creates a depth-only render pass and draws all opaque geometry to populate the depth buffer.
/// Switches between non-skinned, skinned, and billboard pipelines based on draw call properties.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_depth_prepass(
    cmd_buffer: &mut super::command_buffer::MetalCommandBuffer,
    depth_pipeline: &MetalGraphicsPipeline,
    depth_pipeline_skinned: Option<&MetalGraphicsPipeline>,
    depth_pipeline_billboard: Option<&MetalGraphicsPipeline>,
    depth_view: &MetalTextureView,
    width: u32,
    height: u32,
    frame_uniform_buffer: &MetalBuffer,
    object_storage_buffer: &MetalBuffer,
    meshes: &ResourceStorage<MetalMesh>,
    materials: &ResourceStorage<MetalMaterial>,
    draw_list: &crate::renderer::types::DrawList,
    skeleton_buffers: &ResourceStorage<MetalBuffer>,
    bindless_argument_buffer: Option<&objc2::runtime::ProtocolObject<dyn objc2_metal::MTLBuffer>>,
    shared_sampler: Option<&super::sampler::MetalSamplerState>,
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

    /// Pipeline variant currently bound.
    #[derive(Clone, Copy, PartialEq)]
    enum PipelineVariant {
        Regular,
        Skinned,
        Billboard,
    }

    let mut current_variant = PipelineVariant::Regular;

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
        let is_billboard = draw.is_billboard && depth_pipeline_billboard.is_some();

        let target_variant = if is_skinned {
            PipelineVariant::Skinned
        } else if is_billboard {
            PipelineVariant::Billboard
        } else {
            PipelineVariant::Regular
        };

        if target_variant != current_variant {
            let (pipeline, need_rebind) = match target_variant {
                PipelineVariant::Skinned => (depth_pipeline_skinned.unwrap(), true),
                PipelineVariant::Billboard => (depth_pipeline_billboard.unwrap(), true),
                PipelineVariant::Regular => (depth_pipeline, false),
            };
            encoder.bind_graphics_pipeline(pipeline);
            if need_rebind {
                encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
                encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);
                // The billboard fragment samples the bindless icon texture to
                // discard transparent texels before depth is written.
                if target_variant == PipelineVariant::Billboard {
                    if let Some(argument_buffer) = bindless_argument_buffer {
                        unsafe {
                            encoder.inner.setVertexBuffer_offset_atIndex(
                                Some(argument_buffer),
                                0,
                                9,
                            );
                            encoder.inner.setFragmentBuffer_offset_atIndex(
                                Some(argument_buffer),
                                0,
                                9,
                            );
                        }
                        encoder.use_buffer(
                            argument_buffer,
                            objc2_metal::MTLResourceUsage::Read,
                            objc2_metal::MTLRenderStages::Fragment,
                        );
                    }
                    if let Some(sampler) = shared_sampler {
                        unsafe {
                            encoder
                                .inner
                                .setFragmentSamplerState_atIndex(Some(&sampler.inner), 0);
                        }
                    }
                }
            }
            current_variant = target_variant;
        }

        if is_skinned && let Some(skeleton_buf) = skeleton_buffers.get(draw.skeleton.index()) {
            encoder.bind_storage_buffer(skeleton_buf, 0, 2, stages);
        }

        encoder.bind_vertex_buffer(&mesh.vertex_buffer, 0, 10);
        encoder.bind_index_buffer(&mesh.index_buffer, 0, IndexType::Uint32);

        // Metal's instance_id starts from 0 regardless of baseInstance,
        // so rebind the object buffer with an offset so objects[0] maps
        // to the correct per-object data.
        let object_offset =
            draw.instance_index as usize * super::metal_renderer::OBJECT_UNIFORM_SIZE as usize;
        unsafe {
            encoder.inner.setVertexBuffer_offset_atIndex(
                Some(&object_storage_buffer.inner),
                object_offset,
                1,
            );
        }

        encoder.draw_indexed(mesh.index_count, 1, 0, 0, 0);
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
        assert!(prepass.pipeline_billboard.is_none());
    }
}
