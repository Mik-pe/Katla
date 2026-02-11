use crate::sync::{VkFence, VkSemaphore};
use crate::RenderGraphError;
use ash::{khr::swapchain::Device as SwapchainDevice, vk, Device};

pub struct SwapData {
    frames_in_flight: usize,
    frame: usize,
    images_in_flight: Vec<vk::Fence>,
    in_flight_fences: Vec<vk::Fence>,
    /// Per-swapchain-image semaphores to avoid reuse issues
    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
}

impl SwapData {
    pub fn new(device: &Device, swapchain_images: &[vk::Image], frames_in_flight: usize) -> Self {
        let num_swapchain_images = swapchain_images.len();

        // Create per-frame semaphores for acquire (we don't know which image we'll get yet)
        let create_info = vk::SemaphoreCreateInfo::default();
        let image_available_semaphores: Vec<_> = (0..frames_in_flight)
            .map(|_| unsafe { device.create_semaphore(&create_info, None) }.unwrap())
            .collect();
        // Create per-swapchain-image semaphores for finished rendering
        // This prevents semaphore reuse issues when swapchain has more images than FRAMES_IN_FLIGHT
        let render_finished_semaphores: Vec<_> = (0..num_swapchain_images)
            .map(|_| unsafe { device.create_semaphore(&create_info, None) }.unwrap())
            .collect();

        let create_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let in_flight_fences: Vec<_> = (0..frames_in_flight)
            .map(|_| unsafe { device.create_fence(&create_info, None) }.unwrap())
            .collect();
        let images_in_flight: Vec<_> = swapchain_images.iter().map(|_| vk::Fence::null()).collect();

        let frame = 0;
        Self {
            frames_in_flight,
            frame,
            images_in_flight,
            in_flight_fences,
            image_available_semaphores,
            render_finished_semaphores,
        }
    }

    pub fn wait_for_fence(&self, device: &Device) {
        unsafe {
            device
                .wait_for_fences(&[self.in_flight_fences[self.frame]], true, u64::MAX)
                .unwrap();
        }
    }

    /// Swaps the queued images and returns a tuple containing:
    /// - next available semaphore
    /// - finished semaphore
    /// - in flight fence
    /// - swapimage index
    pub fn swap_images(
        &mut self,
        device: &Device,
        swapchain_loader: &SwapchainDevice,
        swapchain: vk::SwapchainKHR,
    ) -> Result<(VkSemaphore, VkSemaphore, VkFence, u32), RenderGraphError> {
        // Use per-frame semaphore for acquire (we don't know which image we'll get yet)
        let available_semaphore = self.image_available_semaphores[self.frame];

        // The second value (suboptimal) indicates whether the swapchain is no longer optimal
        // but can still be used. We ignore it and let the frame proceed normally.
        let (image_index, _) = unsafe {
            swapchain_loader.acquire_next_image(
                swapchain,
                u64::MAX,
                available_semaphore,
                vk::Fence::null(),
            )
        }
        .map_err(|err| {
            if err == vk::Result::ERROR_OUT_OF_DATE_KHR || err == vk::Result::SUBOPTIMAL_KHR {
                RenderGraphError::SwapchainOutOfDate
            } else {
                RenderGraphError::VulkanError(err)
            }
        })?;

        let image_in_flight = self.images_in_flight[image_index as usize];
        if image_in_flight != vk::Fence::null() {
            unsafe { device.wait_for_fences(&[image_in_flight], true, u64::MAX) }
                .map_err(RenderGraphError::VulkanError)?;
        }
        self.images_in_flight[image_index as usize] = self.in_flight_fences[self.frame];

        // Use per-image semaphore for finished - this prevents reuse issues
        // because each swapchain image has its own dedicated semaphore
        let finished_semaphore = self.render_finished_semaphores[image_index as usize % self.render_finished_semaphores.len()];

        Ok((
            VkSemaphore::new(available_semaphore),
            VkSemaphore::new(finished_semaphore),
            VkFence::new(self.in_flight_fences[self.frame]),
            image_index,
        ))
    }

    pub fn step_frame(&mut self) {
        self.frame = (self.frame + 1) % self.frames_in_flight;
    }

    pub fn destroy(&mut self, device: &Device) {
        unsafe {
            for &semaphore in self
                .image_available_semaphores
                .iter()
                .chain(self.render_finished_semaphores.iter())
            {
                device.destroy_semaphore(semaphore, None);
            }

            for &fence in &self.in_flight_fences {
                device.destroy_fence(fence, None);
            }
        }
    }
}
