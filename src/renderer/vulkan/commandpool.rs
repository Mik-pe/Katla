use ash::{
    vk::{self},
    Device,
};

pub struct CommandPool {
    device: Device,
    command_pool: vk::CommandPool,
}

impl CommandPool {
    pub fn new(device: Device, queue_family_idx: u32) -> Self {
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_family_idx)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { device.create_command_pool(&create_info, None) }.unwrap();
        Self {
            device,
            command_pool,
        }
    }

    pub fn vk_command_pool(&self) -> vk::CommandPool {
        self.command_pool
    }

    pub fn create_command_buffers(&self, num_cmd_buffers: u32) -> Vec<super::CommandBuffer> {
        let allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(num_cmd_buffers);
        let vk_command_buffers = unsafe {
            self.device
                .allocate_command_buffers(&allocate_info)
                .unwrap()
        };
        let mut command_buffers = Vec::with_capacity(num_cmd_buffers as usize);
        for vk_command_buffer in vk_command_buffers {
            command_buffers.push(super::CommandBuffer {
                device: self.device.clone(),
                command_pool: self.command_pool,
                command_buffer: vk_command_buffer,
            });
        }
        command_buffers
    }

    pub fn destroy(&self) {
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
        }
    }
}
