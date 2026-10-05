use super::context::VulkanContext;
use super::retirement::RetiredBuffer;
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

impl From<crate::backend::command::IndexType> for IndexType {
    fn from(format: crate::backend::command::IndexType) -> Self {
        match format {
            crate::backend::command::IndexType::Uint8 => IndexType::Uint8,
            crate::backend::command::IndexType::Uint16 => IndexType::Uint16,
            crate::backend::command::IndexType::Uint32 => IndexType::Uint32,
        }
    }
}

impl From<crate::backend::command::IndexType> for vk::IndexType {
    fn from(format: crate::backend::command::IndexType) -> Self {
        IndexType::from(format).into()
    }
}

use std::mem::ManuallyDrop;

struct BufferObject {
    allocation: ManuallyDrop<Allocation>,
    buffer: vk::Buffer,
    buf_size: vk::DeviceSize,
    count: u32,
    context: Rc<VulkanContext>,
    buffer_usage: vk::BufferUsageFlags,
    /// Where this buffer's memory lives; device-local buffers are only
    /// writable through staged copies, never direct mappings.
    location: gpu_allocator::MemoryLocation,
}

impl Drop for BufferObject {
    fn drop(&mut self) {
        let allocation = unsafe { ManuallyDrop::take(&mut self.allocation) };
        self.context.free_buffer(self.buffer, allocation);
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
    fn resize(&mut self, min_size: vk::DeviceSize) -> RetiredBuffer {
        let new_size = min_size * 2;
        let create_info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(self.buffer_usage)
            .size(new_size);
        // Build the replacement first: if allocation fails, the old buffer
        // and its allocation stay valid and owned by this object.
        let (buffer, allocation) = self
            .context
            .allocate_buffer(&create_info, gpu_allocator::MemoryLocation::CpuToGpu)
            .expect("Failed to resize buffer");
        let old_allocation = unsafe { ManuallyDrop::take(&mut self.allocation) };
        let retired = RetiredBuffer::new(self.buffer, old_allocation, self.context.clone());
        self.buffer = buffer;
        self.allocation = ManuallyDrop::new(allocation);
        self.buf_size = new_size;
        retired
    }

    fn upload_data(&mut self, data: &[u8]) -> Option<RetiredBuffer> {
        assert!(
            self.location == gpu_allocator::MemoryLocation::CpuToGpu,
            "direct upload requires host-visible memory; device-local mesh \
             buffers are populated through staged copies"
        );
        let data_size = std::mem::size_of_val(data) as vk::DeviceSize;
        let retired = if self.buf_size < data_size {
            Some(self.resize(data_size))
        } else {
            None
        };
        let mapped_ptr = self
            .context
            .map_buffer(&self.allocation)
            .expect("Failed to map buffer");
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), mapped_ptr, data_size as usize);
        }
        retired
    }

    /// Byte capacity of this buffer's storage.
    fn capacity(&self) -> vk::DeviceSize {
        self.buf_size
    }

    /// Where this buffer's memory lives.
    fn location(&self) -> gpu_allocator::MemoryLocation {
        self.location
    }

    /// Write bytes through the host-visible mapping and flush.
    ///
    /// Only valid for host-visible buffers; device-local placement is
    /// populated through staged copies instead.
    fn write_host_visible(&self, data: &[u8]) -> Result<(), crate::error::RendererError> {
        let ptr = self.mapped_ptr()?;
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len()) };
        self.context
            .flush_mapped_memory(&self.allocation, 0, data.len() as vk::DeviceSize)
    }

    /// Host-visible mapping of the whole buffer, without writing to it.
    fn mapped_ptr(&self) -> Result<*mut u8, crate::error::RendererError> {
        self.context.map_buffer(&self.allocation)
    }

    /// Transfer the native buffer and allocation out without destroying them.
    ///
    /// The caller (the retirement queue) becomes responsible for freeing.
    fn into_native_parts(mut self) -> (vk::Buffer, Allocation) {
        let allocation = unsafe { ManuallyDrop::take(&mut self.allocation) };
        let buffer = self.buffer;
        std::mem::forget(self);
        (buffer, allocation)
    }
}

impl IndexBuffer {
    pub fn new(
        context: Rc<VulkanContext>,
        buf_size: vk::DeviceSize,
        index_type: IndexType,
        count: u32,
    ) -> Self {
        Self::try_new(context, buf_size, index_type, count)
            .expect("Failed to allocate index buffer")
    }

    /// Fallible constructor used by paths that must preserve prior state on
    /// allocation failure (dynamic mesh growth).
    pub(crate) fn try_new(
        context: Rc<VulkanContext>,
        buf_size: vk::DeviceSize,
        index_type: IndexType,
        count: u32,
    ) -> Result<Self, crate::error::RendererError> {
        let create_info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::INDEX_BUFFER)
            .size(buf_size);
        let (buffer, allocation) =
            context.allocate_buffer(&create_info, gpu_allocator::MemoryLocation::CpuToGpu)?;

