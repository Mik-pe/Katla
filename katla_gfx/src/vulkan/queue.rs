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
        let stage_masks: Vec<_> = (0..wait_semaphores.len())
            .map(|_| vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .collect();
        self.submit_with_stages(
            command_buffers,
            wait_semaphores,
            signal_semaphores,
            signal_fence,
            &stage_masks,
        )
    }

    /// Submit command buffers with per-semaphore wait stage masks.
    ///
    /// Each semaphore in `wait_semaphores` is paired with a stage mask from `wait_stage_masks`.
    /// This allows different semaphores to block at different pipeline stages.
    pub fn submit_with_stages(
        &self,
        command_buffers: &[&CommandBuffer],
        wait_semaphores: &[Semaphore],
        signal_semaphores: &[Semaphore],
        signal_fence: Fence,
        wait_stage_masks: &[vk::PipelineStageFlags],
    ) {
        let vk_cmd_buffers: Vec<_> = command_buffers
            .iter()
            .map(|cb| cb.vk_command_buffer())
            .collect();

        let submit_info = vk::SubmitInfo::default()
            .wait_dst_stage_mask(wait_stage_masks)
            .wait_semaphores(wait_semaphores)
            .signal_semaphores(signal_semaphores)
            .command_buffers(&vk_cmd_buffers);

        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info], signal_fence)
                .unwrap();
        }
    }

    /// Submit command buffers and wait for completion.
    ///
    /// Creates a temporary fence, submits the command buffers, waits for completion,
    /// and destroys the fence. Useful for one-time operations like texture uploads.
    ///
    /// # Arguments
    /// * `command_buffers` - Command buffers to submit
    /// * `wait_semaphores` - Semaphores to wait on before execution (optional)
    /// * `signal_semaphores` - Semaphores to signal after completion (optional)
    pub fn submit_and_wait(
        &self,
        command_buffers: &[&CommandBuffer],
        wait_semaphores: &[Semaphore],
        signal_semaphores: &[Semaphore],
    ) {
        // Create fence for this operation
        let fence_info = vk::FenceCreateInfo::default();
        let fence = unsafe {
            self.device
                .create_fence(&fence_info, None)
                .expect("Failed to create fence for submit_and_wait")
        };

        // Submit with fence
        self.submit(command_buffers, wait_semaphores, signal_semaphores, fence);

        // Wait for completion
        unsafe {
            self.device
                .wait_for_fences(&[fence], true, u64::MAX)
                .expect("Failed to wait for fence in submit_and_wait");
            self.device.destroy_fence(fence, None);
        }
    }

}
