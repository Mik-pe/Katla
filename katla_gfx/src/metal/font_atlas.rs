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
    ) -> TextureHandle {
        log::debug!(
            "METAL create_ui_font_atlas: {}x{}, {} bytes, current_font_atlas={:?}",
            width,
            height,
            data.len(),
            self.ui_font_atlas,
        );
        // Destroy the old atlas to free its bindless slot and GPU resource.
        // Without this, repeated calls leak textures and exhaust bindless slots.
        if let Some(old_handle) = self.ui_font_atlas.take() {
            GpuRenderer::destroy_texture(self, old_handle);
        }
        let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Srgb);
        let handle = GpuRenderer::create_texture(self, &desc, data);
        let slot = self.get_bindless_slot(handle);
        log::debug!(
            "METAL create_ui_font_atlas: created texture handle idx={}, bindless_slot={:?}",
            handle.index(),
            slot,
        );
        self.ui_font_atlas = Some(handle);
        handle
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
            GpuRenderer::destroy_texture(self, atlas_handle);
        }
        let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Srgb);
        let handle = GpuRenderer::create_texture(self, &desc, data);
        self.ui_font_atlas = Some(handle);
    }

    pub(crate) fn ui_font_atlas_handle_impl(&self) -> Option<TextureHandle> {
        self.ui_font_atlas
    }
}
