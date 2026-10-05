use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTL4ArgumentTable, MTL4CommandEncoder, MTL4ComputeCommandEncoder, MTLBuffer, MTLSize,
};

#[cfg(test)]
use objc2_metal::{MTLSamplerState, MTLTexture};

use crate::backend::command::*;

use super::MetalBackend;
use super::buffer::MetalBuffer;
#[cfg(test)]
use super::sampler::MetalSamplerState;
#[cfg(test)]
use super::texture::MetalTextureView;

pub(crate) struct MetalComputeEncoder {
    pub(crate) inner: Retained<ProtocolObject<dyn MTL4ComputeCommandEncoder>>,
    workgroup_size: MTLSize,
    state: super::argument_state::ArgumentState,
    layout: Option<super::binding_schema::ArgumentTableLayout>,
    resources: std::rc::Rc<super::encoding_resources::EncodingResources>,
    table: Retained<ProtocolObject<dyn MTL4ArgumentTable>>,
    ended: std::cell::Cell<bool>,
}

impl MetalComputeEncoder {
    pub(crate) fn new(
        inner: Retained<ProtocolObject<dyn MTL4ComputeCommandEncoder>>,
        resources: std::rc::Rc<super::encoding_resources::EncodingResources>,
    ) -> Self {
        let table = resources.argument_table("compute arguments");
        inner.setArgumentTable(Some(&table));
        Self {
            ended: std::cell::Cell::new(false),
            inner,
            resources,
            table,
            state: Default::default(),
            layout: None,
            workgroup_size: MTLSize {
                width: 1,
                height: 1,
                depth: 1,
            },
        }
    }
    pub(crate) fn bind_native_buffer(
        &self,
        buffer: &ProtocolObject<dyn MTLBuffer>,
        offset: u64,
        index: u32,
    ) {
        self.state.buffer(
            index as usize,
            (buffer.length() as u64).saturating_sub(offset),
        );
        self.resources
            .residency
            .add_buffer(buffer)
            .expect("Metal compute buffer residency");
        unsafe {
            self.table
                .setAddress_atIndex(buffer.gpuAddress() + offset, index as usize);
        }
    }

    pub(crate) fn bind_storage_buffer_range(
        &mut self,
        buffer: &MetalBuffer,
        offset: u64,
        size: u64,
        index: u32,
    ) {
        self.bind_native_buffer(&buffer.inner, offset, index);
        self.state.buffer(index as usize, size);
    }
    fn prepare_arguments(&self) -> bool {
        if let Some(layout) = &self.layout {
            self.resources.capture_binding(&self.table, layout);
            if let Err(error) = self.state.validate(layout) {
                self.resources.fail(error);
                return false;
            }
            if let Some(index) = layout.sizes_buffer {
                let words = self.state.sizes(layout);
                let buffer = self.resources.inline_bytes(bytemuck::cast_slice(&words));
                unsafe {
                    self.table.setAddress_atIndex(buffer.gpuAddress(), index);
                }
            }
        }
        true
    }

    pub(crate) fn dispatch_indirect(&self, buffer: &MetalBuffer, offset: u64) {
        if !self.prepare_arguments() {
            return;
        }
        self.resources
            .residency
            .add_buffer(&buffer.inner)
            .expect("Metal indirect buffer residency");
        unsafe {
            self.inner
                .dispatchThreadgroupsWithIndirectBuffer_threadsPerThreadgroup(
                    buffer.inner.gpuAddress() + offset,
                    self.workgroup_size,
                );
        }
    }
}

impl GpuComputeEncoder<MetalBackend> for MetalComputeEncoder {
    fn end_encoding(self) {
        if !self.ended.replace(true) {
            self.inner.endEncoding();
        }
    }

    fn bind_compute_pipeline(
        &mut self,
        pipeline: &<MetalBackend as crate::backend::traits::GpuBackend>::ComputePipeline,
    ) {
        self.layout = Some(pipeline.table_layout.clone());
        self.resources.retain_table_layout(&pipeline.table_layout);
        self.resources
            .retain_compute_pipeline(&pipeline.pipeline_state);
        self.inner.setComputePipelineState(&pipeline.pipeline_state);
        self.workgroup_size = MTLSize {
            width: pipeline.workgroup[0] as usize,
            height: pipeline.workgroup[1] as usize,
            depth: pipeline.workgroup[2] as usize,
        };
    }

    #[cfg(test)]
    fn bind_storage_buffer(&mut self, buffer: &MetalBuffer, offset: u64, index: u32) {
        self.bind_native_buffer(&buffer.inner, offset, index);
    }

    #[cfg(test)]
    fn bind_texture(&mut self, view: &MetalTextureView, index: u32) {
        self.state.texture(index as usize);
        self.resources
            .residency
            .add_texture(&view.inner)
            .expect("Metal compute texture residency");
        unsafe {
            self.table
                .setTexture_atIndex(view.inner.gpuResourceID(), index as usize);
        }
    }

    #[cfg(test)]
    fn bind_sampler(&mut self, sampler: &MetalSamplerState, index: u32) {
        self.state.sampler(index as usize);
        self.resources.retain_sampler(&sampler.inner);
        unsafe {
            self.table
                .setSamplerState_atIndex(sampler.inner.gpuResourceID(), index as usize);
        }
    }

    fn set_push_constants(&mut self, data: &[u8], index: u32) {
        let buffer = self.resources.inline_bytes(data);
        self.bind_native_buffer(&buffer, 0, index);
    }

    fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        if !self.prepare_arguments() {
            return;
        }
        self.inner.dispatchThreadgroups_threadsPerThreadgroup(
            MTLSize {
                width: group_count_x as usize,
                height: group_count_y as usize,
                depth: group_count_z as usize,
            },
            self.workgroup_size,
        );
    }
}

impl Drop for MetalComputeEncoder {
    fn drop(&mut self) {
        if !self.ended.replace(true) {
            self.inner.endEncoding();
        }
    }
}
