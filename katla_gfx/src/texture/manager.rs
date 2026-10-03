//! Texture manager for centralized texture creation and storage.
//!
//! TextureManager provides a clean API for creating, storing, and looking up
//! textures using opaque TextureHandle values and one descriptor-safe fallback.

use crate::handle::{ResourceStorage, TextureHandle, TextureMarker};
use crate::vulkan::context::VulkanContext;
use crate::vulkan::texture::Texture;
use std::collections::HashMap;
use std::rc::Rc;

use super::descriptor::TextureDescriptor;

/// Centralized texture creation and storage.
///
/// TextureManager provides:
/// - Handle-based texture creation (no direct Vulkan exposure)
/// - One descriptor-safe fallback texture
/// - Lookup by handle for internal rendering operations
/// - Optional bindless slot tracking
pub struct TextureManager {
    /// Storage for all textures.
    textures: ResourceStorage<Rc<Texture>, TextureMarker>,
    /// Vulkan context for texture creation.
    context: Rc<VulkanContext>,
    /// Descriptor-safe fallback texture.
    default_texture: TextureHandle,
    /// Optional bindless slot tracking.
    /// Maps TextureHandle -> bindless slot index.
    bindless_slots: HashMap<TextureHandle, u32>,
}

impl TextureManager {
    /// Create storage and one descriptor-safe fallback.
    pub fn new(context: Rc<VulkanContext>) -> Result<Self, crate::error::RendererError> {
        let mut textures = ResourceStorage::new();
        let texture =
            Texture::from_descriptor(&context, &TextureDescriptor::rgba8_unorm(1, 1), &[255; 4])?;
        let default_texture = textures.insert(Rc::new(texture));
        Ok(Self {
            textures,
            context,
            default_texture,
            bindless_slots: HashMap::new(),
        })
    }

    // ========================================================================
    // Creation API
    // ========================================================================

    /// Create a texture from a descriptor and pixel data.
    ///
    /// # Arguments
    /// * `desc` - Texture descriptor specifying dimensions, format, and usage
    /// * `data` - Pixel data (must match descriptor dimensions and format;
    ///   empty data creates the texture uninitialized for later upload)
    ///
    /// # Returns
    /// A `TextureHandle` for the created texture, or a typed error when the
    /// descriptor rejects the data, allocation fails, or upload fails. Failed
    /// creation inserts nothing: no half-created texture is retained.
    pub fn create(
        &mut self,
        desc: &TextureDescriptor,
        data: &[u8],
    ) -> Result<TextureHandle, crate::error::RendererError> {
        let texture = Rc::new(Texture::from_descriptor(&self.context, desc, data)?);
        Ok(self.textures.insert(texture))
    }

    /// Create an RGBA8 SRGB texture from pixel data.
    ///
    /// Convenience method for common texture type.
    pub fn create_rgba(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<TextureHandle, crate::error::RendererError> {
        let desc = TextureDescriptor::rgba8_srgb(width, height);
        self.create(&desc, data)
    }

    /// Create an RGBA8 UNORM texture from pixel data.
    ///
    /// Use for linear data like normal maps.
    pub fn create_rgba_unorm(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<TextureHandle, crate::error::RendererError> {
        let desc = TextureDescriptor::rgba8_unorm(width, height);
        self.create(&desc, data)
    }

    /// Create a 1x1 solid color texture.
    ///
    /// Useful for placeholder or fallback textures.
    pub fn create_solid(
        &mut self,
        color: [u8; 4],
    ) -> Result<TextureHandle, crate::error::RendererError> {
        self.create_rgba(1, 1, &color)
    }

    /// Create an empty texture (no initial data).
    ///
    /// Useful for render targets or textures that will be filled later.
    pub fn create_empty(
        &mut self,
        desc: &TextureDescriptor,
    ) -> Result<TextureHandle, crate::error::RendererError> {
        // Zeroed data sized by the descriptor's own expectation, so the
        // validation below cannot disagree with this constructor.
        let size = desc.expected_bytes().ok_or_else(|| {
            crate::error::RendererError::InvalidDescriptor {
                resource: "texture".to_string(),
                reason: format!(
                    "{}x{} {:?}: dimensions overflow",
                    desc.width, desc.height, desc.format
                ),
            }
        })?;
        let data = vec![0u8; size];
        self.create(desc, &data)
    }

    /// The descriptor-safe fallback; application texture policy is explicit.
    pub fn default_texture(&self) -> TextureHandle {
        self.default_texture
    }

    // ========================================================================
    // Lookup (internal use)
    // ========================================================================

    /// Get an Rc reference to the Texture for a handle.
    ///
    /// This returns a clone of the Rc, allowing the caller to keep the texture alive.
    /// The cloned `Rc<Texture>` retains the native texture while an operation uses it.
    pub fn get_texture_rc(&self, handle: TextureHandle) -> Option<Rc<Texture>> {
        self.textures.get(handle).cloned()
    }

    /// Get a reference to the Texture for a handle.
    pub fn get_texture(&self, handle: TextureHandle) -> Option<&Texture> {
        self.textures.get(handle).map(|rc| rc.as_ref())
    }

    /// Get a mutable reference to the Texture for a handle.
    pub fn get_texture_mut(&mut self, handle: TextureHandle) -> Option<&mut Texture> {
        self.textures.get_mut(handle).and_then(|rc| Rc::get_mut(rc))
    }

    /// Check if a handle points to a valid texture.
    pub fn contains(&self, handle: TextureHandle) -> bool {
        self.textures.contains(handle)
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
    ///
    /// Fails with [`crate::RendererError::StaleHandle`] for dead handles and
    /// [`crate::RendererError::UploadFailed`] for size mismatches — never reports
    /// success without uploading.
    pub fn update_data(
        &self,
        handle: TextureHandle,
        data: &[u8],
    ) -> Result<(), crate::error::RendererError> {
        let texture =
            self.get_texture(handle)
                .ok_or_else(|| crate::error::RendererError::StaleHandle {
                    resource: "texture".to_string(),
                    detail: format!("{handle:?} in TextureManager::update_data"),
                })?;
        texture.update_data(data)
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
            .iter_enumerated()
            .filter(|(handle, _)| !self.bindless_slots.contains_key(handle))
            .map(|(handle, _)| handle)
            .collect()
    }

    /// Whether this is the fallback retained for descriptor validity.
    pub fn is_default_texture(&self, handle: TextureHandle) -> bool {
        handle == self.default_texture
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

    /// Destroy a texture, returning it for deferred retirement.
    ///
    /// Removes the handle from storage (invalidating it immediately) and
    /// drops the bindless slot registration. The returned `Rc<Texture>` is
    /// the manager's reference: the caller queues it for retirement so the
    /// native image stays alive until the submissions that can still sample
    /// it have completed. `None` means the handle was not live.
    pub fn destroy(&mut self, handle: TextureHandle) -> Option<Rc<Texture>> {
        // Also remove from bindless tracking
        self.bindless_slots.remove(&handle);
        self.textures.remove(handle)
    }

    /// Invalidate application textures while retaining the descriptor fallback.
    pub fn clear(&mut self) {
        let non_defaults: Vec<TextureHandle> = self
            .textures
            .iter_enumerated()
            .map(|(handle, _)| handle)
            .filter(|handle| *handle != self.default_texture)
            .collect();
        for handle in non_defaults {
            self.bindless_slots.remove(&handle);
            self.textures.remove(handle);
        }
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
    use crate::texture::ImageFormat;

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
