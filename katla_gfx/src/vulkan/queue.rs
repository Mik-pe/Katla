use super::CommandBuffer;

use ash::Device;
use ash::vk::{self, Fence, Semaphore};

pub struct Queue {
    device: Device,
    queue: vk::Queue,
}

impl Queue {
    pub fn new(device: Device, queue_family_index: u32, queue_index: u32) -> Self {
        let queue = unsafe { device.get_device_queue(queue_family_index, queue_index) };

        Self { device, queue }
    }

    pub fn wait_idle(&self) {
        unsafe {
            let _ = self.device.queue_wait_idle(self.queue);
        }
    }

    /// Get the raw Vulkan queue handle.
    pub fn vk_queue(&self) -> vk::Queue {
        self.queue
    }

    pub fn submit(
        &self,
        command_buffers: &[&CommandBuffer],
        wait_semaphores: &[Semaphore],
        signal_semaphores: &[Semaphore],
        signal_fence: Fence,
    ) {
        let vk_cmd_buffers: Vec<_> = command_buffers
            .iter()
            .map(|cb| cb.vk_command_buffer())
            .collect();

        let mut wait_dst_stage_mask = Vec::with_capacity(wait_semaphores.len());
        for _ in wait_semaphores {
            wait_dst_stage_mask.push(vk::PipelineStageFlags::ALL_COMMANDS);
        }

        let submit_info = vk::SubmitInfo::default()
            .wait_dst_stage_mask(&wait_dst_stage_mask)
            .wait_semaphores(wait_semaphores)
            .signal_semaphores(signal_semaphores)
            .command_buffers(&vk_cmd_buffers);

        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info], signal_fence)
                .unwrap();
        }
    }

    pub fn present(
        &self,
        signal_semaphores: &[Semaphore],
        image_indices: &[u32],
        swapchains: &[vk::SwapchainKHR],
    ) {
        let _present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(signal_semaphores)
            .swapchains(swapchains)
            .image_indices(image_indices);

        // unsafe {
        //     self.context
        //         .swapchain_loader
        //         .queue_present(self.queue, &present_info)
        // }
        // .unwrap();
    }
}
