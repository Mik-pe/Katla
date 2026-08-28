//! Metal shadow pass subsystem for CSM (Cascaded Shadow Maps).

use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLFunction, MTLPixelFormat, MTLRenderCommandEncoder, MTLVertexDescriptor, MTLVertexFormat,
    MTLVertexStepFunction,
};

use crate::backend::command::{
    DepthAttachmentInfo, GpuCommandBuffer, GpuRenderEncoder, IndexType, RenderPassInfo,
    ShaderStages,
};
use crate::error::RendererError;
use crate::handle::ResourceStorage;
use crate::pipeline::CompareOp;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::shadow::{CascadeParams, CascadeShadowMap};
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};

use super::buffer::MetalBuffer;
use super::context::MetalContext;
use super::pipeline::MetalGraphicsPipeline;
use super::texture::MetalTextureView;

const DEFAULT_SHADOW_RESOLUTION: u32 = 2048;

/// Metal shadow subsystem for CSM shadow mapping.
///
/// Cascade math (PSSM splits, light view-projection, texel snapping) is owned
/// by the shared [`CascadeShadowMap`] so Metal and Vulkan consume identical
/// cascade data.
pub(crate) struct MetalShadowSubsystem {
    shadow_map_texture: Option<MetalTextureView>,
    shadow_pipeline: Option<MetalGraphicsPipeline>,
    cascades: CascadeShadowMap,
    shadow_resolution: u32,
}

impl MetalShadowSubsystem {
    pub(crate) fn new() -> Self {
        Self {
            shadow_map_texture: None,
            shadow_pipeline: None,
            cascades: CascadeShadowMap::new(CascadeParams {
                shadow_map_size: DEFAULT_SHADOW_RESOLUTION,
                ..CascadeParams::default()
            }),
            shadow_resolution: DEFAULT_SHADOW_RESOLUTION,
        }
    }

    pub(crate) fn shadow_map_view(&self) -> Option<&MetalTextureView> {
        self.shadow_map_texture.as_ref()
    }

    pub(crate) fn pipeline(&self) -> Option<&MetalGraphicsPipeline> {
        self.shadow_pipeline.as_ref()
    }

    pub(crate) fn cascade_count(&self) -> u32 {
        self.cascades.cascade_count() as u32
    }

    pub(crate) fn shadow_resolution(&self) -> u32 {
        self.shadow_resolution
    }

    /// Create the shadow map depth texture.
    pub(crate) fn create_shadow_map(
        &mut self,
        context: &MetalContext,
    ) -> Result<(), RendererError> {
        let desc = TextureDescriptor::new(
            self.shadow_resolution,
            self.shadow_resolution,
            ImageFormat::D32Sfloat,
        )
        .with_usage(TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED);

        let (_texture, view) = context.create_texture(&desc)?;
        self.shadow_map_texture = Some(view);
        Ok(())
    }

    /// Create the shadow depth-only pipeline.
    pub(crate) fn create_pipeline(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
    ) -> Result<(), RendererError> {
        // Shadow VS fetches only position ([[attribute(0)]], float3) from the
        // interleaved 48-byte mesh vertex. Explicit descriptor — Metal cannot
        // infer the fetch layout reliably for naga-generated attribute indices.
        let vd = MTLVertexDescriptor::new();
        let layouts = vd.layouts();
        let layout = unsafe { layouts.objectAtIndexedSubscript(10) };
        unsafe {
            layout.setStride(48);
            layout.setStepFunction(MTLVertexStepFunction::PerVertex);
            layout.setStepRate(1);
        }
        let attrs = vd.attributes();
        let pos = unsafe { attrs.objectAtIndexedSubscript(0) };
        pos.setFormat(MTLVertexFormat::Float3);
        unsafe {
            pos.setOffset(0);
            pos.setBufferIndex(10);
        }

        let pipeline = context.create_graphics_pipeline_with_vertex_descriptor(
            vertex_function,
            None,
            &[],
            Some(MTLPixelFormat::Depth32Float),
            true,
            CompareOp::Less,
            objc2_metal::MTLCullMode::Front,
            objc2_metal::MTLWinding::Clockwise,
            Some(&vd),
            false,
        )?;

        self.shadow_pipeline = Some(pipeline);
        Ok(())
    }

    /// Create the skinned shadow depth-only pipeline.
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
            Some(MTLPixelFormat::Depth32Float),
            true,
            CompareOp::Less,
            objc2_metal::MTLCullMode::Front,
            objc2_metal::MTLWinding::Clockwise,
            Some(&vd),
            false,
        )?;

        self.shadow_pipeline = Some(pipeline);
        Ok(())
    }

    /// Update shadow cascade view-projection matrices from camera and light.
    pub(crate) fn update_cascades(
        &mut self,
        view_matrix: &[f32; 16],
        proj_matrix: &[f32; 16],
        light_direction: [f32; 3],
    ) {
        self.cascades
            .update(light_direction, view_matrix, proj_matrix);
    }

    /// Per-cascade GPU data in the shared ShadowFrameData layout.
    pub(crate) fn gpu_data(&self) -> crate::shadow::cascade::ShadowFrameData {
        self.cascades.gpu_data()
    }
}

