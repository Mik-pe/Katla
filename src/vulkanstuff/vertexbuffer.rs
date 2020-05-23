use erupt::{
    utils::allocator::{Allocation, Allocator, MemoryTypeFinder},
    vk1_0::*,
    DeviceLoader,
};

struct BufferObject {
    buffer: Allocation<Buffer>,
    buf_size: DeviceSize,
    count: u32,
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
        //This is a bit awkward.. Probably something finicky within erupt
        let range = ..self.buffer.region().start + data_size;

        let mut map = self.buffer.map(&device, range).unwrap();
        map.import(data);
        map.unmap(&device).unwrap();
    }

    fn destroy(self, device: &DeviceLoader, allocator: &mut Allocator) {
        allocator.free(device, self.buffer);
    }
}

impl IndexBuffer {
    pub fn new(
        device: &DeviceLoader,
        allocator: &mut Allocator,
        buf_size: DeviceSize,
        index_type: IndexType,
        count: u32,
    ) -> Self {
        let buffer = {
            let create_info = BufferCreateInfoBuilder::new()
                .sharing_mode(SharingMode::EXCLUSIVE)
                .usage(BufferUsageFlags::INDEX_BUFFER)
                .size(buf_size);

            let buffer = allocator
                .allocate(
                    device,
                    unsafe { device.create_buffer(&create_info, None, None).unwrap() },
                    MemoryTypeFinder::dynamic(),
                )
                .unwrap();

            BufferObject {
                buffer,
                buf_size,
                count,
            }
        };
        Self { buffer, index_type }
    }

    pub fn upload_data(&mut self, device: &DeviceLoader, data: &[u8]) {
        self.buffer.upload_data(device, data);
    }

    pub fn object(&self) -> &Buffer {
        self.buffer.buffer.object()
    }

    pub fn count(&self) -> u32 {
        self.buffer.count
    }

    pub fn destroy(self, device: &DeviceLoader, allocator: &mut Allocator) {
        self.buffer.destroy(device, allocator);
    }
}

impl VertexBuffer {
    pub fn new(
        device: &DeviceLoader,
        allocator: &mut Allocator,
        buf_size: DeviceSize,
        count: u32,
    ) -> Self {
        let buffer = {
            let create_info = BufferCreateInfoBuilder::new()
                .sharing_mode(SharingMode::EXCLUSIVE)
                .usage(BufferUsageFlags::VERTEX_BUFFER)
                .size(buf_size);

            let buffer = allocator
                .allocate(
                    device,
                    unsafe { device.create_buffer(&create_info, None, None).unwrap() },
                    MemoryTypeFinder::dynamic(),
                )
                .unwrap();

            BufferObject {
                buffer,
                buf_size,
                count,
            }
        };
        Self { buffer }
    }

    pub fn object(&self) -> &Buffer {
        self.buffer.buffer.object()
    }

    pub fn count(&self) -> u32 {
        self.buffer.count
    }

    pub fn upload_data(&mut self, device: &DeviceLoader, data: &[u8]) {
        self.buffer.upload_data(device, data);
    }

    pub fn destroy(self, device: &DeviceLoader, allocator: &mut Allocator) {
        self.buffer.destroy(device, allocator);
    }
}
