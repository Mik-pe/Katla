//! UI rendering subsystem.
//!
//! The `UIRenderer` owns all UI-specific rendering state and provides a clean API
//! for UI operations without polluting the core renderer interface.

use crate::TextureHandle;
use crate::renderer::UiFrameResources;

/// UI rendering subsystem.
///
/// Owns all UI-specific GPU resources and provides font atlas management.
/// Wraps a reference to VulkanRenderer for low-level graphics operations.
pub struct UIRenderer {
    /// Per-frame UI rendering resources (vertex/index buffers, descriptor sets, uniform buffer).
    ui_resources: UiFrameResources,
    /// Font atlas texture handle for text rendering.
    font_atlas: Option<TextureHandle>,
}

impl UIRenderer {
    /// Create a new UI rendering subsystem.
    pub(crate) fn new() -> Self {
        Self {
            ui_resources: UiFrameResources::default(),
            font_atlas: None,
        }
    }

    /// Create or update the UI font atlas texture from pixel data.
    ///
    /// This method creates a texture with the given dimensions and uploads the pixel data.
    /// If a font atlas already exists, it will be replaced.
    ///
    /// # Arguments
    /// * `renderer` - The VulkanRenderer for GPU operations
    /// * `width` - Texture width in pixels
    /// * `height` - Texture height in pixels
    /// * `data` - Raw pixel data (RGBA8 format, 4 bytes per pixel)
    ///
    /// # Returns
    /// A `TextureHandle` for the font atlas texture.
    ///
    /// # Example
    /// ```ignore
    /// ui_renderer.create_font_atlas(&mut renderer, 512, 512, &font_pixels)?;
    /// ```
    pub fn create_font_atlas(
        &mut self,
        renderer: &mut crate::VulkanRenderer,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<TextureHandle, crate::RendererError> {
        log::debug!("Creating UI font atlas: {}x{} pixels", width, height);

        // Use renderer's texture creation method (automatically registers with bindless)
        let handle = renderer.create_texture_rgba(width, height, data);

        // Get the bindless index for logging
        let bindless_idx = renderer.get_texture_bindless_index(handle);

        log::debug!(
            "Font atlas registered with bindless index: {}",
            bindless_idx
        );

        self.font_atlas = Some(handle);

        Ok(handle)
    }

    /// Update the UI font atlas texture with new pixel data.
    ///
    /// If a font atlas doesn't exist yet, this will create a new one.
    ///
    /// # Arguments
    /// * `renderer` - The VulkanRenderer for GPU operations
    /// * `width` - Texture width in pixels
    /// * `height` - Texture height in pixels
    /// * `data` - Raw pixel data (RGBA8 format, 4 bytes per pixel)
    ///
    /// # Example
    /// ```ignore
    /// ui_renderer.update_font_atlas(&mut renderer, 512, 512, &new_font_pixels)?;
    /// ```
    pub fn update_font_atlas(
        &mut self,
        renderer: &mut crate::VulkanRenderer,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<(), crate::RendererError> {
        if let Some(_handle) = self.font_atlas {
            log::debug!("Updating UI font atlas: {}x{} pixels", width, height);

            // For updates, we create a new texture and replace the old one
            // This is simpler than trying to update in place
            let new_handle = renderer.create_texture_rgba(width, height, data);

            log::debug!("Font atlas updated successfully");

            self.font_atlas = Some(new_handle);
        } else {
            log::warn!("update_font_atlas called but no font atlas exists yet, creating new one");
            self.create_font_atlas(renderer, width, height, data)?;
        }

        Ok(())
    }

    /// Get the font atlas texture handle.
    ///
    /// Returns `None` if no font atlas has been created yet.
    pub fn font_atlas(&self) -> Option<TextureHandle> {
        self.font_atlas
    }

    /// Get mutable access to UI resources (for frame graph execution).
    ///
    /// This is used internally by the frame graph to manage per-frame UI buffers.
    pub(crate) fn ui_resources_mut(&mut self) -> &mut UiFrameResources {
        &mut self.ui_resources
    }

    /// Get the font atlas texture handle (for frame graph execution).
    ///
    /// This is used internally by the frame graph when binding UI resources.
    pub(crate) fn font_atlas_handle(&self) -> Option<TextureHandle> {
        self.font_atlas
    }
}

impl Default for UIRenderer {
    fn default() -> Self {
        Self::new()
    }
}
