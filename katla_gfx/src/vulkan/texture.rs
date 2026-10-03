//! Vulkan image ownership and validated descriptor-based texture creation.

#[path = "texture_upload.rs"]
mod upload;

use super::context::VulkanContext;
use crate::{
    RendererError,
    sync::{VkImage, VkImageView},
    texture::{ImageFormat, TextureDescriptor, TextureUsage},
};
use ash::vk;
use gpu_allocator::vulkan::Allocation;
use std::{mem::ManuallyDrop, rc::Rc};

pub struct Texture {
    pub(crate) width: u32,
    pub(crate) height: u32,
    descriptor: TextureDescriptor,
    image_memory: ManuallyDrop<Allocation>,
    image: VkImage,
    pub(crate) image_view: VkImageView,
    context: Rc<VulkanContext>,
}

impl Texture {
    /// Allocate and upload a validated 2D texture, optionally generating filtered mips.
    /// Creation errors release every resource without inserting a texture handle.
    pub fn from_descriptor(
        context: &Rc<VulkanContext>,
        desc: &TextureDescriptor,
        pixels: &[u8],
    ) -> Result<Self, RendererError> {
        desc.validate_data(pixels.len())?;
        if desc.depth != 1 || desc.array_layers != 1 || desc.format.block_extent() != [1, 1] {
            return Err(RendererError::UnsupportedFeature(format!(
                "Vulkan asset texture creation does not support {:?} {}x{}x{} layers {}",
                desc.format, desc.width, desc.height, desc.depth, desc.array_layers
            )));
        }
        upload::validate_mip_generation(context, desc)?;
        let mut usage = vk::ImageUsageFlags::TRANSFER_DST;
        if desc.generate_mips {
            usage |= vk::ImageUsageFlags::TRANSFER_SRC;
        }
        if desc.usage.contains(TextureUsage::SAMPLED) {
            usage |= vk::ImageUsageFlags::SAMPLED;
        }
        if desc.usage.contains(TextureUsage::STORAGE) {
            usage |= vk::ImageUsageFlags::STORAGE;
        }
        if desc.usage.contains(TextureUsage::COLOR_ATTACHMENT) {
            usage |= vk::ImageUsageFlags::COLOR_ATTACHMENT;
        }
        if desc.usage.contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT) {
            usage |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
        }
        let info = vk::ImageCreateInfo::default()
            .extent(vk::Extent3D {
                width: desc.width,
                height: desc.height,
                depth: 1,
            })
            .image_type(vk::ImageType::TYPE_2D)
            .mip_levels(desc.mip_levels)
            .array_layers(1)
            .format(desc.format.into())
            .usage(usage)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .tiling(vk::ImageTiling::OPTIMAL)
            .samples(vk::SampleCountFlags::TYPE_1)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (image, allocation) =
            context.create_image(info, gpu_allocator::MemoryLocation::GpuOnly)?;
        let mut texture = Self {
            width: desc.width,
            height: desc.height,
            descriptor: desc.clone(),
            image_memory: ManuallyDrop::new(allocation),
            image: VkImage::new(image),
            image_view: VkImageView::new(vk::ImageView::null()),
            context: context.clone(),
        };
        let view = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(desc.format.into())
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(upload::image_aspects(desc.format))
                    .level_count(desc.mip_levels)
                    .layer_count(1),
            );
        let view = unsafe { context.device.create_image_view(&view, None) }.map_err(|error| {
            RendererError::VulkanError("Failed to create texture image view".into(), error)
        })?;
        texture.image_view = VkImageView::new(view);
        upload::upload(context, image, desc, pixels, vk::ImageLayout::UNDEFINED)?;
        Ok(texture)
    }

    /// Upload the complete base level and regenerate this texture's authored mip chain.
    /// The graphics queue orders these writes before later draws and retains staging.
    pub fn update_data(&self, pixels: &[u8]) -> Result<(), RendererError> {
        self.descriptor.validate_data(pixels.len())?;
        let expected = self
            .descriptor
            .expected_bytes()
            .ok_or_else(|| RendererError::InvalidOperation("Texture byte count overflow".into()))?;
        if pixels.len() != expected {
            return Err(RendererError::UploadFailed {
                resource: "texture".into(),
                expected_bytes: expected,
                actual_bytes: pixels.len(),
                detail: format!(
                    "{}x{} {:?}",
                    self.descriptor.width, self.descriptor.height, self.descriptor.format
                ),
            });
        }
        upload::upload(
            &self.context,
            self.image.vk(),
            &self.descriptor,
            pixels,
            upload::final_layout(&self.descriptor),
        )
    }

    pub(crate) fn image(&self) -> VkImage {
        self.image
    }
    pub(crate) fn format(&self) -> ImageFormat {
        self.descriptor.format
    }
    /// Native view spanning all allocated mip levels.
    pub fn image_view(&self) -> &VkImageView {
        &self.image_view
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_image_view(self.image_view.vk(), None);
        }
        let allocation = unsafe { ManuallyDrop::take(&mut self.image_memory) };
        if let Some(allocation) = self.context.retire_upload_image(self.image, allocation) {
            self.context.free_image(self.image, allocation);
        }
    }
}
