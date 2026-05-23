use objc2_metal::MTLTexture;

use crate::handle::TextureHandle;
use crate::texture::{ImageFormat, TextureDescriptor};

use super::metal_renderer::{MetalRenderer, MetalTextureEntry};

impl MetalRenderer {
    pub(crate) fn create_texture_impl(
        &mut self,
        desc: &TextureDescriptor,
        data: &[u8],
    ) -> TextureHandle {
        let result = if data.is_empty() {
            self.context.create_texture(desc)
        } else {
            self.context.create_texture_with_data(desc)
        };
        match result {
            Ok((texture, view)) => {
                if !data.is_empty() {
                    let region = objc2_metal::MTLRegion {
                        origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
                        size: objc2_metal::MTLSize {
                            width: desc.width as usize,
                            height: desc.height as usize,
                            depth: 1,
                        },
                    };
                    let bytes_per_row = desc.width as usize * 4;
                    unsafe {
                        texture
                            .inner
                            .replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                                region,
                                0,
                                std::ptr::NonNull::new(data.as_ptr() as *mut std::ffi::c_void)
                                    .unwrap(),
                                bytes_per_row,
                            );
                    }
                }

                let bindless_slot = self.bindless_manager.register_texture(&texture.inner).ok();

                let entry = MetalTextureEntry {
                    _view: view,
                    bindless_slot,
                };
                let id = self.textures.insert(entry);
                TextureHandle::new(id)
            }
            Err(_) => self.default_texture_impl(),
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
        self.default_texture.unwrap_or(TextureHandle::default())
    }

    pub(crate) fn destroy_texture_impl(&mut self, handle: TextureHandle) {
        if let Some(entry) = self.textures.remove(handle.index()) {
            if let Some(slot) = entry.bindless_slot {
                self.bindless_manager.release_slot(slot);
            }
        }
    }
}
