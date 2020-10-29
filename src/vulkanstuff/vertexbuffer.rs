use std::sync::Arc;

use erupt::{
    utils::allocator::{Allocation, MemoryTypeFinder},
    vk1_0::*,
    DeviceLoader,
};

use super::context::VulkanContext;

struct BufferObject {
    buffer: Option<Allocation<Buffer>>,
    buf_size: DeviceSize,
    count: u32,
    context: Arc<VulkanContext>,
}

//TODO: Holding an RC for every buffer is... meh.
// figure out a better way of pooling this, also for safer dropping of buffers
// that are in-flight
impl Drop for BufferObject {
    fn drop(&mut self) {
        let device = &self.context.device;
        if let Some(buffer) = self.buffer.take() {
            self.context.free_object(buffer);
        }
    }
}

pub struct VertexBuffer {
    buffer: BufferObject,
}

pub struct IndexBuffer {
    buffer: BufferObject,
    pub index_type: IndexType,
}

impl BufferObject {
    fn upload_data(&mut self, device: &DeviceLoader, data: &[u8]) {
        let data_size = std::mem::size_of_val(data) as DeviceSize;
        if self.buf_size < data_size {
            panic!(
                "Too little memory allocated for buffer of size {}",
                data_size
            );
        }
        match &self.buffer {
            Some(buffer) => {
                //This is a bit awkward.. Probably something finicky within erupt
                let range = ..buffer.region().start + data_size;

                let mut map = buffer.map(&device, range).unwrap();
                map.import(data);
                map.unmap(&device).unwrap();
            }
            _ => {}
        }
    }
}

impl IndexBuffer {
    pub fn new(
        context: Arc<VulkanContext>,
        buf_size: DeviceSize,
        index_type: IndexType,
        count: u32,
    ) -> Self {
        let buffer = {
            let create_info = BufferCreateInfoBuilder::new()
                .sharing_mode(SharingMode::EXCLUSIVE)
                .usage(BufferUsageFlags::INDEX_BUFFER)
                .size(buf_size);
            let device = &context.device;
            let buffer = context
                .allocate_object(
                    unsafe { device.create_buffer(&create_info, None, None).unwrap() },
                    MemoryTypeFinder::dynamic(),
                )
                .unwrap();

            BufferObject {
                buffer: Some(buffer),
                buf_size,
                count,
                context,
            }
        };
        Self { buffer, index_type }
    }

    pub fn upload_data(&mut self, device: &DeviceLoader, data: &[u8]) {
        self.buffer.upload_data(device, data);
    }

    pub fn object(&self) -> &Buffer {
        self.buffer.buffer.as_ref().unwrap().object()
    }

    pub fn count(&self) -> u32 {
        self.buffer.count
    }
}

impl VertexBuffer {
    pub fn new(context: Arc<VulkanContext>, buf_size: DeviceSize, count: u32) -> Self {
        let buffer = {
            let create_info = BufferCreateInfoBuilder::new()
                .sharing_mode(SharingMode::EXCLUSIVE)
                .usage(BufferUsageFlags::VERTEX_BUFFER)
                .size(buf_size);
            let device = &context.device;
            let buffer = context
                .allocate_object(
                    unsafe { device.create_buffer(&create_info, None, None).unwrap() },
                    MemoryTypeFinder::dynamic(),
                )
                .unwrap();

            BufferObject {
                buffer: Some(buffer),
                buf_size,
                count,
                context,
            }
        };
        Self { buffer }
    }

    pub fn object(&self) -> &Buffer {
        self.buffer.buffer.as_ref().unwrap().object()
    }

    pub fn count(&self) -> u32 {
        self.buffer.count
    }

    pub fn upload_data(&mut self, device: &DeviceLoader, data: &[u8]) {
        self.buffer.upload_data(device, data);
    }
}
