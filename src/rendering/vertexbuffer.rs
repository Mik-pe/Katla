use erupt::{
    utils::allocator::{Allocation, Allocator, MemoryTypeFinder},
    vk1_0::*,
    DeviceLoader,
};
pub struct VertexBuffer {
    buffer: Allocation<Buffer>,
    buf_size: DeviceSize,
}

impl VertexBuffer {
    pub fn new(device: &DeviceLoader, allocator: &mut Allocator, buf_size: DeviceSize) -> Self {
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

        Self { buffer, buf_size }
    }

    pub fn upload_data(&mut self, device: &DeviceLoader, data: &[u8]) {
        if self.buf_size < data.len() as _ {
            panic!(
                "Too little memory allocated for buffer of size {}",
                data.len()
            );
        }
        let mut map = self
            .buffer
            .map(&device, ..(data.len() as DeviceSize))
            .unwrap();
        map.import(data);
        map.unmap(&device).unwrap();
        println!("Uploaded data of size {}", data.len());
    }

    pub fn destroy(self, device: &DeviceLoader, allocator: &mut Allocator) {
        allocator.free(device, self.buffer);
    }
}
