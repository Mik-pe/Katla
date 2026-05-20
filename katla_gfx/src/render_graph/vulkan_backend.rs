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
        texture.image_view.clone()
    }

    fn swapchain_image_view(&self, image_index: u32) -> Self::ImageView {
        self.frame_context.swapchain_image_views[image_index as usize].clone()
    }

    fn depth_image_view(&self, frame_index: usize) -> Option<Self::ImageView> {
        self.frame_context
            .depth_render_textures
            .get(frame_index)
            .map(|dt| dt.image_view.clone())
    }

    fn transition_texture(
        _texture: &mut Self::TransientTexture,
        _from: super::resource::ResourceState,
        _to: super::resource::ResourceState,
    ) {
    }

    fn transition_backbuffer(&self, _image_index: u32, _to: super::resource::ResourceState) {}

    fn depth_render_pass_sync(&self, _frame_index: usize) {}
}

/// Vulkan-specific extension methods for `ResourceState`.
///
/// Converts backend-agnostic resource states to Vulkan pipeline stage
/// and access flags. These were previously on `ResourceState` directly;
/// now they're on the Vulkan backend to keep the core types backend-agnostic.
impl crate::render_pass::ResourceState {
    /// Convert to Vulkan pipeline stage flags.
    pub fn to_vk_stage_flags(self) -> vk::PipelineStageFlags {
        match self {
            Self::Undefined => vk::PipelineStageFlags::TOP_OF_PIPE,
            Self::ColorAttachment => vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            Self::DepthStencilAttachment => {
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
            }
            Self::ShaderRead | Self::ShaderWrite => {
                vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER
            }
            Self::TransferSrc | Self::TransferDst => vk::PipelineStageFlags::TRANSFER,
            Self::PresentSrc => vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        }
    }

    /// Convert to Vulkan access flags.
    pub fn to_vk_access_flags(self) -> vk::AccessFlags {
        match self {
            Self::Undefined => vk::AccessFlags::empty(),
            Self::ColorAttachment => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            Self::DepthStencilAttachment => {
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
            }
            Self::ShaderRead => vk::AccessFlags::SHADER_READ,
            Self::ShaderWrite => vk::AccessFlags::SHADER_WRITE,
            Self::TransferSrc => vk::AccessFlags::TRANSFER_READ,
            Self::TransferDst => vk::AccessFlags::TRANSFER_WRITE,
            Self::PresentSrc => vk::AccessFlags::NONE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_state_vk_conversions() {
        use crate::render_pass::ResourceState;

        assert_eq!(
            ResourceState::Undefined.to_vk_stage_flags(),
            vk::PipelineStageFlags::TOP_OF_PIPE
        );
        assert_eq!(
            ResourceState::ColorAttachment.to_vk_stage_flags(),
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
        );
        assert_eq!(
            ResourceState::ShaderRead.to_vk_access_flags(),
            vk::AccessFlags::SHADER_READ
        );
        assert_eq!(
            ResourceState::TransferDst.to_vk_stage_flags(),
            vk::PipelineStageFlags::TRANSFER
        );
    }
}