        let buffer = BufferObject {
            allocation: ManuallyDrop::new(allocation),
            buffer,
            buf_size,
            count,
            context,
            buffer_usage: vk::BufferUsageFlags::INDEX_BUFFER,
            location: gpu_allocator::MemoryLocation::CpuToGpu,
        };
        Ok(Self { buffer, index_type })
    }

    /// Wrap an already-allocated buffer (staged static-mesh upload).
    ///
    /// Ownership of the native buffer and allocation transfers into this
    /// wrapper; staged copies for device-local placements must have been
    /// queued by the allocating batch before the wrappers are used.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_native(
        context: Rc<VulkanContext>,
        buffer: vk::Buffer,
        allocation: Allocation,
        buf_size: vk::DeviceSize,
        index_type: IndexType,
        count: u32,
        usage: vk::BufferUsageFlags,
        location: gpu_allocator::MemoryLocation,
    ) -> Self {
        Self {
            buffer: BufferObject {
                allocation: ManuallyDrop::new(allocation),
                buffer,
                buf_size,
                count,
                context,
                buffer_usage: usage,
                location,
            },
            index_type,
        }
    }

    /// Upload bytes, growing the buffer when the data exceeds capacity.
    ///
    /// Returns the replaced storage for retirement when a grow happened,
    /// `None` when the data fit the existing buffer.
    pub(crate) fn upload_data(&mut self, data: &[u8]) -> Option<RetiredBuffer> {
        self.buffer.upload_data(data)
    }

    pub fn object(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    pub fn count(&self) -> u32 {
        self.buffer.count
    }

    /// Where this buffer's memory lives.
    pub fn memory_location(&self) -> gpu_allocator::MemoryLocation {
        self.buffer.location()
    }

    /// Write bytes through the host-visible mapping (fallback placement).
    pub(crate) fn write_host_visible(
        &self,
        data: &[u8],
    ) -> Result<(), crate::error::RendererError> {
        self.buffer.write_host_visible(data)
    }

    /// Byte capacity of this buffer's storage.
    pub fn capacity(&self) -> vk::DeviceSize {
        self.buffer.capacity()
    }

    /// Host-visible mapping of the whole buffer, without writing to it.
    pub fn mapped_ptr(&self) -> Result<*mut u8, crate::error::RendererError> {
        self.buffer.mapped_ptr()
    }

    /// Transfer the native buffer and allocation out without destroying them.
    ///
    /// Used by dynamic-mesh growth: the returned parts enter the retirement
    /// queue instead of being freed while in-flight submissions read them.
    pub fn into_native_parts(self) -> (vk::Buffer, Allocation) {
        self.buffer.into_native_parts()
    }
}

impl VertexBuffer {
    pub fn new(context: Rc<VulkanContext>, buf_size: u64, count: u32) -> Self {
        Self::with_usage(
            context,
            buf_size,
            count,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )
    }

    pub(crate) fn with_usage(
        context: Rc<VulkanContext>,
        buf_size: u64,
        count: u32,
        usage: vk::BufferUsageFlags,
    ) -> Self {
        Self::try_with_usage(context, buf_size, count, usage)
            .expect("Failed to allocate vertex buffer")
    }

    /// Fallible constructor used by paths that must preserve prior state on
    /// allocation failure (dynamic mesh growth).
    pub(crate) fn try_new(
        context: Rc<VulkanContext>,
        buf_size: u64,
        count: u32,
    ) -> Result<Self, crate::error::RendererError> {
        Self::try_with_usage(
            context,
            buf_size,
            count,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )
    }

    pub(crate) fn try_with_usage(
        context: Rc<VulkanContext>,
        buf_size: u64,
        count: u32,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self, crate::error::RendererError> {
        let create_info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(usage)
            .size(buf_size);
        let (buffer, allocation) =
            context.allocate_buffer(&create_info, gpu_allocator::MemoryLocation::CpuToGpu)?;

        let buffer = BufferObject {
            allocation: ManuallyDrop::new(allocation),
            buffer,
            buf_size,
            count,
            context,
            buffer_usage: usage,
            location: gpu_allocator::MemoryLocation::CpuToGpu,
        };
        Ok(Self { buffer })
    }

    /// Wrap an already-allocated buffer (staged static-mesh upload).
    ///
    /// Ownership of the native buffer and allocation transfers into this
    /// wrapper; staged copies for device-local placements must have been
    /// queued by the allocating batch before the wrappers are used.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_native(
        context: Rc<VulkanContext>,
        buffer: vk::Buffer,
        allocation: Allocation,
        buf_size: vk::DeviceSize,
        count: u32,
        usage: vk::BufferUsageFlags,
        location: gpu_allocator::MemoryLocation,
    ) -> Self {
        Self {
            buffer: BufferObject {
                allocation: ManuallyDrop::new(allocation),
                buffer,
                buf_size,
                count,
                context,
                buffer_usage: usage,
                location,
            },
        }
    }

    pub fn object(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    pub fn count(&self) -> u32 {
        self.buffer.count
    }

    /// Where this buffer's memory lives.
    pub fn memory_location(&self) -> gpu_allocator::MemoryLocation {
        self.buffer.location()
    }

    /// Write bytes through the host-visible mapping (fallback placement).
    pub(crate) fn write_host_visible(
        &self,
        data: &[u8],
    ) -> Result<(), crate::error::RendererError> {
        self.buffer.write_host_visible(data)
    }

    /// Upload bytes, growing the buffer when the data exceeds capacity.
    ///
    /// Returns the replaced storage for retirement when a grow happened,
    /// `None` when the data fit the existing buffer.
    pub(crate) fn upload_data(&mut self, data: &[u8]) -> Option<RetiredBuffer> {
        self.buffer.upload_data(data)
    }

    /// Byte capacity of this buffer's storage.
    pub fn capacity(&self) -> vk::DeviceSize {
        self.buffer.capacity()
    }

    /// Host-visible mapping of the whole buffer, without writing to it.
    pub fn mapped_ptr(&self) -> Result<*mut u8, crate::error::RendererError> {
        self.buffer.mapped_ptr()
    }

    /// Transfer the native buffer and allocation out without destroying them.
    ///
    /// Used by dynamic-mesh growth: the returned parts enter the retirement
    /// queue instead of being freed while in-flight submissions read them.
    pub fn into_native_parts(self) -> (vk::Buffer, Allocation) {
        self.buffer.into_native_parts()
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
