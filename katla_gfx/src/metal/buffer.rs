use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::MTLBuffer;

use crate::backend::resource::GpuBuffer;

pub(crate) struct MetalBuffer {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLBuffer>>,
    size: u64,
}

impl MetalBuffer {
    pub(crate) fn new(inner: Retained<ProtocolObject<dyn MTLBuffer>>, size: u64) -> Self {
        Self { inner, size }
    }
}

impl GpuBuffer for MetalBuffer {
    fn size(&self) -> u64 {
        self.size
    }

    fn map(&self) -> *mut u8 {
        self.inner.contents().as_ptr() as *mut u8
    }

    fn unmap(&self) {}

    fn flush(&self, offset: u64, size: u64) {
        let _ = (offset, size);
    }

    fn gpu_address(&self) -> u64 {
        self.inner.gpuAddress()
    }
}

unsafe impl Send for MetalBuffer {}
unsafe impl Sync for MetalBuffer {}
