use crate::error::RendererError;
use crate::handle::BufferHandle;
use crate::render_graph::{BufferDesc, BufferMemoryPolicy, BufferUsages};
use crate::renderer::VulkanRenderer;
use ash::vk;

impl VulkanRenderer {
    pub fn create_buffer(&mut self, desc: BufferDesc) -> Result<BufferHandle, RendererError> {
        if desc.size == 0 || desc.usages.is_empty() {
            return Err(RendererError::InvalidOperation(
                "Buffer size and usage must be non-empty".into(),
            ));
        }

        let mut usage = vk::BufferUsageFlags::empty();
        if desc.usages.contains(BufferUsages::UNIFORM) {
            usage |= vk::BufferUsageFlags::UNIFORM_BUFFER;
        }
        if desc.usages.contains(BufferUsages::STORAGE) {
            usage |= vk::BufferUsageFlags::STORAGE_BUFFER;
        }
        if desc.usages.contains(BufferUsages::VERTEX) {
            usage |= vk::BufferUsageFlags::VERTEX_BUFFER;
        }
        if desc.usages.contains(BufferUsages::INDEX) {
            usage |= vk::BufferUsageFlags::INDEX_BUFFER;
        }
        if desc.usages.contains(BufferUsages::INDIRECT) {
            usage |= vk::BufferUsageFlags::INDIRECT_BUFFER;
        }
        if desc.usages.contains(BufferUsages::TRANSFER_SOURCE) {
            usage |= vk::BufferUsageFlags::TRANSFER_SRC;
        }
        if desc.usages.contains(BufferUsages::TRANSFER_DESTINATION)
            || desc.usages.contains(BufferUsages::READBACK)
        {
            usage |= vk::BufferUsageFlags::TRANSFER_DST;
        }

        let memory = match desc.memory {
            BufferMemoryPolicy::DeviceLocal => gpu_allocator::MemoryLocation::GpuOnly,
            BufferMemoryPolicy::CpuVisible => gpu_allocator::MemoryLocation::CpuToGpu,
            BufferMemoryPolicy::Readback => gpu_allocator::MemoryLocation::GpuToCpu,
        };
        let create_info = vk::BufferCreateInfo::default()
            .size(desc.size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (buffer, allocation) = self.context.allocate_buffer_named(
            &create_info,
            memory,
            "Render Graph Imported Buffer",
        )?;
        Ok(self.graph_buffers.insert(
            crate::render_graph::transient_buffer::VulkanGraphBuffer::new(
                self.context.clone(),
                buffer,
                allocation,
                desc,
            ),
        ))
    }

    pub fn destroy_buffer(&mut self, handle: BufferHandle) -> Result<(), RendererError> {
        self.wait_for_device();
        self.graph_buffers.remove(handle).map(drop).ok_or_else(|| {
            RendererError::InvalidOperation(format!("Unknown buffer handle {}", handle.index()))
        })
    }
}
