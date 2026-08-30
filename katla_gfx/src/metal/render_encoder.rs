use core::ffi::c_void;
use core::ptr::NonNull;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLBuffer, MTLCommandEncoder, MTLIndexType, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderStages, MTLResource, MTLResourceUsage, MTLScissorRect, MTLTexture, MTLViewport,
};

use crate::backend::command::*;

use super::MetalBackend;
use super::buffer::MetalBuffer;
use super::format::to_mtl_index_type;
use super::sampler::MetalSamplerState;
use super::texture::MetalTextureView;

pub(crate) struct MetalRenderEncoder {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>,
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_type: Option<MTLIndexType>,
    index_offset: u64,
}

impl MetalRenderEncoder {
    pub(crate) fn new(inner: Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>) -> Self {
        Self {
            inner,
            index_buffer: None,
            index_type: None,
            index_offset: 0,
        }
    }

    pub(crate) fn use_buffer(
        &self,
        buffer: &ProtocolObject<dyn MTLBuffer>,
        usage: MTLResourceUsage,
        stages: MTLRenderStages,
    ) {
        let resource: &ProtocolObject<dyn MTLResource> = buffer.as_ref();
        self.inner.useResource_usage_stages(resource, usage, stages);
    }

    pub(crate) fn use_texture(
        &self,
        texture: &ProtocolObject<dyn MTLTexture>,
        usage: MTLResourceUsage,
        stages: MTLRenderStages,
    ) {
        let resource: &ProtocolObject<dyn MTLResource> = texture.as_ref();
        self.inner.useResource_usage_stages(resource, usage, stages);
    }

    /// Bind a storage buffer at a byte offset (sub-allocated region binding).
    pub(crate) fn bind_storage_buffer_at_offset_render(
        &self,
        buffer: &MetalBuffer,
        offset: u64,
        group: u32,
        index: u32,
        stages: ShaderStages,
    ) {
        let _ = group;
        if stages.vertex {
            unsafe {
                self.inner.setVertexBuffer_offset_atIndex(
                    Some(&buffer.inner),
                    offset as usize,
                    index as usize,
                );
            }
        }
        if stages.fragment {
            unsafe {
                self.inner.setFragmentBuffer_offset_atIndex(
                    Some(&buffer.inner),
                    offset as usize,
                    index as usize,
                );
            }
        }
    }

    /// Issue an indirect draw: vertex_count/instance_count/first_vertex/
    /// first_instance read from a MTLBuffer written by a compute pass
    /// earlier in the same command buffer.
    pub(crate) fn draw_indirect(&self, indirect_buffer: &MetalBuffer, offset: u64) {
        unsafe {
            self.inner
                .drawPrimitives_indirectBuffer_indirectBufferOffset(
                    objc2_metal::MTLPrimitiveType::Triangle,
                    &indirect_buffer.inner,
                    offset as usize,
                );
        }
    }
}

impl GpuRenderEncoder<MetalBackend> for MetalRenderEncoder {
    fn end_encoding(self) {
        self.inner.endEncoding();
    }

    fn bind_graphics_pipeline(
        &mut self,
        pipeline: &<MetalBackend as crate::backend::traits::GpuBackend>::GraphicsPipeline,
    ) {
        self.inner.setRenderPipelineState(&pipeline.pipeline_state);
        if let Some(ref ds) = pipeline.depth_stencil_state {
            self.inner.setDepthStencilState(Some(ds));
        }
        self.inner.setCullMode(pipeline.cull_mode);
        self.inner.setFrontFacingWinding(pipeline.front_face);
        if let Some((bias, slope, clamp)) = pipeline.depth_bias {
            self.inner.setDepthBias_slopeScale_clamp(bias, slope, clamp);
        }
    }

    fn bind_vertex_buffer(&mut self, buffer: &MetalBuffer, offset: u64, index: u32) {
        unsafe {
            self.inner.setVertexBuffer_offset_atIndex(
                Some(&buffer.inner),
                offset as usize,
                index as usize,
            );
        }
    }

