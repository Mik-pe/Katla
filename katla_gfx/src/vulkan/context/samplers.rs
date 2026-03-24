use ash::vk;

use super::*;
use crate::sync::VkSampler;

impl VulkanContext {
    /// Create a CLAMP_TO_EDGE sampler suitable for UI/2D rendering.
    ///
    /// Uses LINEAR filtering with no anisotropy.
    #[allow(dead_code)]
    pub(crate) fn create_sampler_clamp_to_edge(&self) -> Result<VkSampler, vk::Result> {
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .anisotropy_enable(false)
            .max_anisotropy(1.0)
            .border_color(vk::BorderColor::INT_TRANSPARENT_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(0.0);

        let sampler = unsafe { self.device.create_sampler(&create_info, None)? };
        Ok(VkSampler::new(sampler))
    }

    /// Create a REPEAT sampler with anisotropy for 3D textures.
    ///
    /// Uses LINEAR filtering with 16x anisotropy and mipmaps.
    pub(crate) fn create_sampler_repeat_anisotropic(&self) -> Result<VkSampler, vk::Result> {
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(true)
            .max_anisotropy(16.0)
            .border_color(vk::BorderColor::INT_OPAQUE_WHITE)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::NEVER)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(vk::LOD_CLAMP_NONE);

        let sampler = unsafe { self.device.create_sampler(&create_info, None)? };
        Ok(VkSampler::new(sampler))
    }
}
