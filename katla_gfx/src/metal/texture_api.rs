use objc2_metal::MTLTexture;

use crate::backend::resource::GpuImage;
use crate::error::RendererError;
use crate::handle::TextureHandle;
use crate::texture::{ImageFormat, TextureDescriptor};

use super::metal_renderer::{MetalRenderer, MetalTextureEntry};

impl MetalRenderer {
    pub(crate) fn create_texture_impl(
        &mut self,
        desc: &TextureDescriptor,
        data: &[u8],
    ) -> Result<TextureHandle, RendererError> {
        // Initial data goes through the staged upload queue and is blitted at
        // the start of the frame. Textures keep Shared storage until the
        // private-storage sampling anomaly is root-caused (see issue #58).
        // Empty data creates the texture uninitialized for later upload.
        desc.validate_data(data.len())?;
        let (texture, view) = self.context.create_texture(desc)?;
        if !data.is_empty() {
            // Staging after creation: propagate the error without inserting
            // anything. The Metal texture drops with its refcount; no slot
            // was registered. Callers decide fallback policy explicitly.
            self.texture_uploads.stage(
                &self.context,
                texture.clone(),
                desc.format,
                desc.width,
                desc.height,
                data,
            )?;
        }

        let bindless_slot = self.bindless_manager.register_texture(&texture.inner).ok();

        let entry = MetalTextureEntry {
            texture: texture.clone(),
            _view: view,
            bindless_slot,
        };
        Ok(self.textures.insert(entry))
    }

    pub(crate) fn create_texture_solid_impl(
        &mut self,
        color: [u8; 4],
    ) -> Result<TextureHandle, RendererError> {
        let desc = TextureDescriptor::new(1, 1, ImageFormat::R8G8B8A8Srgb);
        self.create_texture_impl(&desc, &color)
    }

    pub(crate) fn get_bindless_slot_impl(&self, handle: TextureHandle) -> Option<u32> {
        self.textures
            .get(handle)
            .and_then(|entry| entry.bindless_slot)
    }

    pub(crate) fn get_texture_at_slot_impl(&self, slot: u32) -> Option<TextureHandle> {
        self.textures
            .iter_enumerated()
            .find(|(_, entry)| entry.bindless_slot == Some(slot))
            .map(|(handle, _)| handle)
    }

    pub(crate) fn default_texture_impl(&self) -> TextureHandle {
        self.default_texture.unwrap_or_default()
    }

    pub(crate) fn destroy_texture_impl(&mut self, handle: TextureHandle) {
        if let Some(entry) = self.textures.remove(handle)
            && let Some(slot) = entry.bindless_slot
        {
            self.bindless_manager.release_slot(slot);
        }
    }

    pub(crate) fn update_texture_impl(
        &mut self,
        handle: TextureHandle,
        data: &[u8],
    ) -> Result<(), RendererError> {
        let (format, width, height, texture) = {
            let entry = self
                .textures
                .get(handle)
                .ok_or_else(|| RendererError::StaleHandle {
                    resource: "texture".to_string(),
                    detail: format!("{handle:?} in Metal update_texture"),
                })?;
            let view = &entry._view;
            let texture = entry.texture.clone();
            let format = texture.format();
            let width = view.inner.width() as u32;
            let height = view.inner.height() as u32;
            (format, width, height, texture)
        };
        self.texture_uploads
            .stage(&self.context, texture, format, width, height, data)?;
        Ok(())
    }
}
