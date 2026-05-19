use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::MTLDepthStencilState;

pub(crate) struct MetalDepthStencilState {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLDepthStencilState>>,
}

unsafe impl Send for MetalDepthStencilState {}
unsafe impl Sync for MetalDepthStencilState {}
