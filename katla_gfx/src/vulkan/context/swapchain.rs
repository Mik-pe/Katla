use std::rc::Rc;

use ash::{Device, vk};
use gpu_allocator::{MemoryLocation, vulkan::Allocation};

use crate::sync::{VkImage, VkImageView};

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

            let image_memory = std::mem::take(&mut self.image_memory);
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
    ) -> Result<vk::ImageView, RendererError> {
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
        unsafe { device.create_image_view(&create_info, None) }.map_err(|error| {
            RendererError::VulkanError("Failed to create image view".into(), error)
        })
    }

    fn create_output_views(
        device: &Device,
        images: &[vk::Image],
        format: vk::Format,
    ) -> Result<Vec<VkImageView>, RendererError> {
        let mut views: Vec<VkImageView> = Vec::with_capacity(images.len());
        for &image in images {
            match Self::create_image_view(device, image, format, vk::ImageAspectFlags::COLOR) {
                Ok(view) => views.push(VkImageView::new(view)),
                Err(error) => {
                    for view in views {
                        unsafe { device.destroy_image_view(view.vk(), None) };
                    }
                    return Err(error);
                }
            }
        }
        Ok(views)
    }

    pub fn init(
        context: &Rc<VulkanContext>,
        extent: vk::Extent2D,
    ) -> Result<Self, crate::error::RendererError> {
        let command_buffers = context
            .gfx_cmdpool
            .create_command_buffers(crate::renderer::FRAMES_IN_FLIGHT as u32)?
            .into_iter()
            .map(Rc::new)
            .collect();

        let (swapchain_loader, surface_loader, surface) =
            context.window_resources().ok_or_else(|| {
                crate::error::RendererError::InitializationFailed(
                    "VulkanFrameCtx requires a window surface (not available in headless mode)"
                        .to_string(),
                )
            })?;

        let swapchain = super::super::Swapchain::create_swapchain(
            context.gfx_cmdpool.owner.native.clone(),
            swapchain_loader.clone(),
            surface_loader,
            context.physical_device,
            surface,
            None,
            extent,
        )?;

        let swapchain_images = swapchain.get_swapchain_images()?;

        let swapchain_image_views =
            Self::create_output_views(&context.device, &swapchain_images, swapchain.format.format)?;
        let swapchain_images_wrapped: Vec<VkImage> = swapchain_images
            .iter()
            .map(|img| VkImage::new(*img))
            .collect();

        Ok(Self {
            context: context.clone(),
            extent: swapchain.get_extent(),
            swapchain: Some(swapchain),
            offscreen_targets: Vec::new(),
            swapchain_image_views,
            swapchain_image_layouts: (0..swapchain_images_wrapped.len())
                .map(|_| std::cell::Cell::new(vk::ImageLayout::UNDEFINED))
                .collect(),
            swapchain_image_contents: (0..swapchain_images_wrapped.len())
                .map(|_| std::cell::Cell::new(false))
                .collect(),
            pending_output_contents: std::cell::Cell::new(None),
            pending_transient_layouts: Default::default(),
            swapchain_images: swapchain_images_wrapped,
            command_buffers,
        })
    }

    pub fn init_headless(
        context: &Rc<VulkanContext>,
        extent: vk::Extent2D,
    ) -> Result<Self, crate::error::RendererError> {
        let command_buffers = context
            .gfx_cmdpool
            .create_command_buffers(crate::renderer::FRAMES_IN_FLIGHT as u32)?
            .into_iter()
            .map(Rc::new)
            .collect();

        let mut images = Vec::new();
        let mut views = Vec::new();
        let mut targets = Vec::new();
        for _ in 0..crate::renderer::FRAMES_IN_FLIGHT {
            let info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::B8G8R8A8_SRGB)
                .extent(vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::TRANSFER_DST,
                );
            let (image, allocation) = context.create_image(info, MemoryLocation::GpuOnly)?;
            let view = match Self::create_image_view(
                &context.device,
                image,
                vk::Format::B8G8R8A8_SRGB,
                vk::ImageAspectFlags::COLOR,
            ) {
                Ok(view) => VkImageView::new(view),
                Err(error) => {
                    context.free_image(VkImage::new(image), allocation);
                    return Err(error);
                }
            };
            views.push(view);
            images.push(VkImage::new(image));
            targets.push(RenderTexture {
                image_view: view,
                depth_stencil_image_view: None,
                image: VkImage::new(image),
                image_memory: allocation,
                context: context.clone(),
            });
        }
        Ok(Self {
            context: context.clone(),
            swapchain: None,
            extent,
            swapchain_image_layouts: (0..images.len())
                .map(|_| std::cell::Cell::new(vk::ImageLayout::UNDEFINED))
                .collect(),
            swapchain_image_contents: (0..images.len())
                .map(|_| std::cell::Cell::new(false))
                .collect(),
            pending_output_contents: std::cell::Cell::new(None),
            pending_transient_layouts: Default::default(),
            swapchain_images: images,
            swapchain_image_views: views,
            offscreen_targets: targets,
            command_buffers,
        })
    }

    pub fn recreate_swapchain(
        &mut self,
        extent: vk::Extent2D,
    ) -> Result<(), crate::error::RendererError> {
        if extent.width == 0 || extent.height == 0 {
            return Err(crate::error::RendererError::InvalidOperation(
                "Output dimensions must be nonzero".into(),
            ));
        }
        if self.swapchain.is_none() {
            let replacement = Self::init_headless(&self.context, extent)?;
            self.pending_transient_layouts.borrow_mut().rollback();
            self.destroy();
            *self = replacement;
            return Ok(());
        }
        let (swapchain_loader, surface_loader, surface) =
            self.context.window_resources().ok_or_else(|| {
                crate::error::RendererError::InitializationFailed(
                    "VulkanFrameCtx requires a window surface (not available in headless mode)"
                        .to_string(),
                )
            })?;

        let swapchain = super::super::Swapchain::create_swapchain(
            self.context.gfx_cmdpool.owner.native.clone(),
            swapchain_loader.clone(),
            surface_loader,
            self.context.physical_device,
            surface,
            self.swapchain.as_ref().map(|s| s.swapchain),
            extent,
        )?;
        let swapchain_images = swapchain.get_swapchain_images()?;
        let views = Self::create_output_views(
            &self.context.device,
            &swapchain_images,
            swapchain.format.format,
        )?;
        self.destroy_targets();
        self.extent = swapchain.get_extent();

        self.swapchain_images = swapchain_images
            .iter()
            .map(|img| VkImage::new(*img))
            .collect();

        self.swapchain_image_layouts = (0..swapchain_images.len())
            .map(|_| std::cell::Cell::new(vk::ImageLayout::UNDEFINED))
            .collect();
        self.swapchain_image_contents = (0..swapchain_images.len())
            .map(|_| std::cell::Cell::new(false))
            .collect();
        self.pending_output_contents.set(None);
        self.pending_transient_layouts.borrow_mut().rollback();

        self.swapchain_image_views = views;
        self.swapchain = Some(swapchain);
        Ok(())
    }

    pub fn destroy(&mut self) {
        self.command_buffers.clear();
        self.destroy_targets();
    }

    fn destroy_targets(&mut self) {
        unsafe {
            if let Some(swapchain) = self.swapchain.take() {
                for image_view in &self.swapchain_image_views {
                    self.context
                        .device
                        .destroy_image_view(image_view.vk(), None);
                }
                drop(swapchain);
            }
            self.offscreen_targets.clear();
            self.swapchain_image_views.clear();
        }
    }
}

impl Drop for VulkanFrameCtx {
    fn drop(&mut self) {
        if self.command_buffers.is_empty()
            && self.swapchain.is_none()
            && self.offscreen_targets.is_empty()
        {
            return;
        }
        unsafe {
            let _ = self.context.device.device_wait_idle();
        }
        self.destroy();
    }
}
