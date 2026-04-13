use ash::{
    Device,
    vk::{self},
};

pub struct CommandPool {
    pub(crate) device: Device,
    pub(crate) command_pool: vk::CommandPool,
}

impl CommandPool {
    pub fn new(device: Device, queue_family_idx: u32) -> Self {
        let create_info = vk::CommandPoolCreateInfo::default()
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
        let mut command_buffers = Vec::with_capacity(num_cmd_buffers as usize);
        for _ in 0..num_cmd_buffers {
            command_buffers.push(super::CommandBuffer::new(&self.device, self));
        }
        command_buffers
    }

    pub fn allocate_secondary(&self) -> super::CommandBuffer {
        super::CommandBuffer::new_secondary(&self.device, self)
    }

    pub fn reset(&self) {
        unsafe {
            self.device
                .reset_command_pool(self.command_pool, vk::CommandPoolResetFlags::empty())
                .expect("Failed to reset command pool");
        }
    }

    pub fn destroy(&self) {
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
        }
    }
}
