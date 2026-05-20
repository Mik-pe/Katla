//! Metal outline pass subsystem for stencil-based selection highlighting.
//!
//! Uses a two-pass approach:
//! 1. Stencil mark pass: Render selected objects, writing stencil ref 1
//! 2. Outline draw pass: Render selected objects slightly scaled up, only where stencil != 1

use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCompareFunction, MTLFunction, MTLPixelFormat, MTLStencilOperation};

use crate::backend::command::{
    ColorAttachmentInfo, DepthAttachmentInfo, GpuCommandBuffer, GpuRenderEncoder, IndexType,
    RenderPassInfo, ShaderStages,
};
use crate::error::RendererError;
use crate::handle::ResourceStorage;
use crate::pipeline::CompareOp;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

use super::buffer::MetalBuffer;
use super::context::{MetalContext, StencilFaceOps};
use super::metal_renderer::{MetalMaterial, MetalMesh};
use super::pipeline::MetalGraphicsPipeline;
use super::texture::MetalTextureView;

const DEFAULT_OUTLINE_WIDTH: f32 = 0.004;
const DEFAULT_OUTLINE_COLOR: [f32; 4] = [1.0, 0.55, 0.0, 1.0];
const BASE_HEIGHT: f32 = 1080.0;

/// Push constants for outline rendering.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct OutlinePushConstants {
    pub outline_width: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    pub outline_color: [f32; 4],
}

impl Default for OutlinePushConstants {
    fn default() -> Self {
        Self {
            outline_width: DEFAULT_OUTLINE_WIDTH,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            outline_color: DEFAULT_OUTLINE_COLOR,
        }
    }
}

fn compute_outline_width(viewport_height: f32) -> f32 {
    DEFAULT_OUTLINE_WIDTH * (BASE_HEIGHT / viewport_height)
}

/// Metal outline subsystem for stencil-based selection highlight.
pub(crate) struct MetalOutlineSubsystem {
    stencil_mark_pipeline: Option<MetalGraphicsPipeline>,
    outline_draw_pipeline: Option<MetalGraphicsPipeline>,
    stencil_mark_skinned_pipeline: Option<MetalGraphicsPipeline>,
    outline_draw_skinned_pipeline: Option<MetalGraphicsPipeline>,
}

impl MetalOutlineSubsystem {
    pub(crate) fn new() -> Self {
        Self {
            stencil_mark_pipeline: None,
            outline_draw_pipeline: None,
            stencil_mark_skinned_pipeline: None,
            outline_draw_skinned_pipeline: None,
        }
    }

    pub(crate) fn stencil_mark_pipeline(&self) -> Option<&MetalGraphicsPipeline> {
        self.stencil_mark_pipeline.as_ref()
    }

    pub(crate) fn outline_draw_pipeline(&self) -> Option<&MetalGraphicsPipeline> {
        self.outline_draw_pipeline.as_ref()
    }

    /// Create the stencil mark pipeline.
    ///
    /// Renders selected objects with stencil always pass + replace (writes ref 1).
    /// No color write, depth test enabled but no depth write.
    pub(crate) fn create_stencil_mark_pipeline(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
    ) -> Result<(), RendererError> {
        let stencil_face = StencilFaceOps {
            compare_func: MTLCompareFunction::Always,
            stencil_fail_op: MTLStencilOperation::Keep,
            depth_fail_op: MTLStencilOperation::Keep,
            depth_stencil_pass_op: MTLStencilOperation::Replace,
            read_mask: 0xFF,
            write_mask: 0x01,
        };

        let pipeline = context.create_graphics_pipeline_with_stencil(
            vertex_function,
            None,
            &[MTLPixelFormat::RGBA16Float],
            Some(MTLPixelFormat::Depth32Float_Stencil8),
            false,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::None,
            objc2_metal::MTLWinding::Clockwise,
            stencil_face,
        )?;

        self.stencil_mark_pipeline = Some(pipeline);
        Ok(())
    }

    /// Create the outline draw pipeline.
    ///
    /// Renders selected objects with front-face culling (inverted), stencil compare NotEqual,
    /// meaning only pixels where stencil was NOT written in the mark pass will be drawn.
    /// This creates the outline effect around selected objects.
    pub(crate) fn create_outline_draw_pipeline(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
        fragment_function: &ProtocolObject<dyn MTLFunction>,
    ) -> Result<(), RendererError> {
        let stencil_face = StencilFaceOps {
            compare_func: MTLCompareFunction::NotEqual,
            stencil_fail_op: MTLStencilOperation::Keep,
            depth_fail_op: MTLStencilOperation::Keep,
            depth_stencil_pass_op: MTLStencilOperation::Keep,
            read_mask: 0xFF,
            write_mask: 0x00,
        };

        let pipeline = context.create_graphics_pipeline_with_stencil(
            vertex_function,
            Some(fragment_function),
            &[MTLPixelFormat::RGBA16Float],
            Some(MTLPixelFormat::Depth32Float_Stencil8),
            false,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::Front,
            objc2_metal::MTLWinding::Clockwise,
            stencil_face,
        )?;

        self.outline_draw_pipeline = Some(pipeline);
        Ok(())
    }

