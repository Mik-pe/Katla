use core::ffi::c_void;
use core::ptr::NonNull;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandEncoder, MTLComputeCommandEncoder, MTLSize};

use crate::backend::command::*;

use super::MetalBackend;
use super::buffer::MetalBuffer;
use super::sampler::MetalSamplerState;
use super::texture::MetalTextureView;

pub(crate) struct MetalComputeEncoder {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLComputeCommandEncoder>>,
    workgroup_size: MTLSize,
}

impl MetalComputeEncoder {
    pub(crate) fn new(inner: Retained<ProtocolObject<dyn MTLComputeCommandEncoder>>) -> Self {
        Self {
            inner,
            workgroup_size: MTLSize {
                width: 1,
                height: 1,
                depth: 1,
            },
        }
    }
}

impl GpuComputeEncoder<MetalBackend> for MetalComputeEncoder {
    fn end_encoding(self) {
        self.inner.endEncoding();
    }

    fn bind_compute_pipeline(
        &mut self,
        pipeline: &<MetalBackend as crate::backend::traits::GpuBackend>::ComputePipeline,
    ) {
        self.inner.setComputePipelineState(&pipeline.pipeline_state);
        self.workgroup_size = MTLSize {
            width: pipeline.workgroup[0] as usize,
            height: pipeline.workgroup[1] as usize,
            depth: pipeline.workgroup[2] as usize,
        };
    }

    fn bind_storage_buffer(&mut self, buffer: &MetalBuffer, offset: u64, index: u32) {
        unsafe {
            self.inner.setBuffer_offset_atIndex(
                Some(&buffer.inner),
                offset as usize,
                index as usize,
            );
        }
    }

    fn bind_texture(&mut self, view: &MetalTextureView, index: u32) {
        unsafe {
            self.inner
                .setTexture_atIndex(Some(&view.inner), index as usize);
        }
    }

    fn bind_sampler(&mut self, sampler: &MetalSamplerState, index: u32) {
        unsafe {
            self.inner
                .setSamplerState_atIndex(Some(&sampler.inner), index as usize);
        }
    }

    fn set_push_constants(&mut self, data: &[u8], index: u32) {
        unsafe {
            let ptr = NonNull::new(data.as_ptr() as *mut c_void).unwrap();
            self.inner
                .setBytes_length_atIndex(ptr, data.len(), index as usize);
        }
    }

    fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
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
