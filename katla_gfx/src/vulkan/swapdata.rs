use ash::{Device, vk};

use crate::error::RendererError;

pub struct SwapData {
    frames_in_flight: usize,
    frame: usize,
    in_flight_fences: Vec<vk::Fence>,
    /// Per-swapchain-image semaphores to avoid reuse issues
    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
    /// Per-frame semaphores signaled when a frame's GPU work is complete.
    /// Waited on by the next frame to ensure proper synchronization across
    /// all pipeline stages (COMPUTE, TRANSFER, CLEAR, etc.).
    frame_complete_semaphores: Vec<vk::Semaphore>,
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

        let frame_complete_semaphores: Vec<_> = (0..frames_in_flight)
            .map(|_| {
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create frame complete semaphore: {:?}",
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
            in_flight_fences,
            image_available_semaphores,
            render_finished_semaphores,
            frame_complete_semaphores,
        })
    }

    pub fn wait_for_fence(&self, device: &Device) -> Result<(), RendererError> {
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
        Ok(())
    }

    pub fn step_frame(&mut self) {
        self.frame = (self.frame + 1) % self.frames_in_flight;
    }

    /// Get the current frame index (0 to frames_in_flight-1)
    pub fn current_frame(&self) -> usize {
        self.frame
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

    /// Get the frame complete semaphore for the current frame.
    /// This should be signaled when the frame's GPU work is done.
    pub fn frame_complete_semaphore(&self) -> vk::Semaphore {
        self.frame_complete_semaphores[self.frame]
    }

    /// Get the frame complete semaphore for the previous frame.
    /// This should be waited on at the start of a new frame to ensure
    /// the previous frame's GPU work (including TRANSFER/CLEAR) is complete.
    pub fn previous_frame_complete_semaphore(&self) -> vk::Semaphore {
        self.frame_complete_semaphores
            [(self.frame + self.frames_in_flight - 1) % self.frames_in_flight]
    }

    pub fn destroy(&mut self, device: &Device) {
        unsafe {
            for &semaphore in self
                .image_available_semaphores
                .iter()
                .chain(self.render_finished_semaphores.iter())
                .chain(self.frame_complete_semaphores.iter())
            {
                device.destroy_semaphore(semaphore, None);
            }

            for &fence in &self.in_flight_fences {
                device.destroy_fence(fence, None);
            }
        }
    }
}
