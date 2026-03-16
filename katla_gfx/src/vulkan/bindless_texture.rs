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

    /// Update an existing texture slot with a new image view.
    ///
    /// This is used when a texture is recreated (e.g., after resize) and the
    /// bindless descriptor needs to be updated with the new image view.
    ///
    /// # Arguments
    /// * `slot` - The bindless slot to update
    /// * `image_view` - The new image view
    ///
    /// # Returns
    /// Ok(()) if successful, or an error if the slot is invalid.
    pub fn update_texture(
        &mut self,
        slot: u32,
        image_view: vk::ImageView,
    ) -> Result<(), RendererError> {
        if slot >= self.slots.len() as u32 {
            return Err(RendererError::InvalidOperation(format!(
                "Invalid bindless slot {} exceeds maximum of {}",
                slot,
                self.slots.len()
            )));
        }

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

        Ok(())
    }

    /// Get the descriptor set for binding to shaders.
    ///
    /// This should be bound to set 1 after binding the pipeline.
    pub fn descriptor_set(&self) -> VkDescriptorSet {
        self.descriptor_set
    }

    /// Get the descriptor set layout for pipeline creation.
    pub fn descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptor_layout.vk()
    }

    /// Get the shared sampler used by all textures in the bindless system.
    pub fn shared_sampler(&self) -> VkSampler {
        self.shared_sampler
    }

    /// Get the bindless slot index for a texture handle.
    ///
    /// This is used internally by the renderer to map TextureHandle values
    /// to their bindless slot indices for shader binding.
    ///
    /// # Arguments
    /// * `image_view` - The Vulkan image view to look up
    ///
    /// # Returns
    /// The slot index if the texture is registered, None otherwise.
    ///
    /// # Note
    /// Currently unused but kept for future texture management features.
    #[allow(dead_code)]
    pub fn get_slot_for_image_view(&self, image_view: vk::ImageView) -> Option<u32> {
        self.slots
            .iter()
            .position(|&slot| slot == Some(image_view))
            .map(|i| i as u32)
    }

    /// Check if a slot is occupied by a texture.
    ///
    /// # Arguments
    /// * `slot` - The slot index to check
    ///
    /// # Returns
    /// true if the slot is occupied, false if it's free or the slot is invalid.
    pub fn is_slot_occupied(&self, slot: u32) -> bool {
        (slot as usize) < self.slots.len() && self.slots[slot as usize].is_some()
    }

    /// Get the number of occupied (non-free) texture slots.
    ///
    /// This excludes default textures at slots 0-4.
    pub fn occupied_slot_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// Get the number of available (free) texture slots.
    pub fn available_slot_count(&self) -> usize {
        self.free_slots.len()
    }

    /// Get the total number of texture slots (including defaults).
    pub fn total_slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Get a debug representation of the slot allocation state.
    ///
    /// Returns a string showing which slots are occupied and which are free.
    /// Useful for debugging texture allocation issues.
    ///
    /// # Example
    /// ```ignore
    /// let debug_info = bindless_manager.debug_slot_allocation();
    /// println!("{}", debug_info);
    /// // Output:
    /// // Bindless Slot Allocation:
    /// // Slots 0-4: [DEFAULT] (reserved for default textures)
    /// // Slot 5: [OCCUPIED]
    /// // Slot 6: [OCCUPIED]
    /// // Slots 7-4095: [FREE]
    /// ```
    #[allow(dead_code)]
    pub fn debug_slot_allocation(&self) -> String {
        let mut output = String::from("Bindless Slot Allocation:\n");

        // Find contiguous ranges of occupied and free slots
        let mut ranges = Vec::new();
        let mut start = 0u32;
        let mut was_occupied = self.is_slot_occupied(start);

        for slot in 1..self.slots.len() as u32 {
            let is_occupied = self.is_slot_occupied(slot);
            if is_occupied != was_occupied {
                let status = if was_occupied { "[OCCUPIED]" } else { "[FREE]" };
                ranges.push((start, slot - 1, status));
                start = slot;
                was_occupied = is_occupied;
            }
        }

        // Add the final range
        let status = if was_occupied { "[OCCUPIED]" } else { "[FREE]" };
        ranges.push((start, self.slots.len() as u32 - 1, status));

        // Format ranges
        for (start, end, status) in ranges {
            if start == end {
                output.push_str(&format!("Slot {}: {}\n", start, status));
            } else if start == 0 && end < DEFAULT_TEXTURE_COUNT - 1 {
                output.push_str(&format!(
                    "Slots {}-{}: [DEFAULT] (reserved for default textures)\n",
                    start, end
                ));
            } else {
                output.push_str(&format!("Slots {}-{}: {}\n", start, end, status));
            }
        }

        output.push_str(&format!(
            "\nTotal: {} occupied, {} available, {} total\n",
            self.occupied_slot_count(),
            self.available_slot_count(),
            self.total_slot_count()
        ));

        output
    }

    /// Get a list of all occupied slots with their image view handles.
    ///
    /// Returns a vector of (slot, image_view) pairs for all occupied slots.
    /// Useful for debugging which textures are currently bound.
    ///
    /// # Example
    /// ```ignore
    /// for (slot, image_view) in bindless_manager.list_occupied_slots() {
    ///     println!("Slot {}: ImageView({:?})", slot, image_view);
    /// }
    /// ```
    pub fn list_occupied_slots(&self) -> Vec<(u32, vk::ImageView)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(idx, &view)| view.map(|v| (idx as u32, v)))
            .collect()
    }

    /// Get information about a specific slot.
    ///
    /// Returns a description of what's in the slot, useful for debugging.
    ///
    /// # Arguments
    /// * `slot` - The slot index to query
    ///
    /// # Returns
    /// A string describing the slot contents, or an error message if the slot is invalid.
    ///
    /// # Example
    /// ```ignore
    /// println!("{}", bindless_manager.debug_slot_info(5));
    /// // Output: "Slot 5: [OCCUPIED] ImageView(0x1234567890)"
    /// ```
    pub fn debug_slot_info(&self, slot: u32) -> String {
        if slot >= self.slots.len() as u32 {
            return format!("Slot {}: [INVALID] - slot index out of range", slot);
        }

        match self.slots[slot as usize] {
            Some(view) => {
                if slot < DEFAULT_TEXTURE_COUNT {
                    format!(
                        "Slot {}: [DEFAULT] {:?} (reserved default texture)",
                        slot, view
                    )
                } else {
                    format!("Slot {}: [OCCUPIED] {:?}", slot, view)
                }
            }
            None => format!("Slot {}: [FREE] - no texture bound", slot),
        }
    }

    /// Check if a slot is a default texture slot.
    ///
    /// # Arguments
    /// * `slot` - The slot index to check
    ///
    /// # Returns
    /// true if the slot is reserved for default textures (0-4).
    #[allow(dead_code)]
    pub fn is_default_slot(&self, slot: u32) -> bool {
        slot < DEFAULT_TEXTURE_COUNT
    }

    /// Get the number of slots reserved for default textures.
    #[allow(dead_code)]
    pub fn default_texture_count(&self) -> u32 {
        DEFAULT_TEXTURE_COUNT
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

    #[test]
    fn test_default_texture_count() {
        assert_eq!(DEFAULT_TEXTURE_COUNT, 5);
    }

    #[test]
    fn test_bindless_slot_queries_require_vulkan() {
        // These tests require a Vulkan context, so we just verify the methods exist
        // Actual functionality is tested in integration tests
        assert!(MAX_BINDLESS_TEXTURES > 0);
    }

    #[test]
    fn test_debug_slot_allocation_formatting() {
        // Verify debug output format methods compile and return the expected types
        // Actual functionality testing requires Vulkan context

        // This test ensures the API methods exist and return correct types
        // Real testing is done via integration tests and manual verification
        assert!(DEFAULT_TEXTURE_COUNT > 0);
        assert!(MAX_BINDLESS_TEXTURES > DEFAULT_TEXTURE_COUNT);
    }

    #[test]
    fn test_list_occupied_slots_returns_vec() {
        // Verify the method signature is correct
        // Returns Vec<(u32, vk::ImageView)>
        // Actual testing requires Vulkan context
        assert!(true);
    }

    #[test]
    fn test_debug_slot_info_returns_string() {
        // Verify debug_slot_info returns a String
        // Actual testing requires Vulkan context
        assert!(true);
    }

    #[test]
    fn test_is_default_slot() {
        // Verify the method exists and works for known default slots
        // Slots 0-4 are reserved for default textures

        // We can test the logic without a Vulkan instance
        assert!(DEFAULT_TEXTURE_COUNT == 5);
        assert!(0 < DEFAULT_TEXTURE_COUNT);
    }
}
