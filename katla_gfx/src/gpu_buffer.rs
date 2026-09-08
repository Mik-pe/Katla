use std::rc::Rc;

use ash::vk;

use crate::vulkan::context::VulkanContext;

pub(crate) fn create_buffer(
    context: &Rc<VulkanContext>,
    name: &str,
    size: u64,
    usage: vk::BufferUsageFlags,
    location: gpu_allocator::MemoryLocation,
) -> Result<(vk::Buffer, gpu_allocator::vulkan::Allocation), String> {
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    context
        .allocate_buffer_named(&buffer_info, location, name)
        .map_err(|e| format!("Failed to create buffer '{}': {}", name, e))
}
