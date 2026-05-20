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

    fn create_transient_texture(
        _desc: &super::resource::GraphResourceDesc,
    ) -> Result<Self::TransientTexture, RenderGraphError> {
        Err(RenderGraphError::InvalidConfiguration(
            "Vulkan transient textures must be created through FrameGraph::initialize_transient_textures()".to_string(),
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
            .map_err(|e| RenderGraphError::VulkanError(e.to_string()))
    }

    fn update_bindless_texture(
        &mut self,
        slot: u32,
        texture: &Self::TransientTexture,
    ) -> Result<(), RenderGraphError> {
        self.update_bindless_texture(slot, texture.image_view.vk())
            .map_err(|e| RenderGraphError::VulkanError(e.to_string()))
    }
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