/// Encodes every cascade into the shadow atlas within ONE render pass.
/// Each cascade previously began its own pass with a full-attachment clear,
/// so the last-encoded cascade wiped all earlier quadrants and three of the
/// four cascades always sampled the clear value.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_cascades(
    cmd_buffer: &mut super::command_buffer::MetalCommandBuffer,
    shadow_pipeline: &MetalGraphicsPipeline,
    shadow_map_view: &MetalTextureView,
    shadow_resolution: u32,
    frame_uniform_buffer: &MetalBuffer,
    object_storage_buffer: &MetalBuffer,
    shadow_cascade_buffer: &MetalBuffer,
    buffer_sizes: Option<&MetalBuffer>,
    cascade_count: u32,
    meshes: &ResourceStorage<super::metal_renderer::MetalMesh>,
    materials: &ResourceStorage<super::metal_renderer::MetalMaterial>,
    draw_list: &crate::renderer::types::DrawList,
) {
    let render_pass_info = RenderPassInfo {
        color_attachments: vec![],
        depth_attachment: Some(DepthAttachmentInfo {
            view: shadow_map_view.clone(),
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: ClearValue::DepthStencil {
                depth: 1.0,
                stencil: 0,
            },
            format: ImageFormat::D32Sfloat,
        }),
    };

    let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);

    encoder.bind_graphics_pipeline(shadow_pipeline);

    let stages = ShaderStages::VERTEX;
    encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
    encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);
    encoder.bind_storage_buffer(shadow_cascade_buffer, 0, 2, stages);
    // naga emits runtime-array bounds checks against [[buffer(8)]]; without it
    // every objects[] read clamps and all shadow vertices collapse to origin.
    if let Some(buffer_sizes) = buffer_sizes {
        encoder.bind_storage_buffer(buffer_sizes, 0, 8, stages);
    }

    // Render into each cascade's quadrant of the atlas. The sampling side
    // (cascade_uv_offset_scale) maps cascade i into the quadrant at
    // (col * 0.5, row * 0.5) with row = 1 - i/2 in UV space; Metal viewports
    // are y-down, which matches that layout (cascades 0/1 = uv-top = y-offset 0).
    let quarter = (shadow_resolution / 2) as f32;
    for cascade_index in 0..cascade_count {
        let col = (cascade_index % 2) as f32;
        let row = 1.0 - (cascade_index / 2) as f32;
        encoder.set_viewport(col * quarter, row * quarter, quarter, quarter, 0.0, 1.0);
        encoder.set_scissor(
            (col * quarter) as u32,
            (row * quarter) as u32,
            quarter as u32,
            quarter as u32,
        );

        let shadow_params: [u32; 4] = [cascade_index, 0, 0, 0];
        encoder.set_push_constants(
            bytemuck::cast_slice(&shadow_params),
            3,
            ShaderStages::VERTEX,
        );

        encode_cascade_draws(
            &mut encoder,
            object_storage_buffer,
            meshes,
            materials,
            draw_list,
        );
    }

    encoder.end_encoding();
}

fn encode_cascade_draws(
    encoder: &mut super::render_encoder::MetalRenderEncoder,
    object_storage_buffer: &MetalBuffer,
    meshes: &ResourceStorage<super::metal_renderer::MetalMesh>,
    materials: &ResourceStorage<super::metal_renderer::MetalMaterial>,
    draw_list: &crate::renderer::types::DrawList,
) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_subsystem_creation() {
        let subsystem = MetalShadowSubsystem::new();
        assert!(subsystem.shadow_pipeline.is_none());
        assert!(subsystem.shadow_map_texture.is_none());
        assert_eq!(subsystem.cascade_count(), 4);
    }

    #[test]
    fn test_gpu_data_uses_raw_split_distances() {
        let mut subsystem = MetalShadowSubsystem::new();
        let view = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 5.0, 1.0,
        ];
        // Infinite reverse-Z projection (proj[10] == 0)
        let f = 1.0 / (60.0_f32.to_radians() * 0.5).tan();
        let near = 0.1_f32;
        let proj = [
            f, 0.0, 0.0, 0.0, 0.0, f, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, near, 0.0,
        ];
        subsystem.update_cascades(&view, &proj, [0.3, -1.0, 0.2]);

        let data = subsystem.gpu_data();
        // Splits must be raw view-space distances (shader compares against
        // -(view * world_pos).z), monotonically increasing, in scene range.
        let splits = data.cascades.map(|c| c.split_distance);
        assert!(splits[0] > near, "first split {} not above near", splits[0]);
        for i in 1..splits.len() {
            assert!(
                splits[i] > splits[i - 1],
                "splits not increasing: {splits:?}"
            );
        }
        // light_direction.w carries the cascade count.
        assert_eq!(data.light_direction[3], 4.0);
        // texel_size must be populated (PCF radius collapses to zero otherwise).
        assert!(data.cascades[0].texel_size > 0.0);
    }

    #[test]
    fn test_gpu_data_layout_matches_wgsl() {
        // ShadowFrameData must stay 80B per cascade: mat4 + split + texel + 2 pad.
        assert_eq!(
            std::mem::size_of::<crate::shadow::cascade::ShadowFrameData>(),
            80 * 4 + 16 + 16
        );
    }
}
