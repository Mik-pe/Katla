//! Bindless texture manager for efficient texture binding.
//!
//! This module provides a bindless texture system that eliminates per-material
//! descriptor set bindings by using a single global texture array.
//!
//! # Benefits
//! - **Single descriptor set** for all textures, bound once per frame
//! - **O(1) texture registration** with slot allocation
//! - **Shared sampler** reduces descriptor count
//! - **Scales to 4096 textures** without per-material descriptor updates
//! - **Default textures** at reserved slots for fallback
//!
//! # Architecture
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ BindlessTextureManager                                      │
//! ├─ Descriptor Set Layout (set 1)                             │
//! │  ├─ Binding 0: texture_2d array (4096 slots)               │
//! │  └─ Binding 1: shared sampler                              │
//! ├─ Default Textures (reserved slots 0-4)                     │
//! │  ├─ slots[0]: White (default albedo)                       │
//! │  ├─ slots[1]: Flat normal                                  │
//! │  ├─ slots[2]: Default MR (non-metal, medium roughness)     │
//! │  ├─ slots[3]: White (no occlusion)                         │
//! │  └─ slots[4]: Black (no emission)                          │
//! ├─ User Textures (slots 5+)                                  │
//! │  ├─ slots[5]: Texture A                                    │
//! │  ├─ slots[6]: Texture B                                    │
//! │  └─ ...                                                     │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//! ```ignore
//! let mut manager = BindlessTextureManager::new(context.clone())?;
//!
//! // Register textures (returns slot index)
//! let albedo_idx = manager.register_texture(albedo_view);
//! let normal_idx = manager.register_texture(normal_view);
//!
//! // Bind once per frame
//! cmd.bind_descriptor_sets(
//!     pipeline_layout,
//!     1, // set 1
//!     &[manager.descriptor_set()],
//! );
//!
//! // In shader: textureIndices in ObjectUniforms tells which textures to use
//! // let albedo = textureSample(bindless_textures[indices.albedo], shared_sampler, uv);
//! ```

use ash::vk;
use std::rc::Rc;

use crate::RendererError;
use crate::sync::{VkDescriptorSet, VkDescriptorSetLayout, VkImageView, VkSampler};
use crate::vulkan::context::VulkanContext;
use crate::vulkan::texture::Texture;

/// Maximum number of textures in the bindless array.
pub const MAX_BINDLESS_TEXTURES: u32 = 4096;

/// Number of reserved slots for default textures.
pub(crate) const DEFAULT_TEXTURE_COUNT: u32 = 5;

/// Bindless texture manager.
///
/// Manages a single descriptor set with a texture array and shared sampler.
/// Textures are registered and assigned slot indices, which are passed to
/// shaders via per-object uniforms.
pub struct BindlessTextureManager {
    /// Descriptor pool for the bindless set.
    descriptor_pool: vk::DescriptorPool,
    /// Descriptor set layout for bindless textures.
    descriptor_layout: VkDescriptorSetLayout,
    /// Descriptor set containing the texture array and sampler.
    descriptor_set: VkDescriptorSet,
    /// Shared sampler for all textures.
    shared_sampler: VkSampler,
    /// Texture slots (Some = occupied, None = free).
    slots: Vec<Option<vk::ImageView>>,
    /// Stack of free slot indices for O(1) allocation.
    free_slots: Vec<u32>,
    /// Device handle for cleanup.
    device: ash::Device,
    /// Default textures (kept alive for their resources).
    _default_textures: Vec<Texture>,
}

impl BindlessTextureManager {
    /// Create a new bindless texture manager.
    ///
    /// Creates:
    /// - A descriptor pool with enough space for MAX_BINDLESS_TEXTURES textures
    /// - A descriptor set layout with texture array + sampler bindings
    /// - A descriptor set with the texture array bound
    /// - A shared sampler with reasonable defaults
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    ///
    /// # Returns
    /// A new BindlessTextureManager, or an error if creation fails
    pub fn new(context: Rc<VulkanContext>) -> Result<Self, RendererError> {
        // Create shared sampler with reasonable defaults
        let shared_sampler = context.create_sampler_repeat_anisotropic()?;

        // Create descriptor set layout
        // Binding 0: texture_2d array (SAMPLED_IMAGE, count = MAX_BINDLESS_TEXTURES)
        // Binding 1: shared sampler (SAMPLER, count = 1)
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(MAX_BINDLESS_TEXTURES)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];

