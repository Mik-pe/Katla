use super::*;

impl VulkanRenderer {
    /// Create a texture from a descriptor and pixel data.
    ///
    /// This is the primary method for texture creation. The texture is
    /// automatically registered with the bindless system.
    ///
    /// # Arguments
    /// * `desc` - Texture descriptor specifying dimensions, format, and usage
    /// * `data` - Pixel data (must match descriptor dimensions and format)
    ///
    /// # Returns
    /// A TextureHandle for the created texture.
    ///
    /// # Example
    /// ```ignore
    /// use katla_gfx::{TextureDescriptor, VulkanRenderer};
    ///
    /// let desc = TextureDescriptor::rgba8_srgb(512, 512);
    /// let texture = renderer.create_texture(&desc, &pixel_data);
    /// ```
    pub fn create_texture(&mut self, desc: &TextureDescriptor, data: &[u8]) -> TextureHandle {
        let handle = self.texture_manager.create(desc, data);

        if let Some(texture) = self.texture_manager.get_texture_rc(handle) {
            let slot = self
                .bindless_manager
                .register_texture(texture.image_view().vk())
                .expect("Failed to register texture with bindless system");
            self.texture_manager.register_bindless_slot(handle, slot);
        }

        handle
    }

    /// Create a 1x1 solid color texture.
    ///
    /// Useful for placeholder or fallback textures.
    /// The texture is automatically registered with the bindless system.
    pub fn create_texture_solid(&mut self, color: [u8; 4]) -> TextureHandle {
        let handle = self.texture_manager.create_solid(color);

        if let Some(texture) = self.texture_manager.get_texture_rc(handle) {
            let slot = self
                .bindless_manager
                .register_texture(texture.image_view().vk())
                .expect("Failed to register solid texture with bindless system");
            self.texture_manager.register_bindless_slot(handle, slot);
        }

        handle
    }

    /// Get the default white texture.
    pub fn default_texture(&self) -> TextureHandle {
        self.texture_manager.default_white()
    }

    /// Get the shared sampler used by the bindless texture system.
    ///
    /// This sampler can be used for transient textures that need to be sampled
    /// (e.g., viewport render targets displayed in the UI).
    pub fn shared_sampler(&self) -> crate::sync::VkSampler {
        self.bindless_manager.shared_sampler()
    }
}
