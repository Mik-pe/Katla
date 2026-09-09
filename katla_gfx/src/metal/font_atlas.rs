use objc2_metal::MTLTexture;

use crate::backend::resource::GpuImage;
use crate::handle::TextureHandle;
use crate::renderer::gpu_renderer::GpuRenderer;
use crate::texture::{ImageFormat, TextureDescriptor};

use super::metal_renderer::MetalRenderer;

impl MetalRenderer {
    pub(crate) fn create_ui_font_atlas_impl(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<TextureHandle, crate::error::RendererError> {
        log::debug!(
            "METAL create_ui_font_atlas: {}x{}, {} bytes, current_font_atlas={:?}",
            width,
            height,
            data.len(),
            self.ui_font_atlas,
        );
        let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Srgb);
        // Replacement-first: the old atlas is destroyed only after the new
        // one exists, so a failed creation keeps the previous atlas instead
        // of substituting a placeholder.
        let handle = GpuRenderer::create_texture(self, &desc, data)?;
        // Destroy the old atlas to free its bindless slot and GPU resource.
        // Without this, repeated calls leak textures and exhaust bindless slots.
        if let Some(old_handle) = self.ui_font_atlas.replace(handle) {
            GpuRenderer::destroy_texture(self, old_handle);
        }
        let slot = self.get_bindless_slot(handle);
        log::debug!(
            "METAL create_ui_font_atlas: created texture handle idx={}, bindless_slot={:?}",
            handle.index(),
            slot,
        );
        Ok(handle)
    }

    pub(crate) fn update_ui_font_atlas_impl(&mut self, width: u32, height: u32, data: &[u8]) {
        if let Some(atlas_handle) = self.ui_font_atlas {
            if let Some(entry) = self.textures.get(atlas_handle.index()) {
                let view = &entry._view;
                let atlas_texture = entry.texture.clone();
                let atlas_format = atlas_texture.format();
                let tex_w = view.inner.width() as u32;
                let tex_h = view.inner.height() as u32;
                if tex_w == width && tex_h == height {
                    if let Err(error) = self.texture_uploads.stage(
                        &self.context,
                        atlas_texture,
                        atlas_format,
                        width,
                        height,
                        data,
                    ) {
                        log::warn!("font atlas re-upload rejected ({error}); recreating atlas");
                    } else {
                        return;
                    }
                }
            }
            // Replacement-first: keep the previous atlas when recreation fails.
            let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Srgb);
            match GpuRenderer::create_texture(self, &desc, data) {
                Ok(handle) => {
                    GpuRenderer::destroy_texture(self, atlas_handle);
                    self.ui_font_atlas = Some(handle);
                }
                Err(error) => {
                    log::warn!("font atlas recreation failed ({error}); keeping previous atlas");
                }
            }
            return;
        }
        let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Srgb);
        match GpuRenderer::create_texture(self, &desc, data) {
            Ok(handle) => {
                self.ui_font_atlas = Some(handle);
            }
            Err(error) => {
                log::warn!("font atlas creation failed ({error}); UI text will miss glyphs");
            }
        }
    }

    pub(crate) fn ui_font_atlas_handle_impl(&self) -> Option<TextureHandle> {
        self.ui_font_atlas
    }
}
