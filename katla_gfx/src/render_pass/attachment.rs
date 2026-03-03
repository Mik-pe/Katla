//! Attachment resource management for render passes.
//!
//! This module provides types for managing GPU images used as render pass attachments.

use ash::vk;
use gpu_allocator::vulkan::Allocation;
use std::collections::HashMap;
use std::rc::Rc;

use crate::sync::{VkImage, VkImageView};
use crate::texture::ImageFormat;
use crate::vulkan::context::VulkanContext;

/// A single attachment resource (image + view + memory).
struct Attachment {
    image: VkImage,
    image_view: VkImageView,
    allocation: Allocation,
    format: vk::Format,
    extent: vk::Extent2D,
}

impl Attachment {
    fn new(
        image: vk::Image,
        image_view: vk::ImageView,
        allocation: Allocation,
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> Self {
        Self {
            image: VkImage::new(image),
            image_view: VkImageView::new(image_view),
            allocation,
            format,
            extent,
        }
    }
}

/// Manages attachment resources for render passes.
///
/// This struct stores named attachments that can be shared between render passes.
/// Attachments are created on demand and destroyed when no longer needed.
pub struct AttachmentResources {
    attachments: HashMap<String, Attachment>,
}

impl AttachmentResources {
    /// Create a new empty attachment resources manager.
    pub fn new() -> Self {
        Self {
            attachments: HashMap::new(),
        }
    }

    /// Create a new attachment with the given parameters.
    ///
    /// # Arguments
    /// * `context` - Vulkan context for resource creation
    /// * `name` - Unique name for this attachment
    /// * `format` - Image format
    /// * `extent` - Image dimensions
    /// * `usage` - Image usage flags (e.g., COLOR_ATTACHMENT, SAMPLED)
    ///
    /// # Returns
    /// `Ok(())` if creation succeeded, or a Vulkan error.
    pub fn create_attachment(
        &mut self,
        context: &Rc<VulkanContext>,
        name: &str,
        format: ImageFormat,
        extent: vk::Extent2D,
        usage: vk::ImageUsageFlags,
    ) -> Result<(), vk::Result> {
        let vk_format: vk::Format = format.into();
        let extent_3d = vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        };

        let create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .mip_levels(1)
            .array_layers(1)
            .format(vk_format)
            .extent(extent_3d)
            .tiling(vk::ImageTiling::OPTIMAL)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(usage);

        let (image, allocation) =
            context.create_image(create_info, gpu_allocator::MemoryLocation::GpuOnly);

        let aspect_mask = Self::get_aspect_mask(vk_format);
        let image_view = Self::create_image_view(&context.device, image, vk_format, aspect_mask)?;

        let attachment = Attachment::new(image, image_view, allocation, vk_format, extent);
        self.attachments.insert(name.to_string(), attachment);

        Ok(())
    }

    /// Get the Vulkan image view for an attachment by name.
    pub fn get_view(&self, name: &str) -> Option<vk::ImageView> {
        self.attachments.get(name).map(|a| a.image_view.vk())
    }

    /// Get the Vulkan image for an attachment by name.
    pub fn get_image(&self, name: &str) -> Option<vk::Image> {
        self.attachments.get(name).map(|a| a.image.vk())
    }

    /// Get the format of an attachment by name.
    pub fn get_format(&self, name: &str) -> Option<vk::Format> {
        self.attachments.get(name).map(|a| a.format)
    }

    /// Get the extent of an attachment by name.
    pub fn get_extent(&self, name: &str) -> Option<vk::Extent2D> {
        self.attachments.get(name).map(|a| a.extent)
    }

    /// Check if an attachment exists.
    pub fn contains(&self, name: &str) -> bool {
        self.attachments.contains_key(name)
    }

    /// Get the number of attachments.
    pub fn len(&self) -> usize {
        self.attachments.len()
    }

    /// Check if there are no attachments.
    pub fn is_empty(&self) -> bool {
        self.attachments.is_empty()
    }