        // Enable update_after_bind for dynamic texture registration
        let binding_flags = [
            vk::DescriptorBindingFlags::PARTIALLY_BOUND
                | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
            vk::DescriptorBindingFlags::empty(),
        ];

        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut binding_flags_info);

        let descriptor_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&layout_info, None)?
        };

        // Create descriptor pool with UPDATE_AFTER_BIND flag
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(MAX_BINDLESS_TEXTURES),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(1),
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1)
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND);

        let descriptor_pool = unsafe { context.device.create_descriptor_pool(&pool_info, None)? };

        // Allocate descriptor set
        let layouts = [descriptor_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe { context.device.allocate_descriptor_sets(&alloc_info)? };
        let descriptor_set = descriptor_sets[0];

        // Write the shared sampler to binding 1 (static, done once)
        let sampler_info = [vk::DescriptorImageInfo::default().sampler(shared_sampler.vk())];

        let sampler_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .image_info(&sampler_info);

        unsafe {
            context.device.update_descriptor_sets(&[sampler_write], &[]);
        }

        // Create default textures (1x1 pixels for each type)
        let (default_image_views, default_textures) =
            Self::create_default_textures(&context, descriptor_set);

        // Initialize slots with default textures at reserved positions
        let mut slots = vec![None; MAX_BINDLESS_TEXTURES as usize];
        for (i, view) in default_image_views.iter().enumerate() {
            slots[i] = Some(view.vk());
        }

        // Initialize free slots stack (skip reserved slots 0-4)
        let free_slots: Vec<u32> = (DEFAULT_TEXTURE_COUNT..MAX_BINDLESS_TEXTURES)
            .rev()
            .collect();

        Ok(Self {
            descriptor_pool,
            descriptor_layout: VkDescriptorSetLayout::new(descriptor_layout),
            descriptor_set: VkDescriptorSet::new(descriptor_set),
            shared_sampler,
            slots,
            free_slots,
            device: context.device.clone(),
            _default_textures: default_textures,
        })
    }

    /// Create default textures (white, normal, MR, AO, emission).
    fn create_default_textures(
        context: &Rc<VulkanContext>,
        descriptor_set: vk::DescriptorSet,
    ) -> (Vec<VkImageView>, Vec<Texture>) {
        let mut views = Vec::with_capacity(DEFAULT_TEXTURE_COUNT as usize);
        let mut textures = Vec::with_capacity(DEFAULT_TEXTURE_COUNT as usize);

        // Default pixel data for each texture type
        let default_pixels: [[u8; 4]; DEFAULT_TEXTURE_COUNT as usize] = [
            [255, 255, 255, 255], // Slot 0: White (default albedo)
            [128, 128, 255, 255], // Slot 1: Flat normal (+Z direction)
            [255, 128, 0, 255],   // Slot 2: Default MR (G=roughness=0.5, B=metallic=0)
            [255, 255, 255, 255], // Slot 3: White (no occlusion)
            [0, 0, 0, 255],       // Slot 4: Black (no emission)
        ];

        for (slot_idx, pixels) in default_pixels.iter().enumerate() {
            let texture = Texture::create_image(
                context.clone(),
                1,
                1,
                crate::ImageFormat::R8G8B8A8Srgb,
                pixels,
            );

            // Update descriptor set for this slot
            let image_info = [vk::DescriptorImageInfo::default()
                .image_view(texture.image_view.vk())
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];

            let write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .dst_array_element(slot_idx as u32)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&image_info);

            unsafe {
                context.device.update_descriptor_sets(&[write], &[]);
            }

            views.push(texture.image_view);
            textures.push(texture);
        }

        (views, textures)
    }

    /// Register a texture with the bindless system.
    ///
    /// Allocates a slot and updates the descriptor set with the texture's image view.
    ///
    /// # Arguments
    /// * `image_view` - The Vulkan image view to register
    ///
    /// # Returns
    /// The slot index for this texture, or an error if no slots are available.
    pub fn register_texture(&mut self, image_view: vk::ImageView) -> Result<u32, RendererError> {
        // Allocate a slot
        let slot = self.free_slots.pop().ok_or_else(|| {
            RendererError::InvalidOperation("No free bindless texture slots available".into())
        })?;

        // Update the slot
        self.slots[slot as usize] = Some(image_view);

        // Update descriptor set for this slot
        let image_info = [vk::DescriptorImageInfo::default()
            .image_view(image_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];

        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_set.vk())
            .dst_binding(0)
            .dst_array_element(slot)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(&image_info);

        unsafe {
            self.device.update_descriptor_sets(&[write], &[]);
        }

        Ok(slot)
    }

    /// Get the descriptor set for binding to shaders.
    ///
    /// This should be bound to set 1 after binding the pipeline.
    pub fn descriptor_set(&self) -> VkDescriptorSet {
        self.descriptor_set
    }

}

impl Drop for BindlessTextureManager {
    fn drop(&mut self) {
        unsafe {
            // Default textures are dropped automatically via their Drop impl

            // Destroy descriptor infrastructure
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_layout.vk(), None);
            self.device.destroy_sampler(self.shared_sampler.vk(), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_bindless_textures() {
        // Ensure we have a reasonable limit
        assert!(MAX_BINDLESS_TEXTURES >= 1024);
        assert!(MAX_BINDLESS_TEXTURES <= 16384);
    }
}
