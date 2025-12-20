use super::context::VulkanContext;
use ash::vk;
use gpu_allocator::vulkan::Allocation;

use std::rc::Rc;

struct BufferObject {
    allocation: Option<Allocation>,
    buffer: vk::Buffer,
    buf_size: vk::DeviceSize,
    count: u32,
    context: Rc<VulkanContext>,
}

//TODO: Holding an RC for every buffer is... meh.
// figure out a better way of pooling this, also for safer dropping of buffers
// that are in-flight
impl Drop for BufferObject {
    fn drop(&mut self) {
        if let Some(allocation) = self.allocation.take() {
            self.context.free_buffer(self.buffer, allocation);
        }
    }
}

pub struct VertexBuffer {
    buffer: BufferObject,
}

pub struct IndexBuffer {
    buffer: BufferObject,
    pub index_type: vk::IndexType,
}

impl BufferObject {
    fn upload_data(&mut self, data: &[u8]) {
        let data_size = std::mem::size_of_val(data) as vk::DeviceSize;
        if self.buf_size < data_size {
            panic!(
                "Too little memory allocated for buffer of size {}",
                data_size
            );
        }
        if let Some(allocation) = &self.allocation {
            let mapped_ptr = self.context.map_buffer(allocation);
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), mapped_ptr, data_size as usize);
            }
        }
    }
}

impl IndexBuffer {
    pub fn new(
        context: Rc<VulkanContext>,
        buf_size: vk::DeviceSize,
        index_type: vk::IndexType,
        count: u32,
    ) -> Self {
        let buffer = {
            let create_info = vk::BufferCreateInfo::default()
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .usage(vk::BufferUsageFlags::INDEX_BUFFER)
                .size(buf_size);
            let (buffer, allocation) =
                context.allocate_buffer(&create_info, gpu_allocator::MemoryLocation::CpuToGpu);

            BufferObject {
                allocation: Some(allocation),
                buffer,
                buf_size,
                count,
                context,
            }
        };
        Self { buffer, index_type }
    }

    pub fn upload_data(&mut self, data: &[u8]) {
        self.buffer.upload_data(data);
    }

    pub fn object(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    pub fn count(&self) -> u32 {
        self.buffer.count
    }
}

impl VertexBuffer {
    pub fn new(context: Rc<VulkanContext>, buf_size: vk::DeviceSize, count: u32) -> Self {
        let buffer = {
            let create_info = vk::BufferCreateInfo::default()
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .usage(vk::BufferUsageFlags::VERTEX_BUFFER)
                .size(buf_size);
            let (buffer, allocation) =
                context.allocate_buffer(&create_info, gpu_allocator::MemoryLocation::CpuToGpu);

            BufferObject {
                allocation: Some(allocation),
                buffer,
                buf_size,
                count,
                context,
            }
        };
        Self { buffer }
    }

    pub fn object(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    pub fn count(&self) -> u32 {
        self.buffer.count
    }

    pub fn upload_data(&mut self, data: &[u8]) {
        self.buffer.upload_data(data);
    }
}
