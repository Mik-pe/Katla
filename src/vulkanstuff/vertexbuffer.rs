use erupt::{
    utils::allocator::{Allocation, Allocator, MemoryTypeFinder},
    vk1_0::*,
    DeviceLoader,
};

pub struct BufferObject {
    pub buffer: Allocation<Buffer>,
    buf_size: DeviceSize,
    pub count: u32,
}
pub trait IndexBuffer {
    fn new(
        device: &DeviceLoader,
        allocator: &mut Allocator,
        buf_size: DeviceSize,
        count: u32,
    ) -> Self;
}

pub trait VertexBuffer {
    fn new(
        device: &DeviceLoader,
        allocator: &mut Allocator,
        buf_size: DeviceSize,
        count: u32,
    ) -> Self;
}

impl BufferObject {
    pub fn upload_data(&mut self, device: &DeviceLoader, data: &[u8]) {
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

    pub fn destroy(self, device: &DeviceLoader, allocator: &mut Allocator) {
        allocator.free(device, self.buffer);
    }
}

impl IndexBuffer for BufferObject {
    fn new(
        device: &DeviceLoader,
        allocator: &mut Allocator,
        buf_size: DeviceSize,
        count: u32,
    ) -> Self {
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

        Self {
            buffer,
            buf_size,
            count,
        }
    }
}

impl VertexBuffer for BufferObject {
    fn new(
        device: &DeviceLoader,
        allocator: &mut Allocator,
        buf_size: DeviceSize,
        count: u32,
    ) -> Self {
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

        Self {
            buffer,
            buf_size,
            count,
        }
    }
}