    /// Create skinned variant of the stencil mark pipeline.
    pub(crate) fn create_stencil_mark_skinned_pipeline(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
    ) -> Result<(), RendererError> {
        let stencil_face = StencilFaceOps {
            compare_func: MTLCompareFunction::Always,
            stencil_fail_op: MTLStencilOperation::Keep,
            depth_fail_op: MTLStencilOperation::Keep,
            depth_stencil_pass_op: MTLStencilOperation::Replace,
            read_mask: 0xFF,
            write_mask: 0x01,
        };

        let pipeline = context.create_graphics_pipeline_with_stencil(
            vertex_function,
            None,
            &[MTLPixelFormat::RGBA16Float],
            Some(MTLPixelFormat::Depth32Float_Stencil8),
            false,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::None,
            objc2_metal::MTLWinding::Clockwise,
            stencil_face,
        )?;

        self.stencil_mark_skinned_pipeline = Some(pipeline);
        Ok(())
    }

    /// Create skinned variant of the outline draw pipeline.
    pub(crate) fn create_outline_draw_skinned_pipeline(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
        fragment_function: &ProtocolObject<dyn MTLFunction>,
    ) -> Result<(), RendererError> {
        let stencil_face = StencilFaceOps {
            compare_func: MTLCompareFunction::NotEqual,
            stencil_fail_op: MTLStencilOperation::Keep,
            depth_fail_op: MTLStencilOperation::Keep,
            depth_stencil_pass_op: MTLStencilOperation::Keep,
            read_mask: 0xFF,
            write_mask: 0x00,
        };

        let pipeline = context.create_graphics_pipeline_with_stencil(
            vertex_function,
            Some(fragment_function),
            &[MTLPixelFormat::RGBA16Float],
            Some(MTLPixelFormat::Depth32Float_Stencil8),
            false,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::Front,
            objc2_metal::MTLWinding::Clockwise,
            stencil_face,
        )?;

        self.outline_draw_skinned_pipeline = Some(pipeline);
        Ok(())
    }
}

/// Render the stencil mark pass for selected objects.
///
/// Draws selected objects writing stencil ref 1 to mark their silhouette.
pub(crate) fn render_stencil_mark(
    cmd_buffer: &mut super::command_buffer::MetalCommandBuffer,
    stencil_pipeline: &MetalGraphicsPipeline,
    color_view: &MetalTextureView,
    depth_view: &MetalTextureView,
    width: u32,
    height: u32,
    frame_uniform_buffer: &MetalBuffer,
    object_storage_buffer: &MetalBuffer,
    meshes: &ResourceStorage<MetalMesh>,
    materials: &ResourceStorage<MetalMaterial>,
    draw_list: &crate::renderer::types::DrawList,
) {
    let render_pass_info = RenderPassInfo {
        color_attachments: vec![ColorAttachmentInfo {
            view: color_view.clone(),
            load_op: LoadOp::Load,
            store_op: StoreOp::Store,
            clear_value: ClearValue::OPAQUE_BLACK,
        }],
        depth_attachment: Some(DepthAttachmentInfo {
            view: depth_view.clone(),
            load_op: LoadOp::Load,
            store_op: StoreOp::Store,
            clear_value: ClearValue::DEFAULT_DEPTH,
            format: ImageFormat::D32SfloatS8Uint,
        }),
    };

    let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);

    encoder.bind_graphics_pipeline(stencil_pipeline);
    encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
    encoder.set_stencil_reference_value(1);

    let stages = ShaderStages::VERTEX_FRAGMENT;
    encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
    encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);

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

        encoder.bind_vertex_buffer(&mesh.vertex_buffer, 0, 10);
        encoder.bind_index_buffer(&mesh.index_buffer, 0, IndexType::Uint32);
        encoder.draw_indexed(mesh.index_count, 1, 0, 0, draw.instance_index);
    }

    encoder.end_encoding();
}

/// Render the outline draw pass.
///
/// Draws selected objects slightly scaled up, only where stencil != 1,
/// creating the outline effect around selected objects.
pub(crate) fn render_outline(
    cmd_buffer: &mut super::command_buffer::MetalCommandBuffer,
    outline_pipeline: &MetalGraphicsPipeline,
    color_view: &MetalTextureView,
    depth_view: &MetalTextureView,
    width: u32,
    height: u32,
    frame_uniform_buffer: &MetalBuffer,
    object_storage_buffer: &MetalBuffer,
    meshes: &ResourceStorage<MetalMesh>,
    materials: &ResourceStorage<MetalMaterial>,
    draw_list: &crate::renderer::types::DrawList,
) {
    let outline_width = compute_outline_width(height as f32);
    let push_constants = OutlinePushConstants {
        outline_width,
        ..OutlinePushConstants::default()
    };

    let render_pass_info = RenderPassInfo {
        color_attachments: vec![ColorAttachmentInfo {
            view: color_view.clone(),
            load_op: LoadOp::Load,
            store_op: StoreOp::Store,
            clear_value: ClearValue::OPAQUE_BLACK,
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

    encoder.bind_graphics_pipeline(outline_pipeline);
    encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
    encoder.set_stencil_reference_value(1);

    let stages = ShaderStages::VERTEX_FRAGMENT;
    encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
    encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);

    encoder.set_push_constants(
        bytemuck::cast_slice(&[push_constants]),
        2,
        ShaderStages::VERTEX_FRAGMENT,
    );

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
    fn test_outline_subsystem_creation() {
        let subsystem = MetalOutlineSubsystem::new();
        assert!(subsystem.stencil_mark_pipeline.is_none());
        assert!(subsystem.outline_draw_pipeline.is_none());
    }

    #[test]
    fn test_outline_push_constants_default() {
        let params = OutlinePushConstants::default();
        assert!((params.outline_width - 0.004).abs() < f32::EPSILON);
        assert!((params.outline_color[0] - 1.0).abs() < f32::EPSILON);
        assert!((params.outline_color[1] - 0.55).abs() < f32::EPSILON);
    }
}
