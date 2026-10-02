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

        if desc.memory == BufferMemoryPolicy::DeviceLocal {
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

    pub(crate) fn write_buffer(
        &mut self,
        frame: &super::frame_scope::FrameToken,
        handle: BufferHandle,
        offset: u64,
        data: &[u8],
    ) -> Result<(), RendererError> {
        use ash::vk::Handle;
        self.frame_write_check(frame)?;
        let buffer = self
            .graph_buffers
            .get(handle)
            .ok_or_else(|| RendererError::InvalidOperation("Unknown buffer handle".into()))?;
        if let Some(&Some(fence)) = self
            .graph_buffer_consumers
            .get(&buffer.vk_buffer().as_raw())
        {
            unsafe {
                self.context
                    .device
                    .wait_for_fences(&[fence], true, u64::MAX)
            }
            .map_err(|error| {
                RendererError::VulkanError("Cannot retire buffer consumer".into(), error)
            })?;
        }
        buffer.write(offset, data)
    }

    pub(crate) fn create_buffer_with_data(
        &mut self,
        desc: BufferDesc,
        data: &[u8],
    ) -> Result<BufferHandle, RendererError> {
        if data.len() as u64 > desc.size {
            return Err(RendererError::InvalidOperation(
                "Initial buffer bytes exceed allocation".into(),
            ));
        }
        let handle = self.create_buffer(desc)?;
        let result = (|| {
            let destination = self.graph_buffers.get(handle).ok_or_else(|| {
                RendererError::InvalidOperation("Allocated buffer disappeared".into())
            })?;
            if data.is_empty() {
                return Ok(());
            }
            if desc.memory != BufferMemoryPolicy::DeviceLocal {
                return destination.write(0, data);
            }
            let info = vk::BufferCreateInfo::default()
                .size(data.len() as u64)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let (staging, allocation) = self
                .context
                .allocate_buffer(&info, gpu_allocator::MemoryLocation::CpuToGpu)?;
            let result = (|| {
                let pointer = self.context.map_buffer(&allocation)?;
                unsafe {
                    std::ptr::copy_nonoverlapping(data.as_ptr(), pointer, data.len());
                }
                self.context
                    .flush_mapped_memory(&allocation, 0, data.len() as u64)?;
                let command = self.context.begin_single_time_commands()?;
                let copy = vk::BufferCopy::default().size(data.len() as u64);
                let barrier = vk::BufferMemoryBarrier2::default()
                    .buffer(destination.vk_buffer())
                    .size(data.len() as u64)
                    .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
                unsafe {
                    self.context.device.cmd_copy_buffer(
                        command.vk_command_buffer(),
                        staging,
                        destination.vk_buffer(),
                        &[copy],
                    );
                    self.context.device.cmd_pipeline_barrier2(
                        command.vk_command_buffer(),
                        &vk::DependencyInfo::default().buffer_memory_barriers(&[barrier]),
                    );
                }
                self.context.end_single_time_commands(command)
            })();
            self.context.free_buffer(staging, allocation);
            result
        })();
        if let Err(error) = result {
            self.graph_buffers.remove(handle);
            return Err(error);
        }
        Ok(handle)
    }

    pub(crate) fn read_buffer_completed(
        &mut self,
        handle: BufferHandle,
        range: crate::render_graph::BufferByteRange,
    ) -> Result<Option<Vec<u8>>, RendererError> {
        use ash::vk::Handle;
        let buffer = self
            .graph_buffers
            .get(handle)
            .ok_or_else(|| RendererError::InvalidOperation("Unknown readback buffer".into()))?;
        if buffer.desc.memory != BufferMemoryPolicy::Readback
            || range.size == 0
            || range
                .offset
                .checked_add(range.size)
                .is_none_or(|end| end > buffer.size())
        {
            return Err(RendererError::InvalidOperation(
                "Readback requires a valid range in an explicit readback allocation".into(),
            ));
        }
        let Some(fence) = self
            .graph_buffer_consumers
            .get(&buffer.vk_buffer().as_raw())
        else {
            return Ok(None);
        };
        if let Some(fence) = fence
            && !unsafe { self.context.device.get_fence_status(*fence) }.map_err(|error| {
                RendererError::VulkanError("Cannot observe buffer owner".into(), error)
            })?
        {
            return Ok(None);
        }
        buffer.read_range(range).map(Some)
    }

    pub fn destroy_buffer(&mut self, handle: BufferHandle) -> Result<(), RendererError> {
        self.wait_for_device();
        self.graph_buffers
            .remove(handle)
            .map(|buffer| {
                use ash::vk::Handle;
                self.graph_buffer_consumers
                    .remove(&buffer.vk_buffer().as_raw());
                drop(buffer);
            })
            .ok_or_else(|| {
                RendererError::InvalidOperation(format!("Unknown buffer handle {}", handle.index()))
            })
    }
}
