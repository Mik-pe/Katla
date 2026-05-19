use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLComputePipelineState, MTLCullMode, MTLDepthStencilState, MTLRenderPipelineState, MTLWinding,
};

use crate::backend::resource::{GpuComputePipeline, GpuGraphicsPipeline};

#[derive(Clone)]
pub(crate) struct MetalGraphicsPipeline {
    pub(crate) pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    pub(crate) depth_stencil_state: Option<Retained<ProtocolObject<dyn MTLDepthStencilState>>>,
    pub(crate) cull_mode: MTLCullMode,
    pub(crate) front_face: MTLWinding,
    pub(crate) depth_bias: Option<(f32, f32, f32)>,
}

impl GpuGraphicsPipeline for MetalGraphicsPipeline {}

unsafe impl Send for MetalGraphicsPipeline {}
unsafe impl Sync for MetalGraphicsPipeline {}

#[derive(Clone)]
pub(crate) struct MetalComputePipeline {
    pub(crate) pipeline_state: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    pub(crate) workgroup: [u32; 3],
}

impl GpuComputePipeline for MetalComputePipeline {
    fn workgroup_size(&self) -> [u32; 3] {
        self.workgroup
    }
}

unsafe impl Send for MetalComputePipeline {}
unsafe impl Sync for MetalComputePipeline {}
