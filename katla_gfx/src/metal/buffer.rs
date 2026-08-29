use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSRange;
use objc2_metal::{MTLBuffer, MTLResource, MTLStorageMode};

use crate::backend::resource::GpuBuffer;

#[derive(Clone)]
pub(crate) struct MetalBuffer {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLBuffer>>,
    size: u64,
    storage_mode: MTLStorageMode,
}

impl MetalBuffer {
    pub(crate) fn new(inner: Retained<ProtocolObject<dyn MTLBuffer>>, size: u64) -> Self {
        let storage_mode = inner.storageMode();
        Self {
            inner,
            size,
            storage_mode,
        }
    }
}

impl GpuBuffer for MetalBuffer {
    fn size(&self) -> u64 {
        self.size
    }

    fn map(&self) -> *mut u8 {
        self.inner.contents().as_ptr() as *mut u8
    }

    fn unmap(&self) {
        self.flush(0, self.size);
    }

    fn flush(&self, offset: u64, size: u64) {
        if self.storage_mode == MTLStorageMode::Managed {
            self.inner
                .didModifyRange(NSRange::new(offset as usize, size as usize));
        }
    }

    fn gpu_address(&self) -> u64 {
        self.inner.gpuAddress()
    }
}

unsafe impl Send for MetalBuffer {}
unsafe impl Sync for MetalBuffer {}
