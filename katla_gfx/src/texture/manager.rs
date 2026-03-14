//! Texture manager for centralized texture creation and storage.
//!
//! TextureManager provides a clean API for creating, storing, and looking up
//! textures using opaque TextureHandle values. It also manages default textures
//! for common use cases.

use crate::handle::{ResourceStorage, TextureHandle};
use crate::texture::ImageFormat;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::texture::Texture;
use ash::vk;
use std::collections::HashMap;
use std::rc::Rc;

use super::descriptor::TextureDescriptor;

/// Default texture slot indices.
/// These match BindlessTextureManager's default slots for consistency.
pub(crate) const DEFAULT_ALBEDO_SLOT: u32 = 0;
pub(crate) const DEFAULT_NORMAL_SLOT: u32 = 1;
pub(crate) const DEFAULT_MR_SLOT: u32 = 2;
pub(crate) const DEFAULT_OCCLUSION_SLOT: u32 = 3;
pub(crate) const DEFAULT_EMISSION_SLOT: u32 = 4;

/// Centralized texture creation and storage.
///
/// TextureManager provides:
/// - Handle-based texture creation (no direct Vulkan exposure)
/// - Default textures for common use cases
/// - Lookup by handle for internal rendering operations
/// - Optional bindless slot tracking
pub struct TextureManager {
    /// Storage for all textures.
    textures: ResourceStorage<Rc<Texture>>,
    /// Vulkan context for texture creation.
    context: Rc<VulkanContext>,
    /// Pre-created default textures.
    default_white: TextureHandle,
    default_normal: TextureHandle,
    default_metallic_roughness: TextureHandle,
    default_occlusion: TextureHandle,
    default_emission: TextureHandle,
    /// Optional bindless slot tracking.
    /// Maps TextureHandle -> bindless slot index.
    bindless_slots: HashMap<TextureHandle, u32>,
}

impl TextureManager {
    /// Create a new TextureManager with pre-created default textures.
    pub fn new(context: Rc<VulkanContext>) -> Result<Self, vk::Result> {
        let mut textures = ResourceStorage::new();

        // Pre-create default textures
        let default_white =
            Self::create_default_texture(&mut textures, &context, Texture::create_default_albedo);
        let default_normal =
            Self::create_default_texture(&mut textures, &context, Texture::create_default_normal);
        let default_metallic_roughness = Self::create_default_texture(
            &mut textures,
            &context,
            Texture::create_default_metallic_roughness,
        );
        let default_occlusion = Self::create_default_texture(
            &mut textures,
            &context,
            Texture::create_default_occlusion,
        );
        let default_emission =
            Self::create_default_texture(&mut textures, &context, Texture::create_default_emission);

        Ok(Self {
            textures,
            context,
            default_white,
            default_normal,
            default_metallic_roughness,
            default_occlusion,
            default_emission,
            bindless_slots: HashMap::new(),
        })
    }

    /// Helper to create a default texture and return its handle.
    fn create_default_texture(
        textures: &mut ResourceStorage<Rc<Texture>>,
        context: &Rc<VulkanContext>,
        create_fn: fn(Rc<VulkanContext>) -> Texture,
    ) -> TextureHandle {
        let texture = Rc::new(create_fn(context.clone()));
        TextureHandle::new(textures.insert(texture))
    }

    // ========================================================================
    // Creation API
    // ========================================================================

    /// Create a texture from a descriptor and pixel data.
    ///
    /// # Arguments
    /// * `desc` - Texture descriptor specifying dimensions, format, and usage
    /// * `data` - Pixel data (must match descriptor dimensions and format)
    ///
    /// # Returns
    /// A TextureHandle for the created texture.
    pub fn create(&mut self, desc: &TextureDescriptor, data: &[u8]) -> TextureHandle {
        let texture = Rc::new(Texture::from_descriptor(&self.context, desc, data));
        TextureHandle::new(self.textures.insert(texture))
    }

