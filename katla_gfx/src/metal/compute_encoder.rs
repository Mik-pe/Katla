use core::ffi::c_void;
use core::ptr::NonNull;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLBarrierScope, MTLCommandEncoder, MTLComputeCommandEncoder, MTLComputePipelineState, MTLSize,
};

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
    /// Bind an already-built compute pipeline state directly (no registry
    /// handle). Used by the particle subsystem, which owns its pipelines.
    pub(crate) fn bind_compute_pipeline_raw(
        &mut self,
        pipeline_state: &Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    ) {
        self.inner.setComputePipelineState(pipeline_state);
        self.workgroup_size = MTLSize {
            width: pipeline_state.threadExecutionWidth(),
            height: 1,
            depth: 1,
        };
    }

    /// Bind a storage buffer at a byte offset (sub-allocated region binding).
    pub(crate) fn bind_storage_buffer_at_offset(
        &mut self,
        buffer: &MetalBuffer,
        offset: u64,
        group: u32,
        index: u32,
    ) {
        let _ = group;
        unsafe {
            self.inner.setBuffer_offset_atIndex(
                Some(&buffer.inner),
                offset as usize,
                index as usize,
            );
        }
    }

    /// Dispatch with an explicit threadgroup width (raw pipelines don't carry
    /// the WGSL @workgroup_size through naga's metadata the way the trait
    /// pipeline wrapper does).
    pub(crate) fn dispatch_raw(
        &mut self,
        groups_x: u32,
        groups_y: u32,
        groups_z: u32,
        tg_width: u32,
    ) {
        self.inner.dispatchThreadgroups_threadsPerThreadgroup(
            MTLSize {
                width: groups_x as usize,
                height: groups_y as usize,
                depth: groups_z as usize,
            },
            MTLSize {
                width: tg_width as usize,
                height: 1,
                depth: 1,
            },
        );
    }

    /// Full buffer-to-buffer memory barrier (compute write visibility).
    pub(crate) fn memory_barrier_buffers(&self) {
        self.inner.memoryBarrierWithScope(MTLBarrierScope::Buffers);
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
