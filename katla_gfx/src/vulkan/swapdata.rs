use ash::{Device, vk};

use crate::error::RendererError;

pub struct SwapData {
    frames_in_flight: usize,
    frame: usize,
    /// Monotonic frame counter (never wraps), unlike `frame` which cycles
    /// through frame slots. Resource retirement ages are measured with this.
    frame_counter: u64,
    in_flight_fences: Vec<vk::Fence>,
    submitted: Vec<bool>,
    /// Per-swapchain-image semaphores to avoid reuse issues
    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
}

impl SwapData {
    pub(crate) fn new(
        device: &Device,
        swapchain_images: &[vk::Image],
        frames_in_flight: usize,
    ) -> Result<Self, RendererError> {
        let num_swapchain_images = swapchain_images.len();

        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let image_available_semaphores: Vec<_> = (0..frames_in_flight)
            .map(|_| {
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create image available semaphore: {:?}",
                        e
                    ))
                })
            })
            .collect::<Result<_, _>>()?;
        let render_finished_semaphores: Vec<_> = (0..num_swapchain_images)
            .map(|_| {
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create render finished semaphore: {:?}",
                        e
                    ))
                })
            })
            .collect::<Result<_, _>>()?;

        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let in_flight_fences: Vec<_> = (0..frames_in_flight)
            .map(|_| {
                unsafe { device.create_fence(&fence_info, None) }.map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create in-flight fence: {:?}",
                        e
                    ))
                })
            })
            .collect::<Result<_, _>>()?;

        let frame = 0;
        Ok(Self {
            frames_in_flight,
            frame,
            frame_counter: 0,
            in_flight_fences,
            submitted: vec![false; frames_in_flight],
            image_available_semaphores,
            render_finished_semaphores,
        })
    }

    pub fn wait_for_fence(&mut self, device: &Device) -> Result<(), RendererError> {
        if !self.submitted[self.frame] {
            return Ok(());
        }
        unsafe {
            device
                .wait_for_fences(&[self.in_flight_fences[self.frame]], true, u64::MAX)
                .map_err(|e| {
                    RendererError::SwapchainError(format!(
                        "Failed to wait for in-flight fence: {:?}",
                        e
                    ))
                })?;
        }
        self.submitted[self.frame] = false;
        Ok(())
    }

    pub(crate) fn mark_submitted(&mut self) {
        self.submitted[self.frame] = true;
    }

    pub fn step_frame(&mut self) {
        self.frame = (self.frame + 1) % self.frames_in_flight;
        self.frame_counter += 1;
    }

    /// Get the current frame index (0 to frames_in_flight-1)
    pub fn current_frame(&self) -> usize {
        self.frame
    }

    /// Monotonic count of frames started since renderer creation.
    pub fn frame_counter(&self) -> u64 {
        self.frame_counter
    }

    /// Number of frame slots that may have uncompleted submissions.
    pub fn frames_in_flight(&self) -> usize {
        self.frames_in_flight
    }

    /// Get the image available semaphore for the current frame
    pub fn image_available_semaphore(&self) -> vk::Semaphore {
        self.image_available_semaphores[self.frame]
    }

    /// Get the render finished semaphore for a specific swapchain image
    pub fn render_finished_semaphore(&self, image_index: u32) -> vk::Semaphore {
        self.render_finished_semaphores[image_index as usize]
    }

    /// Get the in-flight fence for the current frame
    pub fn in_flight_fence(&self) -> vk::Fence {
        self.in_flight_fences[self.frame]
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
