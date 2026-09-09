use super::*;

impl VulkanRenderer {
    /// Create or update the UI font atlas texture from pixel data.
    ///
    /// Creates a texture with the given dimensions and uploads the pixel data.
    /// The texture is automatically registered with the bindless system for shader access.
    ///
    /// # Arguments
    /// * `width` - Atlas width in pixels
    /// * `height` - Atlas height in pixels
    /// * `data` - RGBA pixel data
    ///
    /// # Returns
    /// The texture handle for the font atlas, or a typed error when creation
    /// fails. Failed creation changes nothing.
    pub fn create_ui_font_atlas(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<TextureHandle, crate::error::RendererError> {
        let desc = TextureDescriptor::rgba8_unorm(width, height);
        let handle = self.create_texture(&desc, data)?;

        // create_texture() already registers with the bindless system.
        // Use that slot instead of registering a second time.
        if let Some(slot) = self.texture_manager.get_bindless_slot(handle) {
            self.ui_renderer.set_font_atlas_bindless_slot(slot);
            log::debug!(
                "Font atlas registered with bindless system at slot {}",
                slot
            );
        } else {
            log::error!("Font atlas not registered with bindless system after create_texture");
        }

        self.ui_renderer.set_font_atlas(handle);
        Ok(handle)
    }

    /// Update the UI font atlas texture with new pixel data.
    ///
    /// Use this when the atlas has been resized or new glyphs have been added.
    ///
    /// # Arguments
    /// * `width` - Atlas width in pixels
    /// * `height` - Atlas height in pixels
    /// * `data` - RGBA pixel data
    pub fn update_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) {
        let current_handle = self.ui_renderer.font_atlas();

        if let Some(handle) = current_handle {
            if let Some(texture) = self.texture_manager.get_texture_rc(handle) {
                if texture.width == width && texture.height == height {
                    if let Err(error) =
                        super::gpu_renderer::GpuRenderer::update_texture(self, handle, data)
                    {
                        log::warn!("Font atlas re-upload failed ({error}); keeping previous atlas");
                    }
                } else {
                    // Replacement-first: keep the old atlas when recreation fails.
                    match self.create_ui_font_atlas(width, height, data) {
                        Ok(new_handle) => {
                            self.destroy_texture(handle);
                            self.ui_renderer.set_font_atlas(new_handle);
                        }
                        Err(error) => {
                            log::warn!(
                                "Font atlas recreation failed ({error}); keeping previous atlas"
                            );
                        }
                    }
                }
            }
        } else if let Err(error) = self
            .create_ui_font_atlas(width, height, data)
            .map(|handle| self.ui_renderer.set_font_atlas(handle))
        {
            log::warn!("Font atlas creation failed ({error}); UI text will miss glyphs");
        }
    }

    /// Get the font atlas texture handle.
    pub fn ui_font_atlas(&self) -> Option<TextureHandle> {
        self.ui_renderer.font_atlas()
    }
}
