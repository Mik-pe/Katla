//! Vulkan backend for the render graph.
//!
//! Implements `RenderGraphBackend` for `VulkanRenderer`, providing
//! concrete transient texture creation, bindless management, and
//! frame indexing using Vulkan GPU resources.

use super::backend::RenderGraphBackend;
use super::error::RenderGraphError;
use super::transient_texture::TransientTexture;
use crate::renderer::VulkanRenderer;
use ash::vk;

impl RenderGraphBackend for VulkanRenderer {
    type TransientTexture = TransientTexture;
    type ImageView = crate::sync::VkImageView;

    fn create_transient_texture(
        &self,
        desc: &super::resource::GraphResourceDesc,
    ) -> Result<Self::TransientTexture, RenderGraphError> {
        let vk_format: vk::Format = desc.format.into();

        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width: desc.width,
                height: desc.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .format(vk_format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(match desc.resource_type {
                super::resource::GraphResourceType::ColorAttachment { .. } => {
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::INPUT_ATTACHMENT
                        // Editor targets may be read back (GPU picking, PNG
                        // capture), matching the swapchain image usage.
                        | vk::ImageUsageFlags::TRANSFER_SRC
                }
                super::resource::GraphResourceType::DepthAttachment { sampled, .. } => {
                    let mut usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
                    if sampled {
                        usage |= vk::ImageUsageFlags::SAMPLED;
                    }
                    usage
                }
                super::resource::GraphResourceType::SampledImage => {
                    vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST
                }
            })
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (image, allocation) = self
            .context
            .create_image(image_info, gpu_allocator::MemoryLocation::GpuOnly)
            .map_err(|_e| RenderGraphError::AllocationFailed(0))?;

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk_format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: if matches!(
                    desc.resource_type,
                    super::resource::GraphResourceType::DepthAttachment { .. }
                ) {
                    vk::ImageAspectFlags::DEPTH
                } else {
                    vk::ImageAspectFlags::COLOR
                },
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_view = unsafe {
            self.context
                .device
                .create_image_view(&view_info, None)
                .map_err(|e| {
                    RenderGraphError::BackendError(format!("Failed to create image view: {}", e))
                })?
        };

        Ok(TransientTexture::new(
            self.context.clone(),
            image,
            Some(allocation),
            crate::sync::VkImageView::new(image_view),
            vk_format,
            vk::Extent2D {
                width: desc.width,
                height: desc.height,
            },
        ))
    }

    fn destroy_transient_texture(texture: Self::TransientTexture) {
        drop(texture);
    }

    fn current_frame(&self) -> usize {
        VulkanRenderer::current_frame(self)
    }

    fn transient_texture_frames() -> usize {
        2
    }

    fn register_bindless_texture(
        &mut self,
        texture: &Self::TransientTexture,
    ) -> Result<u32, RenderGraphError> {
        self.register_bindless_texture(texture.image_view.vk())
            .map_err(|e| RenderGraphError::BackendError(e.to_string()))
    }

    fn update_bindless_texture(
        &mut self,
        slot: u32,
        texture: &Self::TransientTexture,
    ) -> Result<(), RenderGraphError> {
        self.update_bindless_texture(slot, texture.image_view.vk())
            .map_err(|e| RenderGraphError::BackendError(e.to_string()))
    }

    fn transient_texture_format(texture: &Self::TransientTexture) -> crate::texture::ImageFormat {
        texture
            .format
            .try_into()
            .unwrap_or(crate::texture::ImageFormat::R8G8B8A8Srgb)
    }

    fn transient_texture_extent(texture: &Self::TransientTexture) -> (u32, u32) {
        (texture.extent.width, texture.extent.height)
    }

    fn transient_texture_is_depth(texture: &Self::TransientTexture) -> bool {
        texture.format == vk::Format::D32_SFLOAT
    }

    fn transient_texture_bindless_slot(texture: &Self::TransientTexture) -> Option<u32> {
        texture.bindless_slot
    }

    fn set_transient_texture_bindless_slot(texture: &mut Self::TransientTexture, slot: u32) {
        texture.bindless_slot = Some(slot);
    }

    fn transient_texture_view(texture: &Self::TransientTexture) -> Self::ImageView {
        texture.image_view
    }

    fn swapchain_image_view(&self, image_index: u32) -> Self::ImageView {
        self.frame_context.swapchain_image_views[image_index as usize]
    }

    fn depth_image_view(&self, frame_index: usize) -> Option<Self::ImageView> {
        self.frame_context
            .depth_render_textures
            .get(frame_index)
            .map(|dt| dt.image_view)
    }
}
