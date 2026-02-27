use super::context::VulkanContext;
use ash::vk;
use gpu_allocator::vulkan::Allocation;

use std::rc::Rc;

/// Wrapper for Vulkan index types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexType {
    Uint8,
    Uint16,
    Uint32,
    None,
}

impl IndexType {
    /// Returns the size in bytes for this index type
    pub fn size(&self) -> u32 {
        match self {
            IndexType::Uint8 => 1,
            IndexType::Uint16 => 2,
            IndexType::Uint32 => 4,
            IndexType::None => 0,
        }
    }
}

impl From<IndexType> for vk::IndexType {
    fn from(index_type: IndexType) -> Self {
        match index_type {
            IndexType::Uint8 => vk::IndexType::UINT8_EXT,
            IndexType::Uint16 => vk::IndexType::UINT16,
            IndexType::Uint32 => vk::IndexType::UINT32,
            IndexType::None => vk::IndexType::NONE_KHR,
        }
    }
}

impl From<vk::IndexType> for IndexType {
    fn from(index_type: vk::IndexType) -> Self {
        match index_type {
            vk::IndexType::UINT8_EXT => IndexType::Uint8,
            vk::IndexType::UINT16 => IndexType::Uint16,
            vk::IndexType::UINT32 => IndexType::Uint32,
            vk::IndexType::NONE_KHR => IndexType::None,
            _ => panic!("Unsupported Vulkan index type: {:?}", index_type),
        }
    }
}

struct BufferObject {
    allocation: Option<Allocation>,
    buffer: vk::Buffer,
    buf_size: vk::DeviceSize,
    count: u32,
    context: Rc<VulkanContext>,
}

impl Drop for BufferObject {
    fn drop(&mut self) {
        if let Some(allocation) = self.allocation.take() {
            self.context
                .free_buffer(crate::sync::VkBuffer::new(self.buffer), allocation);
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
        index_type: IndexType,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_type_size() {
        assert_eq!(IndexType::Uint8.size(), 1);
        assert_eq!(IndexType::Uint16.size(), 2);
        assert_eq!(IndexType::Uint32.size(), 4);
        assert_eq!(IndexType::None.size(), 0);
    }

    #[test]
    fn test_index_type_to_vk() {
        let vk_type: vk::IndexType = IndexType::Uint8.into();
        assert_eq!(vk_type, vk::IndexType::UINT8_EXT);

        let vk_type: vk::IndexType = IndexType::Uint16.into();
        assert_eq!(vk_type, vk::IndexType::UINT16);

        let vk_type: vk::IndexType = IndexType::Uint32.into();
        assert_eq!(vk_type, vk::IndexType::UINT32);

        let vk_type: vk::IndexType = IndexType::None.into();
        assert_eq!(vk_type, vk::IndexType::NONE_KHR);
    }

    #[test]
    fn test_vk_to_index_type() {
        let index_type: IndexType = vk::IndexType::UINT8_EXT.into();
        assert_eq!(index_type, IndexType::Uint8);

        let index_type: IndexType = vk::IndexType::UINT16.into();
        assert_eq!(index_type, IndexType::Uint16);

        let index_type: IndexType = vk::IndexType::UINT32.into();
        assert_eq!(index_type, IndexType::Uint32);

        let index_type: IndexType = vk::IndexType::NONE_KHR.into();
        assert_eq!(index_type, IndexType::None);
    }

    #[test]
    fn test_index_type_roundtrip() {
        let original = IndexType::Uint16;
        let vk_type: vk::IndexType = original.into();
        let converted: IndexType = vk_type.into();
        assert_eq!(original, converted);
    }
}
