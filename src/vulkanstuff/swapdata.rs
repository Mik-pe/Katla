use erupt::{extensions::khr_swapchain::*, vk1_0::*, DeviceLoader};

pub struct SwapData {
    frames_in_flight: usize,
    frame: usize,
    images_in_flight: Vec<Fence>,
    in_flight_fences: Vec<Fence>,
    image_available_semaphores: Vec<Semaphore>,
    render_finished_semaphores: Vec<Semaphore>,
}

impl SwapData {
    pub fn new(
        device: &DeviceLoader,
        swapchain_images: &Vec<Image>,
        frames_in_flight: usize,
    ) -> Self {
        let create_info = SemaphoreCreateInfoBuilder::new();
        let image_available_semaphores: Vec<_> = (0..frames_in_flight)
            .map(|_| unsafe { device.create_semaphore(&create_info, None, None) }.unwrap())
            .collect();
        let render_finished_semaphores: Vec<_> = (0..frames_in_flight)
            .map(|_| unsafe { device.create_semaphore(&create_info, None, None) }.unwrap())
            .collect();

        let create_info = FenceCreateInfoBuilder::new().flags(FenceCreateFlags::SIGNALED);
        let in_flight_fences: Vec<_> = (0..frames_in_flight)
            .map(|_| unsafe { device.create_fence(&create_info, None, None) }.unwrap())
            .collect();
        let images_in_flight: Vec<_> = swapchain_images.iter().map(|_| Fence::null()).collect();

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

    pub fn wait_for_fence(&self, device: &DeviceLoader) {
        unsafe {
            device
                .wait_for_fences(&[self.in_flight_fences[self.frame]], true, u64::MAX)
                .unwrap();
        }
    }

    ///Swaps the queued images and returns a tuple containing:
    ///- next available semaphore
    ///- finished semaphore
    ///- in flight fence
    ///- swapimage index
    pub fn swap_images(
        &mut self,
        device: &DeviceLoader,
        swapchain: SwapchainKHR,
    ) -> (Semaphore, Semaphore, Fence, u32) {
        let image_index = unsafe {
            device.acquire_next_image_khr(
                swapchain,
                u64::MAX,
                self.image_available_semaphores[self.frame],
                Fence::null(),
                None,
            )
        }
        .unwrap();

        let image_in_flight = self.images_in_flight[image_index as usize];
        if !image_in_flight.is_null() {
            unsafe { device.wait_for_fences(&[image_in_flight], true, u64::MAX) }.unwrap();
        }
        self.images_in_flight[image_index as usize] = self.in_flight_fences[self.frame];

        (
            self.image_available_semaphores[self.frame],
            self.render_finished_semaphores[self.frame],
            self.in_flight_fences[self.frame],
            image_index,
        )
    }

    pub fn step_frame(&mut self) {
        self.frame = (self.frame + 1) % self.frames_in_flight;
    }

    pub fn destroy(&mut self, device: &DeviceLoader) {
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