    /// Remove and destroy a specific attachment.
    pub fn remove_attachment(&mut self, device: &ash::Device, name: &str) {
        if let Some(attachment) = self.attachments.remove(name) {
            unsafe {
                device.destroy_image_view(attachment.image_view.vk(), None);
                device.destroy_image(attachment.image.vk(), None);
            }
        }
    }

    /// Destroy all attachments.
    pub fn destroy(&mut self, device: &ash::Device) {
        for (_, attachment) in self.attachments.drain() {
            unsafe {
                device.destroy_image_view(attachment.image_view.vk(), None);
                device.destroy_image(attachment.image.vk(), None);
            }
        }
    }

    /// Create an image view for an attachment.
    fn create_image_view(
        device: &ash::Device,
        image: vk::Image,
        format: vk::Format,
        aspect_mask: vk::ImageAspectFlags,
    ) -> Result<vk::ImageView, vk::Result> {
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

        unsafe { device.create_image_view(&create_info, None) }
    }

    /// Determine the aspect mask based on format.
    fn get_aspect_mask(format: vk::Format) -> vk::ImageAspectFlags {
        if format == vk::Format::D16_UNORM
            || format == vk::Format::D32_SFLOAT
            || format == vk::Format::X8_D24_UNORM_PACK32
        {
            vk::ImageAspectFlags::DEPTH
        } else if format == vk::Format::D16_UNORM_S8_UINT
            || format == vk::Format::D24_UNORM_S8_UINT
            || format == vk::Format::D32_SFLOAT_S8_UINT
        {
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        } else {
            vk::ImageAspectFlags::COLOR
        }
    }
}

impl Default for AttachmentResources {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AttachmentResources {
    fn drop(&mut self) {
        if !self.attachments.is_empty() {
            log::warn!(
                "AttachmentResources dropped without explicit destroy - {} attachments leaked",
                self.attachments.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attachment_resources_new() {
        let resources = AttachmentResources::new();
        assert!(resources.is_empty());
        assert_eq!(resources.len(), 0);
    }

    #[test]
    fn test_attachment_resources_default() {
        let resources = AttachmentResources::default();
        assert!(resources.is_empty());
    }

    #[test]
    fn test_attachment_resources_contains() {
        let resources = AttachmentResources::new();
        assert!(!resources.contains("nonexistent"));
    }

    #[test]
    fn test_attachment_resources_get_nonexistent() {
        let resources = AttachmentResources::new();
        assert!(resources.get_view("nonexistent").is_none());
        assert!(resources.get_image("nonexistent").is_none());
        assert!(resources.get_format("nonexistent").is_none());
        assert!(resources.get_extent("nonexistent").is_none());
    }

    #[test]
    fn test_get_aspect_mask_color() {
        assert_eq!(
            AttachmentResources::get_aspect_mask(vk::Format::R8G8B8A8_SRGB),
            vk::ImageAspectFlags::COLOR
        );
        assert_eq!(
            AttachmentResources::get_aspect_mask(vk::Format::B8G8R8A8_SRGB),
            vk::ImageAspectFlags::COLOR
        );
    }

    #[test]
    fn test_get_aspect_mask_depth() {
        assert_eq!(
            AttachmentResources::get_aspect_mask(vk::Format::D32_SFLOAT),
            vk::ImageAspectFlags::DEPTH
        );
        assert_eq!(
            AttachmentResources::get_aspect_mask(vk::Format::D16_UNORM),
            vk::ImageAspectFlags::DEPTH
        );
    }

    #[test]
    fn test_get_aspect_mask_depth_stencil() {
        assert_eq!(
            AttachmentResources::get_aspect_mask(vk::Format::D24_UNORM_S8_UINT),
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        );
        assert_eq!(
            AttachmentResources::get_aspect_mask(vk::Format::D32_SFLOAT_S8_UINT),
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        );
    }
}
