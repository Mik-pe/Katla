use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLComputePipelineState, MTLCullMode, MTLDepthStencilState, MTLRenderPipelineState, MTLWinding,
};

use crate::backend::resource::{GpuComputePipeline, GpuGraphicsPipeline};

#[derive(Clone)]
pub(crate) struct MetalGraphicsPipeline {
    pub(crate) pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    pub(crate) vertex_layout: super::binding_schema::ArgumentTableLayout,
    pub(crate) fragment_layout: Option<super::binding_schema::ArgumentTableLayout>,
    pub(crate) depth_stencil_state: Option<Retained<ProtocolObject<dyn MTLDepthStencilState>>>,
    pub(crate) cull_mode: MTLCullMode,
    pub(crate) front_face: MTLWinding,
    pub(crate) depth_bias: Option<(f32, f32, f32)>,
    pub(crate) stencil_reference: Option<u32>,
    pub(crate) wireframe: bool,
}

impl GpuGraphicsPipeline for MetalGraphicsPipeline {}

// SAFETY: `MTLRenderPipelineState`, `MTLDepthStencilState` are immutable,
// fully-resolved state objects; Apple documents them as thread-safe. `Retained`
// adds ownership only — no interior mutability. Bounds locked by the affinity
// contract in `surface.rs`'s test module.
unsafe impl Send for MetalGraphicsPipeline {}
unsafe impl Sync for MetalGraphicsPipeline {}

#[derive(Clone)]
pub(crate) struct MetalComputePipeline {
    pub(crate) pipeline_state: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    pub(crate) workgroup: [u32; 3],
    pub(crate) uniform_bindings: Vec<(u32, u32)>,
    pub(crate) table_layout: super::binding_schema::ArgumentTableLayout,
}

impl GpuComputePipeline for MetalComputePipeline {
    #[cfg(test)]
    fn workgroup_size(&self) -> [u32; 3] {
        self.workgroup
    }
}

// SAFETY: `MTLComputePipelineState` is an immutable, fully-resolved state
// object; Apple documents it as thread-safe. `Retained` adds ownership only.
unsafe impl Send for MetalComputePipeline {}
unsafe impl Sync for MetalComputePipeline {}
