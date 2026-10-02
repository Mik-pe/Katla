use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTL4ArgumentTable, MTL4CommandBuffer, MTL4CommandEncoder, MTL4RenderCommandEncoder, MTLBuffer,
    MTLIndexType, MTLPrimitiveType, MTLRenderStages, MTLResourceUsage, MTLSamplerState,
    MTLScissorRect, MTLTexture, MTLViewport,
};

use crate::backend::command::*;
use crate::backend::resource::GpuBuffer;

use super::MetalBackend;
use super::buffer::MetalBuffer;
use super::format::to_mtl_index_type;
use super::sampler::MetalSamplerState;
use super::texture::MetalTextureView;

pub(crate) struct MetalRenderEncoder {
    pub(crate) inner: Retained<ProtocolObject<dyn MTL4RenderCommandEncoder>>,
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_type: Option<MTLIndexType>,
    index_offset: u64,
    vertex_state: super::argument_state::ArgumentState,
    fragment_state: super::argument_state::ArgumentState,
    vertex_layout: Option<super::binding_schema::ArgumentTableLayout>,
    fragment_layout: Option<super::binding_schema::ArgumentTableLayout>,
    resources: std::rc::Rc<super::encoding_resources::EncodingResources>,
    vertex_table: Retained<ProtocolObject<dyn MTL4ArgumentTable>>,
    fragment_table: Retained<ProtocolObject<dyn MTL4ArgumentTable>>,
}

impl MetalRenderEncoder {
    pub(crate) fn new(
        inner: Retained<ProtocolObject<dyn MTL4RenderCommandEncoder>>,
        resources: std::rc::Rc<super::encoding_resources::EncodingResources>,
    ) -> Self {
        let vertex_table = resources.argument_table("vertex arguments");
        let fragment_table = resources.argument_table("fragment arguments");
        inner.setArgumentTable_atStages(&vertex_table, MTLRenderStages::Vertex);
        inner.setArgumentTable_atStages(&fragment_table, MTLRenderStages::Fragment);
        Self {
            inner,
            resources,
            vertex_table,
            fragment_table,
            index_buffer: None,
            index_type: None,
            index_offset: 0,
            vertex_state: Default::default(),
            fragment_state: Default::default(),
            vertex_layout: None,
            fragment_layout: None,
        }
    }

    pub(crate) fn use_buffer(
        &self,
        buffer: &ProtocolObject<dyn MTLBuffer>,
        _usage: MTLResourceUsage,
        _stages: MTLRenderStages,
    ) {
        self.resources
            .residency
            .add_buffer(buffer)
            .expect("Metal buffer residency");
    }

    pub(crate) fn use_texture(
        &self,
        texture: &ProtocolObject<dyn MTLTexture>,
        _usage: MTLResourceUsage,
        _stages: MTLRenderStages,
    ) {
        self.resources
            .residency
            .add_texture(texture)
            .expect("Metal texture residency");
    }

    pub(crate) fn bind_native_buffer(
        &self,
        buffer: &ProtocolObject<dyn MTLBuffer>,
        offset: u64,
        index: u32,
        stages: ShaderStages,
    ) {
        let size = (buffer.length() as u64).saturating_sub(offset);
        if stages.vertex {
            self.vertex_state.buffer(index as usize, size);
        }
        if stages.fragment {
            self.fragment_state.buffer(index as usize, size);
        }
        self.use_buffer(
            buffer,
            MTLResourceUsage::Read,
            MTLRenderStages::Vertex | MTLRenderStages::Fragment,
        );
        unsafe {
            if stages.vertex {
                self.vertex_table
                    .setAddress_atIndex(buffer.gpuAddress() + offset, index as usize);
            }
            if stages.fragment {
                self.fragment_table
                    .setAddress_atIndex(buffer.gpuAddress() + offset, index as usize);
            }
        }
    }

    pub(crate) fn bind_native_texture(
        &self,
        texture: &ProtocolObject<dyn MTLTexture>,
        index: u32,
        stages: ShaderStages,
    ) {
        if stages.vertex {
            self.vertex_state.texture(index as usize);
        }
        if stages.fragment {
            self.fragment_state.texture(index as usize);
        }
        self.use_texture(
            texture,
            MTLResourceUsage::Read,
            MTLRenderStages::Vertex | MTLRenderStages::Fragment,
        );
        unsafe {
            if stages.vertex {
                self.vertex_table
                    .setTexture_atIndex(texture.gpuResourceID(), index as usize);
            }
            if stages.fragment {
                self.fragment_table
                    .setTexture_atIndex(texture.gpuResourceID(), index as usize);
            }
        }
    }

