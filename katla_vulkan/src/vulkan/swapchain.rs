use std::sync::Arc;

use ash::{
    extensions::khr::{Surface, Swapchain as vkSwapchainLoader},
    vk::{self, PhysicalDevice},
};

pub struct SwapchainInfo {
    pub surface_caps: vk::SurfaceCapabilitiesKHR,
    pub surface_formats: Vec<vk::SurfaceFormatKHR>,
    pub present_modes: Vec<vk::PresentModeKHR>,
}

pub struct Swapchain {
    pub swapchain_loader: Arc<vkSwapchainLoader>,
    pub swapchain_info: SwapchainInfo,
    pub swapchain: vk::SwapchainKHR,
    pub format: vk::SurfaceFormatKHR,
    //TODO: Change these to renderpasses?
    // pub swapchain_images: Vec<vk::Image>,
    // pub swapchain_image_views: Vec<vk::ImageView>,
}

impl Swapchain {
    pub fn create_swapchain(
        swapchain_loader: Arc<vkSwapchainLoader>,
        surface_loader: &Surface,
        physical_device: PhysicalDevice,
        surface: vk::SurfaceKHR,
        old_swapchain: Option<vk::SwapchainKHR>,
    ) -> Self {
        let swapchain_info =
            SwapchainInfo::query_swapchain_support(surface_loader, physical_device, surface);

        let surface_caps = &swapchain_info.surface_caps;
        let format = swapchain_info.choose_surface_format().unwrap();

        let present_mode = swapchain_info.choose_present_mode();

        let current_extent = surface_caps.current_extent;

        let mut image_count = surface_caps.min_image_count + 1;

        if surface_caps.max_image_count > 0 && image_count > surface_caps.max_image_count {
            image_count = surface_caps.max_image_count;
        }
        let old_swapchain = old_swapchain.unwrap_or(vk::SwapchainKHR::null());
        let create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(current_extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(surface_caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old_swapchain);
        let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None) }.unwrap();

        Self {
            swapchain_loader,
            swapchain_info,
            swapchain,
            format,
        }
    }

    pub fn get_swapchain_images(&self) -> Vec<vk::Image> {
        let swapchain_images =
            unsafe { self.swapchain_loader.get_swapchain_images(self.swapchain) }.unwrap();
        swapchain_images
    }

    pub fn get_extent(&self) -> vk::Extent2D {
        self.swapchain_info.surface_caps.current_extent
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
        }
    }
}

impl SwapchainInfo {
    pub fn choose_present_mode(&self) -> vk::PresentModeKHR {
        self.present_modes
            .iter()
            .find(|format| match **format {
                vk::PresentModeKHR::MAILBOX => true,
                _ => false,
            })
            .cloned()
            .unwrap_or(vk::PresentModeKHR::FIFO)
    }

    pub fn choose_surface_format(&self) -> Option<vk::SurfaceFormatKHR> {
        if self.surface_formats.is_empty() {
            None
        } else {
            for surface_format in &self.surface_formats {
                if surface_format.format == vk::Format::B8G8R8A8_SRGB
                    && surface_format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                {
                    return Some(*surface_format);
                }
            }

            Some(self.surface_formats[0])
        }
    }

    pub fn query_swapchain_support(
        surface_loader: &Surface,
        physical_device: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
    ) -> SwapchainInfo {
        unsafe {
            let surface_caps = surface_loader
                .get_physical_device_surface_capabilities(physical_device, surface)
                .unwrap();
            let surface_formats = surface_loader
                .get_physical_device_surface_formats(physical_device, surface)
                .unwrap();
            let present_modes = surface_loader
                .get_physical_device_surface_present_modes(physical_device, surface)
                .unwrap();

            SwapchainInfo {
                surface_caps,
                surface_formats,
                present_modes,
            }
        }
    }
}