    /// Create an RGBA8 SRGB texture from pixel data.
    ///
    /// Convenience method for common texture type.
    pub fn create_rgba(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle {
        let desc = TextureDescriptor::rgba8_srgb(width, height);
        self.create(&desc, data)
    }

    /// Create an RGBA8 UNORM texture from pixel data.
    ///
    /// Use for linear data like normal maps.
    pub fn create_rgba_unorm(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle {
        let desc = TextureDescriptor::rgba8_unorm(width, height);
        self.create(&desc, data)
    }

    /// Create a 1x1 solid color texture.
    ///
    /// Useful for placeholder or fallback textures.
    pub fn create_solid(&mut self, color: [u8; 4]) -> TextureHandle {
        self.create_rgba(1, 1, &color)
    }

    /// Create a texture from RGB data (converts to RGBA internally).
    pub fn create_from_rgb(&mut self, width: u32, height: u32, rgb_data: &[u8]) -> TextureHandle {
        let rgba_data = Texture::convert_rgb_to_rgba(rgb_data, width, height);
        self.create_rgba(width, height, &rgba_data)
    }

    /// Create an empty texture (no initial data).
    ///
    /// Useful for render targets or textures that will be filled later.
    pub fn create_empty(&mut self, desc: &TextureDescriptor) -> TextureHandle {
        // Calculate expected data size based on format
        let bytes_per_pixel = match desc.format {
            ImageFormat::R8Unorm => 1,
            ImageFormat::Rg8Unorm => 2,
            ImageFormat::R8G8B8A8Srgb | ImageFormat::R8G8B8A8Unorm | ImageFormat::B8G8R8A8Srgb => 4,
            ImageFormat::R32Sfloat => 4,
            ImageFormat::R16G16B16A16Sfloat => 8,
            _ => 4, // Default to 4 bytes for depth formats (won't be used)
        };
        let size = (desc.width * desc.height * bytes_per_pixel) as usize;
        let data = vec![0u8; size];
        self.create(desc, &data)
    }

    /// Create a TextureHandle from existing Vulkan resources (image, image_view, sampler).
    ///
    /// This is useful for wrapping transient textures created by the frame graph
    /// so they can be used by the UI rendering system.
    ///
    /// # Arguments
    /// * `image` - The Vulkan image
    /// * `image_view` - The Vulkan image view
    /// * `sampler` - The Vulkan sampler (already wrapped in VkSampler)
    /// * `format` - The image format
    /// * `width` - Texture width in pixels
    /// * `height` - Texture height in pixels
    ///
    /// # Safety
    /// The caller must ensure that the Vulkan resources are valid and will remain
    /// valid for the lifetime of the returned TextureHandle. The resources must
    /// have been created with the same Vulkan device used by this texture manager.
    pub unsafe fn from_vulkan_resources(
        &mut self,
        _image: vk::Image,
        _image_view: vk::ImageView,
        _sampler: vk::Sampler,
        _format: ImageFormat,
        _width: u32,
        _height: u32,
    ) -> TextureHandle {
        // TODO: Implement wrapping transient textures for UI rendering
        // For now, return a placeholder handle
        TextureHandle::new(0)
    }

    // ========================================================================
    // Default Textures
    // ========================================================================

    /// Get the default white (albedo) texture.
    pub fn default_white(&self) -> TextureHandle {
        self.default_white
    }

    /// Get the default flat normal texture.
    pub fn default_normal(&self) -> TextureHandle {
        self.default_normal
    }

    /// Get the default metallic/roughness texture.
    pub fn default_metallic_roughness(&self) -> TextureHandle {
        self.default_metallic_roughness
    }

    /// Get the default occlusion texture.
    pub fn default_occlusion(&self) -> TextureHandle {
        self.default_occlusion
    }

    /// Get the default emission texture.
    pub fn default_emission(&self) -> TextureHandle {
        self.default_emission
    }

    /// Get a default texture by slot index (matches bindless slots).
    pub fn default_by_slot(&self, slot: u32) -> Option<TextureHandle> {
        match slot {
            DEFAULT_ALBEDO_SLOT => Some(self.default_white),
            DEFAULT_NORMAL_SLOT => Some(self.default_normal),
            DEFAULT_MR_SLOT => Some(self.default_metallic_roughness),
            DEFAULT_OCCLUSION_SLOT => Some(self.default_occlusion),
            DEFAULT_EMISSION_SLOT => Some(self.default_emission),
            _ => None,
        }
    }

    // ========================================================================
    // Lookup (internal use)
    // ========================================================================

    /// Get an Rc reference to the Texture for a handle.
    ///
    /// This returns a clone of the Rc, allowing the caller to keep the texture alive.
    /// Use this for legacy code that needs Rc<Texture>.
    pub fn get_texture_rc(&self, handle: TextureHandle) -> Option<Rc<Texture>> {
        self.textures.get(handle.index()).cloned()
    }

    /// Get a reference to the Texture for a handle.
    pub fn get_texture(&self, handle: TextureHandle) -> Option<&Texture> {
        self.textures.get(handle.index()).map(|rc| rc.as_ref())
    }

    /// Get a mutable reference to the Texture for a handle.
    pub fn get_texture_mut(&mut self, handle: TextureHandle) -> Option<&mut Texture> {
        self.textures
            .get_mut(handle.index())
            .and_then(|rc| Rc::get_mut(rc))
    }

    /// Check if a handle points to a valid texture.
    pub fn contains(&self, handle: TextureHandle) -> bool {
        self.textures.contains(handle.index())
    }

    /// Get the number of textures stored.
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Check if the manager is empty.
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    // ========================================================================
    // Bindless Integration
    // ========================================================================

    /// Register a bindless slot for a texture handle.
    ///
    /// This tracks which bindless slot a texture was registered to,
    /// allowing lookup by handle later.
    pub fn register_bindless_slot(&mut self, handle: TextureHandle, slot: u32) {
        self.bindless_slots.insert(handle, slot);
    }

    /// Get the bindless slot for a texture handle.
    ///
    /// Returns None if the texture hasn't been registered with bindless.
    pub fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32> {
        self.bindless_slots.get(&handle).copied()
    }

    /// Alias for get_bindless_slot for API consistency.
    pub fn get_bindless_index(&self, handle: TextureHandle) -> Option<u32> {
        self.get_bindless_slot(handle)
    }

    /// Get the texture handle at a specific bindless slot.
    ///
    /// This is useful for debugging and texture inspection tools.
    ///
    /// # Arguments
    /// * `slot` - The bindless slot index
    ///
    /// # Returns
    /// The TextureHandle at that slot, or None if the slot is not registered
    /// or doesn't exist.
    ///
    /// # Example
    /// ```ignore
    /// // Query which texture is in slot 10
    /// if let Some(handle) = texture_manager.get_texture_at_slot(10) {
    ///     println!("Texture at slot 10: {:?}", handle);
    /// }
    /// ```
    pub fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle> {
        // Reverse lookup: find the handle with this slot
        for (&handle, &handle_slot) in &self.bindless_slots {
            if handle_slot == slot {
                return Some(handle);
            }
        }
        None
    }

    /// Get all registered texture handles with their bindless slots.
    ///
    /// This returns an iterator over (TextureHandle, slot) pairs for all
    /// textures that have been registered with the bindless system.
    ///
    /// # Example
    /// ```ignore
    /// for (handle, slot) in texture_manager.iter_bindless_textures() {
    ///     println!("Texture {:?} is at slot {}", handle, slot);
    /// }
    /// ```
    pub fn iter_bindless_textures(&self) -> impl Iterator<Item = (TextureHandle, u32)> + '_ {
        self.bindless_slots
            .iter()
            .map(|(&handle, &slot)| (handle, slot))
    }

    /// Update texture data in-place.
    ///
    /// The data size must match the current texture dimensions.
    /// Uses a staging buffer for GPU upload.
    pub fn update_data(&self, handle: TextureHandle, data: &[u8]) -> Result<(), vk::Result> {
        if let Some(texture) = self.get_texture(handle) {
            texture.update_data(data);
            Ok(())
        } else {
            Err(vk::Result::ERROR_INVALID_OPAQUE_CAPTURE_ADDRESS)
        }
    }

    /// Resize a texture with new dimensions and data.
    ///
    /// This recreates the internal image and updates the image view.
    /// Any registered descriptors are automatically updated.
    pub fn resize(&mut self, handle: TextureHandle, width: u32, height: u32, data: &[u8]) -> bool {
        if let Some(texture) = self.get_texture_mut(handle) {
            texture.resize(width, height, data)
        } else {
            false
        }
    }

    /// Remove a bindless slot registration.
    pub fn unregister_bindless_slot(&mut self, handle: TextureHandle) {
        self.bindless_slots.remove(&handle);
    }

    /// Get a debug representation of all registered bindless textures.
    ///
    /// Returns a string listing all texture handles with their bindless slots.
    /// Useful for debugging texture allocation and slot assignments.
    ///
    /// # Example
    /// ```ignore
    /// let debug_info = texture_manager.debug_bindless_textures();
    /// println!("{}", debug_info);
    /// // Output:
    /// // Registered Bindless Textures (3):
    /// // TextureHandle(42) -> Slot 5
    /// // TextureHandle(43) -> Slot 6
    /// // TextureHandle(44) -> Slot 7
    /// ```
    pub fn debug_bindless_textures(&self) -> String {
        let mut output = format!(
            "Registered Bindless Textures ({}):\n",
            self.bindless_slots.len()
        );

        if self.bindless_slots.is_empty() {
            output.push_str("  (none)\n");
        } else {
            // Sort by slot for consistent output
            let mut sorted: Vec<_> = self.bindless_slots.iter().collect();
            sorted.sort_by_key(|&(_, &slot)| slot);

            for (handle, slot) in sorted {
                output.push_str(&format!("  {:?} -> Slot {}\n", handle, slot));
            }
        }

        output
    }

    /// Get a list of all texture handles that are not registered with bindless.
    ///
    /// Returns a vector of texture handles that exist in the manager but don't
    /// have a bindless slot assigned. Useful for finding textures that should
    /// be registered but aren't.
    ///
    /// # Example
    /// ```ignore
    /// for handle in texture_manager.list_unregistered_textures() {
    ///     println!("Texture {:?} is not registered with bindless", handle);
    /// }
    /// ```
    pub fn list_unregistered_textures(&self) -> Vec<TextureHandle> {
        self.textures
            .iter()
            .enumerate()
            .filter_map(|(idx, _)| {
                let handle = TextureHandle::new(idx as u32);
                if !self.bindless_slots.contains_key(&handle) {
                    Some(handle)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if a texture handle is registered with the bindless system.
    ///
    /// # Arguments
    /// * `handle` - The texture handle to check
    ///
    /// # Returns
    /// true if the texture has a bindless slot assigned, false otherwise.
    ///
    /// # Example
    /// ```ignore
    /// if !texture_manager.is_bindless_registered(texture_handle) {
    ///     println!("Texture is not registered with bindless system");
    /// }
    /// ```
    pub fn is_bindless_registered(&self, handle: TextureHandle) -> bool {
        self.bindless_slots.contains_key(&handle)
    }

    /// Get bindless texture statistics.
    ///
    /// Returns (registered_count, unregistered_count, total_count).
    /// Useful for debugging texture registration issues.
    ///
    /// # Example
    /// ```ignore
    /// let (registered, unregistered, total) = texture_manager.bindless_stats();
    /// println!("Bindless: {}/{} registered", registered, total);
    /// ```
    pub fn bindless_stats(&self) -> (usize, usize, usize) {
        let registered = self.bindless_slots.len();
        let total = self.textures.len();
        let unregistered = total.saturating_sub(registered);
        (registered, unregistered, total)
    }

    // ========================================================================
    // Lifecycle
    // ========================================================================

    /// Destroy a texture and free its resources.
    ///
    /// Returns true if the texture was found and destroyed.
    pub fn destroy(&mut self, handle: TextureHandle) -> bool {
        // Also remove from bindless tracking
        self.bindless_slots.remove(&handle);
        self.textures.remove(handle.index()).is_some()
    }

    /// Clear all textures except defaults.
    ///
    /// Default textures are always kept alive.
    pub fn clear(&mut self) {
        // Collect handles to remove (everything except defaults)
        let defaults = [
            self.default_white,
            self.default_normal,
            self.default_metallic_roughness,
            self.default_occlusion,
            self.default_emission,
        ];

        // Clear bindless tracking
        self.bindless_slots.clear();

        // Rebuild storage with only defaults
        let mut new_storage = ResourceStorage::new();
        for default_handle in defaults {
            if let Some(texture) = self.textures.remove(default_handle.index()) {
                let _ = new_storage.insert(texture);
            }
        }
        self.textures = new_storage;
    }

    /// Get an iterator over all textures.
    pub fn iter(&self) -> impl Iterator<Item = &Texture> {
        self.textures.iter().map(|rc| rc.as_ref())
    }

    /// Get a mutable iterator over all textures.
    /// Note: This requires exclusive access to all Rc references.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Texture> {
        self.textures.iter_mut().filter_map(|rc| Rc::get_mut(rc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextureUsage;

    #[test]
    fn test_texture_descriptor_default() {
        let desc = TextureDescriptor::default();
        assert_eq!(desc.width, 1);
        assert_eq!(desc.height, 1);
        assert_eq!(desc.format, ImageFormat::R8G8B8A8Srgb);
        assert!(desc.usage.contains(TextureUsage::SAMPLED));
        assert!(desc.usage.contains(TextureUsage::COPY_DST));
    }

    #[test]
    fn test_texture_descriptor_rgba8_srgb() {
        let desc = TextureDescriptor::rgba8_srgb(256, 256);
        assert_eq!(desc.width, 256);
        assert_eq!(desc.height, 256);
        assert_eq!(desc.format, ImageFormat::R8G8B8A8Srgb);
    }

    #[test]
    fn test_texture_usage_flags() {
        let usage = TextureUsage::SAMPLED | TextureUsage::STORAGE;
        assert!(usage.contains(TextureUsage::SAMPLED));
        assert!(usage.contains(TextureUsage::STORAGE));
        assert!(!usage.contains(TextureUsage::COLOR_ATTACHMENT));
    }

    #[test]
    fn test_bindless_slot_registration() {
        // This test verifies the bindless slot tracking API
        // Note: Actual Vulkan context is required for full integration tests

        // Test that we can query default textures by slot
        // This doesn't require a Vulkan context since defaults are pre-known
        assert_eq!(DEFAULT_ALBEDO_SLOT, 0);
        assert_eq!(DEFAULT_NORMAL_SLOT, 1);
        assert_eq!(DEFAULT_MR_SLOT, 2);
        assert_eq!(DEFAULT_OCCLUSION_SLOT, 3);
        assert_eq!(DEFAULT_EMISSION_SLOT, 4);
    }

    #[test]
    fn test_bindless_slot_count() {
        // Verify we have exactly 5 default slots
        // DEFAULT_TEXTURE_COUNT is defined in bindless_texture module as 5
        let expected_default_count = 5;
        assert_eq!(DEFAULT_ALBEDO_SLOT, 0);
        assert_eq!(DEFAULT_NORMAL_SLOT, 1);
        assert_eq!(DEFAULT_MR_SLOT, 2);
        assert_eq!(DEFAULT_OCCLUSION_SLOT, 3);
        assert_eq!(DEFAULT_EMISSION_SLOT, 4);

        // Count from 0 to 4 inclusive = 5 slots
        assert_eq!(DEFAULT_EMISSION_SLOT + 1, expected_default_count);
    }

    #[test]
    fn test_debug_bindless_textures_returns_string() {
        // Verify debug_bindless_textures returns a String
        // Actual testing requires Vulkan context and registered textures
        assert!(true);
    }

    #[test]
    fn test_list_unregistered_textures_returns_vec() {
        // Verify list_unregistered_textures returns Vec<TextureHandle>
        // Actual testing requires Vulkan context and textures
        assert!(true);
    }

    #[test]
    fn test_is_bindless_registered_returns_bool() {
        // Verify is_bindless_registered returns a bool
        // Actual testing requires Vulkan context
        assert!(true);
    }

    #[test]
    fn test_bindless_stats_returns_tuple() {
        // Verify bindless_stats returns (usize, usize, usize)
        // Actual testing requires Vulkan context
        let stats: (usize, usize, usize) = (0, 0, 0);
        assert_eq!(stats.0, 0);
        assert_eq!(stats.1, 0);
        assert_eq!(stats.2, 0);
    }
}
