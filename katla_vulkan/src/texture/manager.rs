//! Texture manager for centralized texture creation and storage.
//!
//! TextureManager provides a clean API for creating, storing, and looking up
//! textures using opaque TextureHandle values. It also manages default textures
//! for common use cases.

use crate::handle::{ResourceStorage, TextureHandle};
use crate::render_graph::types::ImageFormat;
use crate::sync::VkImageView;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::texture::Texture;
use ash::vk;
use std::collections::HashMap;
use std::rc::Rc;

use super::descriptor::{TextureDescriptor, TextureUsage};

/// Default texture slot indices.
/// These match BindlessTextureManager's default slots for consistency.
pub const DEFAULT_ALBEDO_SLOT: u32 = 0;
pub const DEFAULT_NORMAL_SLOT: u32 = 1;
pub const DEFAULT_MR_SLOT: u32 = 2;
pub const DEFAULT_OCCLUSION_SLOT: u32 = 3;
pub const DEFAULT_EMISSION_SLOT: u32 = 4;

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

    /// Get the Vulkan image view for a texture handle.
    ///
    /// This is for internal use by renderers that need the raw Vulkan handle.
    pub fn get_view(&self, handle: TextureHandle) -> Option<VkImageView> {
        self.textures.get(handle.index()).map(|t| t.image_view)
    }

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
        self.textures.get_mut(handle.index()).map(|rc| Rc::get_mut(rc)).flatten()
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

    /// Remove a bindless slot registration.
    pub fn unregister_bindless_slot(&mut self, handle: TextureHandle) {
        self.bindless_slots.remove(&handle);
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
}
