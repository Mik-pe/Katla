use std::rc::Rc;

use ash::{Device, vk};
use gpu_allocator::{MemoryLocation, vulkan::Allocation};

use crate::{
    barrier::ImageBarrier,
    sync::{VkImage, VkImageView},
};

use super::*;

pub struct RenderTexture {
    pub(crate) image_view: VkImageView,
    /// Image view with both DEPTH and STENCIL aspects.
    /// `None` if the depth format has no stencil component (e.g., D32_SFLOAT).
    pub(crate) depth_stencil_image_view: Option<VkImageView>,
    pub(crate) image: VkImage,
    pub image_memory: Allocation,
    pub context: Rc<VulkanContext>,
}

impl RenderTexture {
    fn destroy(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_image_view(self.image_view.vk(), None);
            if let Some(ds_view) = self.depth_stencil_image_view.take() {
                self.context.device.destroy_image_view(ds_view.vk(), None);
            }

            let image_memory = std::ptr::read(&self.image_memory);
            self.context.free_image(self.image, image_memory);
        }
    }
}

impl Drop for RenderTexture {
    fn drop(&mut self) {
        self.destroy();
    }
}

impl VulkanFrameCtx {
    pub fn create_image_view(
        device: &Device,
        image: vk::Image,
        format: vk::Format,
        aspect_mask: vk::ImageAspectFlags,
    ) -> vk::ImageView {
        let subresource_range = vk::ImageSubresourceRange::default()
            .aspect_mask(aspect_mask)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);
        let create_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::IDENTITY,
                g: vk::ComponentSwizzle::IDENTITY,
                b: vk::ComponentSwizzle::IDENTITY,
                a: vk::ComponentSwizzle::IDENTITY,
            })
            .subresource_range(subresource_range);
        unsafe { device.create_image_view(&create_info, None) }.unwrap()
    }

    pub fn init(context: &Rc<VulkanContext>) -> Result<Self, crate::error::RendererError> {
        let (swapchain_loader, surface_loader, surface) = context
            .window_resources()
            .ok_or_else(|| crate::error::RendererError::InitializationFailed(
                "VulkanFrameCtx requires a window surface (not available in headless mode)".to_string(),
            ))?;

        let swapchain = super::super::Swapchain::create_swapchain(
            swapchain_loader.clone(),
            surface_loader,
            context.physical_device,
            surface,
            None,
        )?;

        let swapchain_images = swapchain.get_swapchain_images()?;

        let swapchain_image_views: Vec<VkImageView> = swapchain_images
            .iter()
            .map(|swapchain_image| {
                VkImageView::new(Self::create_image_view(
                    &context.device,
                    *swapchain_image,
                    swapchain.format.format,
                    vk::ImageAspectFlags::COLOR,
                ))
            })
            .collect();
        let swapchain_images_wrapped: Vec<VkImage> = swapchain_images
            .iter()
            .map(|img| VkImage::new(*img))
            .collect();

        const FRAMES_IN_FLIGHT: usize = 2;
        let depth_render_textures: Vec<RenderTexture> = (0..FRAMES_IN_FLIGHT)
            .map(|_| create_depth_render_texture(context.clone(), swapchain.get_extent()))
            .collect();

        let command_buffers = context
            .gfx_cmdpool
            .create_command_buffers(swapchain_image_views.len() as _);

        Ok(Self {
            context: context.clone(),
            swapchain,
            swapchain_image_views,
            swapchain_images: swapchain_images_wrapped,
            depth_render_textures,
            command_buffers,
        })
    }

    pub fn recreate_swapchain(&mut self) -> Result<(), crate::error::RendererError> {
        let (swapchain_loader, surface_loader, surface) = self
            .context
            .window_resources()
            .ok_or_else(|| crate::error::RendererError::InitializationFailed(
                "VulkanFrameCtx requires a window surface (not available in headless mode)".to_string(),
            ))?;

        let swapchain = super::super::Swapchain::create_swapchain(
            swapchain_loader.clone(),
            surface_loader,
            self.context.physical_device,
            surface,
            Some(self.swapchain.swapchain),
        )?;
        self.destroy();
        self.swapchain = swapchain;

        let swapchain_images = self.swapchain.get_swapchain_images()?;

        self.swapchain_images = swapchain_images
            .iter()
            .map(|img| VkImage::new(*img))
            .collect();

        self.swapchain_image_views = swapchain_images
            .iter()
            .map(|swapchain_image| {
                VkImageView::new(Self::create_image_view(
                    &self.context.device,
                    *swapchain_image,
                    self.swapchain.format.format,
                    vk::ImageAspectFlags::COLOR,
                ))
            })
            .collect();
        const FRAMES_IN_FLIGHT: usize = 2;
        self.depth_render_textures = (0..FRAMES_IN_FLIGHT)
            .map(|_| create_depth_render_texture(self.context.clone(), self.swapchain.get_extent()))
            .collect();
        Ok(())
    }

    pub fn destroy(&mut self) {
        unsafe {
            for image_view in &self.swapchain_image_views {
                self.context
                    .device
                    .destroy_image_view(image_view.vk(), None);
            }
            self.swapchain.destroy();
        }
    }
}

fn create_depth_render_texture(context: Rc<VulkanContext>, extent: vk::Extent2D) -> RenderTexture {
    let depth_format = context
        .find_depth_format()
        .expect("Failed to find depth format");
    let extent_3d = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let create_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .mip_levels(1)
        .array_layers(1)
        .format(depth_format)
        .extent(extent_3d)
        .tiling(vk::ImageTiling::OPTIMAL)
        .samples(vk::SampleCountFlags::TYPE_1)
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED);

    let (depth_image, image_memory) = context
        .create_image(create_info, MemoryLocation::GpuOnly)
        .expect("Failed to create depth image");

    let image_view = VulkanFrameCtx::create_image_view(
        &context.device,
        depth_image,
        depth_format,
        vk::ImageAspectFlags::DEPTH,
    );

    let has_stencil = matches!(
        depth_format,
        vk::Format::D32_SFLOAT_S8_UINT | vk::Format::D24_UNORM_S8_UINT
    );

    let depth_stencil_image_view = if has_stencil {
        Some(VkImageView::new(VulkanFrameCtx::create_image_view(
            &context.device,
            depth_image,
            depth_format,
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
        )))
    } else {
        None
    };

    let cmd_buffer = context
        .begin_single_time_commands()
        .expect("Failed to begin single-time commands");
    let cmd = cmd_buffer.vk_command_buffer();

    let depth_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };

    ImageBarrier::transition_from_undefined_with_range(
        &cmd,
        &context.device,
        depth_image,
        vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        depth_range,
    );

    context
        .end_single_time_commands(cmd_buffer)
        .expect("Failed to end single-time commands");

    RenderTexture {
        image_view: VkImageView::new(image_view),
        depth_stencil_image_view,
        image: VkImage::new(depth_image),
        image_memory,
        context,
    }
}
