use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSRange;
use objc2_metal::{MTLBuffer, MTLResource, MTLStorageMode};

use crate::backend::resource::GpuBuffer;
use crate::render_graph::BufferDesc;

#[derive(Clone)]
pub(crate) struct MetalBuffer {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLBuffer>>,
    size: u64,
    storage_mode: MTLStorageMode,
    pub(crate) lifetime: std::sync::Arc<BufferLifetime>,
}

pub struct MetalGraphBuffer {
    pub(crate) buffer: MetalBuffer,
    pub(crate) desc: BufferDesc,
    pub(crate) offset: u64,
}

impl MetalGraphBuffer {
    pub(crate) fn new(buffer: MetalBuffer, desc: BufferDesc) -> Self {
        Self {
            buffer,
            desc,
            offset: 0,
        }
    }

    pub(crate) fn size(&self) -> u64 {
        self.desc.size
    }

    pub fn native_buffer(&self) -> &ProtocolObject<dyn MTLBuffer> {
        &self.buffer.inner
    }
}

pub(crate) type BufferRetirementQueue = std::sync::Arc<std::sync::Mutex<Vec<u64>>>;
type WeakRetirementQueue = std::sync::Weak<std::sync::Mutex<Vec<u64>>>;

#[derive(Default)]
pub(crate) struct BufferLifetime {
    watchers: std::sync::Mutex<Vec<(u64, WeakRetirementQueue)>>,
}
impl BufferLifetime {
    pub(crate) fn watch(&self, identity: u64, queue: &BufferRetirementQueue) {
        let mut watchers = self
            .watchers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let weak = std::sync::Arc::downgrade(queue);
        if !watchers
            .iter()
            .any(|(id, existing)| *id == identity && std::sync::Weak::ptr_eq(existing, &weak))
        {
            watchers.push((identity, weak));
        }
    }
}
impl Drop for BufferLifetime {
    fn drop(&mut self) {
        for (identity, queue) in self
            .watchers
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
        {
            if let Some(queue) = queue.upgrade() {
                queue
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .push(*identity);
            }
        }
    }
}

impl MetalBuffer {
    pub(crate) fn new(inner: Retained<ProtocolObject<dyn MTLBuffer>>, size: u64) -> Self {
        let storage_mode = inner.storageMode();
        Self {
            inner,
            size,
            storage_mode,
            lifetime: Default::default(),
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
