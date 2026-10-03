//! Vulkan translation of the portable sampler policy.

use ash::vk;

use super::VulkanContext;
use crate::error::RendererError;
use crate::sync::VkSampler;
use crate::{AddressMode, CompareOp, FilterMode, MipFilter, SamplerDescriptor};

impl VulkanContext {
    pub(crate) fn create_sampler(
        &self,
        descriptor: SamplerDescriptor,
    ) -> Result<VkSampler, RendererError> {
        descriptor
            .validate()
            .map_err(|reason| RendererError::InvalidDescriptor {
                resource: "sampler".into(),
                reason: reason.into(),
            })?;
        let filter = |mode| match mode {
            FilterMode::Nearest => vk::Filter::NEAREST,
            FilterMode::Linear => vk::Filter::LINEAR,
        };
        let address = |mode| match mode {
            AddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
            AddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
            AddressMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        };
        let max_anisotropy = unsafe {
            self.instance
                .get_physical_device_properties(self.physical_device)
                .limits
                .max_sampler_anisotropy
        };
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(filter(descriptor.mag_filter))
            .min_filter(filter(descriptor.min_filter))
            .address_mode_u(address(descriptor.address_u))
            .address_mode_v(address(descriptor.address_v))
            .address_mode_w(address(descriptor.address_w))
            .anisotropy_enable(descriptor.anisotropy > 1)
            .max_anisotropy(f32::from(descriptor.anisotropy).min(max_anisotropy))
            .compare_enable(descriptor.comparison.is_some())
            .compare_op(descriptor.comparison.unwrap_or(CompareOp::Always).into())
            .mipmap_mode(match descriptor.mip_filter {
                MipFilter::None | MipFilter::Nearest => vk::SamplerMipmapMode::NEAREST,
                MipFilter::Linear => vk::SamplerMipmapMode::LINEAR,
            })
            .max_lod(if descriptor.mip_filter == MipFilter::None {
                0.0
            } else {
                vk::LOD_CLAMP_NONE
            });
        let sampler = unsafe { self.device.create_sampler(&create_info, None)? };
        Ok(VkSampler::new(sampler))
    }
}
