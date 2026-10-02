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
        desc.validate_data(data.len())
            .inspect_err(|_| self.texture_uploads.record_failure())?;
        let (texture, view) = self
            .context
            .create_texture(desc)
            .inspect_err(|_| self.texture_uploads.record_failure())?;
        let bindless_slot = if desc.depth == 1 && desc.array_layers == 1 {
            Some(
                self.bindless_manager
                    .register_texture(&texture.inner)
                    .inspect_err(|_| self.texture_uploads.record_failure())?,
            )
        } else {
            None
        };
        if !data.is_empty()
            && let Err(error) = self.texture_uploads.stage_region(
                &self.context,
                texture.clone(),
                desc,
                crate::texture::TextureUploadRegion::base(desc),
                data,
            )
        {
            if let Some(slot) = bindless_slot {
                self.bindless_manager.release_slot(slot);
            }
            return Err(error);
        }

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

    /// Resolve a draw's emission texture handle to its binding-table slot.
    ///
    /// The only place a draw emission handle becomes a shader-visible
    /// number. `NONE` and stale handles resolve to 0 — the shaders'
    /// no-emission sentinel — so a dead handle can never sample whatever
    /// texture now occupies a recycled slot.
    pub(crate) fn resolve_emission_texture_slot_impl(&self, handle: TextureHandle) -> u32 {
        self.get_bindless_slot_impl(handle).unwrap_or(0)
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
        texture
            .descriptor()
            .validate_data(data.len())
            .inspect_err(|_| self.texture_uploads.record_failure())?;
        self.texture_uploads
            .stage(&self.context, texture, format, width, height, data)?;
        Ok(())
    }
    pub(crate) fn update_texture_region_impl(
        &mut self,
        handle: TextureHandle,
        region: crate::texture::TextureUploadRegion,
        data: &[u8],
    ) -> Result<(), RendererError> {
        let texture = self
            .textures
            .get(handle)
            .ok_or_else(|| RendererError::StaleHandle {
                resource: "texture".into(),
                detail: format!("{handle:?} in Metal update_texture_region"),
            })?
            .texture
            .clone();
        let desc = texture.descriptor();
        self.texture_uploads
            .stage_region(&self.context, texture, &desc, region, data)
    }
    pub(crate) fn pending_texture_uploads_impl(
        &self,
    ) -> Vec<(TextureHandle, crate::texture::TextureUploadRegion)> {
        if !self.texture_uploads.has_pending() {
            return Vec::new();
        }
        let handles: std::collections::HashMap<_, _> = self
            .textures
            .iter_enumerated()
            .map(|(handle, entry)| (entry.texture.inner.gpuResourceID().to_raw(), handle))
            .collect();
        self.texture_uploads
            .pending_transfers()
            .into_iter()
            .filter_map(|(id, region)| handles.get(&id).copied().map(|handle| (handle, region)))
            .collect()
    }
}
