use std::rc::Rc;

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc};

use crate::vulkan::context::VulkanContext;

pub(crate) fn create_buffer(
    context: &Rc<VulkanContext>,
    name: &str,
    size: u64,
    usage: vk::BufferUsageFlags,
    location: gpu_allocator::MemoryLocation,
) -> Result<(vk::Buffer, Allocation), String> {
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let buffer = unsafe {
        context
            .device
            .create_buffer(&buffer_info, None)
            .map_err(|e| format!("Failed to create buffer '{}': {:?}", name, e))?
    };

    let requirements = unsafe { context.device.get_buffer_memory_requirements(buffer) };

    let allocation = context
        .allocator
        .borrow_mut()
        .allocate(&AllocationCreateDesc {
            name,
            requirements,
            location,
            linear: true,
            allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
        })
        .map_err(|e| format!("Failed to allocate '{}': {}", name, e))?;

    unsafe {
        context
            .device
            .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
            .map_err(|e| format!("Failed to bind buffer '{}': {:?}", name, e))?;
    }

    Ok((buffer, allocation))
}
