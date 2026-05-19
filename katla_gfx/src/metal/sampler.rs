use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::MTLSamplerState;

use crate::backend::resource::GpuSampler;

#[derive(Clone)]
pub(crate) struct MetalSamplerState {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLSamplerState>>,
}

impl GpuSampler for MetalSamplerState {}

unsafe impl Send for MetalSamplerState {}
unsafe impl Sync for MetalSamplerState {}