    pub(crate) fn bind_native_sampler(
        &self,
        sampler: &ProtocolObject<dyn MTLSamplerState>,
        index: u32,
        stages: ShaderStages,
    ) {
        if stages.vertex {
            self.vertex_state.sampler(index as usize);
        }
        if stages.fragment {
            self.fragment_state.sampler(index as usize);
        }
        self.resources.retain_sampler(sampler);
        unsafe {
            if stages.vertex {
                self.vertex_table
                    .setSamplerState_atIndex(sampler.gpuResourceID(), index as usize);
            }
            if stages.fragment {
                self.fragment_table
                    .setSamplerState_atIndex(sampler.gpuResourceID(), index as usize);
            }
        }
    }

    fn prepare_arguments(&self) -> bool {
        for (state, layout, table) in [
            (
                &self.vertex_state,
                self.vertex_layout.as_ref(),
                &self.vertex_table,
            ),
            (
                &self.fragment_state,
                self.fragment_layout.as_ref(),
                &self.fragment_table,
            ),
        ] {
            let Some(layout) = layout else {
                continue;
            };
            if let Err(error) = state.validate(layout) {
                self.resources.fail(error);
                return false;
            }
            if let Some(index) = layout.sizes_buffer {
                let words = state.sizes(layout);
                let buffer = self.resources.inline_bytes(bytemuck::cast_slice(&words));
                unsafe {
                    table.setAddress_atIndex(buffer.gpuAddress(), index);
                }
            }
        }
        true
    }

    pub(crate) fn bind_bindless(
        &self,
        snapshot: std::rc::Rc<super::argument_buffer::BindlessSnapshot>,
    ) {
        if let Some(command) = self.inner.commandBuffer() {
            command.useResidencySet(snapshot.residency.native());
        }
        self.bind_native_buffer(&snapshot.buffer, 0, 9, ShaderStages::VERTEX_FRAGMENT);
        self.resources.retain_bindless(snapshot);
    }

    pub(crate) fn draw_indirect(&self, buffer: &MetalBuffer, offset: u64) {
        if !self.prepare_arguments() {
            return;
        }
        self.use_buffer(
            &buffer.inner,
            MTLResourceUsage::Read,
            MTLRenderStages::Vertex,
        );
        if !offset.is_multiple_of(4) || offset.checked_add(16).is_none_or(|end| end > buffer.size())
        {
            self.resources
                .fail("Indirect draw command exceeds its native buffer range".into());
            return;
        }
        self.inner.drawPrimitives_indirectBuffer(
            MTLPrimitiveType::Triangle,
            buffer.inner.gpuAddress() + offset,
        );
    }

    pub(crate) fn bind_storage_buffer_range_render(
        &self,
        buffer: &MetalBuffer,
        offset: u64,
        size: u64,
        index: u32,
        stages: ShaderStages,
    ) {
        self.bind_native_buffer(&buffer.inner, offset, index, stages);
        if stages.vertex {
            self.vertex_state.buffer(index as usize, size);
        }
        if stages.fragment {
            self.fragment_state.buffer(index as usize, size);
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
        self.vertex_layout = Some(pipeline.vertex_layout.clone());
        self.fragment_layout = pipeline.fragment_layout.clone();
        self.resources.retain_graphics_pipeline(pipeline);
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
        self.bind_native_buffer(&buffer.inner, offset, index, ShaderStages::VERTEX);
    }

    fn bind_index_buffer(&mut self, buffer: &MetalBuffer, offset: u64, index_type: IndexType) {
        self.use_buffer(
            &buffer.inner,
            MTLResourceUsage::Read,
            MTLRenderStages::Vertex,
        );
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
        self.bind_native_buffer(&buffer.inner, offset, index, stages);
    }

    fn bind_texture(&mut self, view: &MetalTextureView, index: u32, stages: ShaderStages) {
        self.bind_native_texture(&view.inner, index, stages);
    }

    fn bind_sampler(&mut self, sampler: &MetalSamplerState, index: u32, stages: ShaderStages) {
        self.bind_native_sampler(&sampler.inner, index, stages);
    }

    fn set_push_constants(&mut self, data: &[u8], index: u32, stages: ShaderStages) {
        let buffer = self.resources.inline_bytes(data);
        self.bind_native_buffer(&buffer, 0, index, stages);
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
        if !self.prepare_arguments() {
            return;
        }
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
        if !self.prepare_arguments() {
            return;
        }
        if let (Some(index_buffer), Some(index_type)) = (&self.index_buffer, self.index_type) {
            let index_size = match index_type {
                MTLIndexType::UInt16 => 2u64,
                MTLIndexType::UInt32 => 4u64,
                _ => 2,
            };
            unsafe {
                self.inner.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferLength_instanceCount_baseVertex_baseInstance(
                    MTLPrimitiveType::Triangle,
                    index_count as usize,
                    index_type,
                    index_buffer.gpuAddress() + self.index_offset + first_index as u64 * index_size,
                    index_buffer.length() - (self.index_offset + first_index as u64 * index_size) as usize,
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
