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
    ) -> TextureHandle {
        // Initial data goes through the staged upload queue and is blitted at
        // the start of the frame. Textures keep Shared storage until the
        // private-storage sampling anomaly is root-caused (see issue #58).
        let result = self.context.create_texture(desc);
        match result {
            Ok((texture, view)) => {
                if !data.is_empty()
                    && let Err(error) = self.texture_uploads.stage(
                        &self.context,
                        texture.clone(),
                        desc.format,
                        desc.width,
                        desc.height,
                        data,
                    )
                {
                    log::error!(
                        "texture upload rejected for {:?} ({}x{}, {:?}): {} — substituting placeholder per asset policy",
                        desc.label,
                        desc.width,
                        desc.height,
                        desc.format,
                        error
                    );
                    return self.default_texture_impl();
                }

                let bindless_slot = self.bindless_manager.register_texture(&texture.inner).ok();

                let entry = MetalTextureEntry {
                    texture: texture.clone(),
                    _view: view,
                    bindless_slot,
                };
                let id = self.textures.insert(entry);
                TextureHandle::new(id)
            }
            Err(error) => {
                log::error!(
                    "Metal texture creation failed for {:?} ({}x{}, {:?}): {} — substituting placeholder per asset policy",
                    desc.label,
                    desc.width,
                    desc.height,
                    desc.format,
                    error
                );
                self.default_texture_impl()
            }
        }
    }

    pub(crate) fn create_texture_solid_impl(&mut self, color: [u8; 4]) -> TextureHandle {
        let desc = TextureDescriptor::new(1, 1, ImageFormat::R8G8B8A8Srgb);
        self.create_texture_impl(&desc, &color)
    }

    pub(crate) fn get_bindless_slot_impl(&self, handle: TextureHandle) -> Option<u32> {
        self.textures
            .get(handle.index())
            .and_then(|entry| entry.bindless_slot)
    }

    pub(crate) fn get_texture_at_slot_impl(&self, slot: u32) -> Option<TextureHandle> {
        for (idx, entry) in self.textures.iter().enumerate() {
            if entry.bindless_slot == Some(slot) {
                return Some(TextureHandle::new(idx as u32));
            }
        }
        None
    }

    pub(crate) fn default_texture_impl(&self) -> TextureHandle {
        self.default_texture.unwrap_or_default()
    }

    pub(crate) fn destroy_texture_impl(&mut self, handle: TextureHandle) {
        if let Some(entry) = self.textures.remove(handle.index())
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
            let entry = self.textures.get(handle.index()).ok_or_else(|| {
                RendererError::InvalidOperation(format!("Invalid texture handle {:?}", handle))
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