    fn bind_index_buffer(&mut self, buffer: &MetalBuffer, offset: u64, index_type: IndexType) {
        self.index_buffer = Some(buffer.inner.clone());
        self.index_type = Some(to_mtl_index_type(index_type));
        self.index_offset = offset;
    }

    fn bind_storage_buffer(
        &mut self,
        buffer: &MetalBuffer,
        offset: u64,
        index: u32,
        stages: ShaderStages,
    ) {
        unsafe {
            if stages.vertex {
                self.inner.setVertexBuffer_offset_atIndex(
                    Some(&buffer.inner),
                    offset as usize,
                    index as usize,
                );
            }
            if stages.fragment {
                self.inner.setFragmentBuffer_offset_atIndex(
                    Some(&buffer.inner),
                    offset as usize,
                    index as usize,
                );
            }
        }
    }

    fn bind_texture(&mut self, view: &MetalTextureView, index: u32, stages: ShaderStages) {
        unsafe {
            if stages.vertex {
                self.inner
                    .setVertexTexture_atIndex(Some(&view.inner), index as usize);
            }
            if stages.fragment {
                self.inner
                    .setFragmentTexture_atIndex(Some(&view.inner), index as usize);
            }
        }
    }

    fn bind_sampler(&mut self, sampler: &MetalSamplerState, index: u32, stages: ShaderStages) {
        unsafe {
            if stages.vertex {
                self.inner
                    .setVertexSamplerState_atIndex(Some(&sampler.inner), index as usize);
            }
            if stages.fragment {
                self.inner
                    .setFragmentSamplerState_atIndex(Some(&sampler.inner), index as usize);
            }
        }
    }

    fn set_push_constants(&mut self, data: &[u8], index: u32, stages: ShaderStages) {
        unsafe {
            let ptr = NonNull::new(data.as_ptr() as *mut c_void).unwrap();
            let len = data.len();
            if stages.vertex {
                self.inner
                    .setVertexBytes_length_atIndex(ptr, len, index as usize);
            }
            if stages.fragment {
                self.inner
                    .setFragmentBytes_length_atIndex(ptr, len, index as usize);
            }
        }
    }

    fn set_viewport(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        min_depth: f32,
        max_depth: f32,
    ) {
        self.inner.setViewport(MTLViewport {
            originX: x as f64,
            originY: y as f64,
            width: width as f64,
            height: height as f64,
            znear: min_depth as f64,
            zfar: max_depth as f64,
        });
    }

    fn set_scissor(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.inner.setScissorRect(MTLScissorRect {
            x: x as usize,
            y: y as usize,
            width: width as usize,
            height: height as usize,
        });
    }

    fn set_depth_bias(&mut self, bias: f32, slope: f32, clamp: f32) {
        self.inner.setDepthBias_slopeScale_clamp(bias, slope, clamp);
    }

    fn draw(
        &mut self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        _first_instance: u32,
    ) {
        unsafe {
            self.inner
                .drawPrimitives_vertexStart_vertexCount_instanceCount(
                    MTLPrimitiveType::Triangle,
                    first_vertex as usize,
                    vertex_count as usize,
                    instance_count as usize,
                );
        }
    }

    fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        if let (Some(index_buffer), Some(index_type)) = (&self.index_buffer, self.index_type) {
            let index_size = match index_type {
                MTLIndexType::UInt16 => 2u64,
                MTLIndexType::UInt32 => 4u64,
                _ => 2,
            };
            unsafe {
                self.inner.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset_instanceCount_baseVertex_baseInstance(
                    MTLPrimitiveType::Triangle,
                    index_count as usize,
                    index_type,
                    index_buffer,
                    (self.index_offset + first_index as u64 * index_size) as usize,
                    instance_count as usize,
                    vertex_offset as isize,
                    first_instance as usize,
                );
            }
        }
    }

    fn set_stencil_reference_value(&mut self, reference: u32) {
        self.inner
            .setStencilFrontReferenceValue_backReferenceValue(reference, reference);
    }
}
