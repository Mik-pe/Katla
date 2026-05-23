use objc2_metal::MTLTexture;

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
        let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Srgb);
        let handle = GpuRenderer::create_texture(self, &desc, data);
        self.ui_font_atlas = Some(handle);
        handle
    }

    pub(crate) fn update_ui_font_atlas_impl(&mut self, width: u32, height: u32, data: &[u8]) {
        if let Some(atlas_handle) = self.ui_font_atlas {
            if let Some(entry) = self.textures.get(atlas_handle.index()) {
                let tex = &entry._view;
                let tex_w = tex.inner.width() as u32;
                let tex_h = tex.inner.height() as u32;
                if tex_w == width && tex_h == height {
                    let region = objc2_metal::MTLRegion {
                        origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
                        size: objc2_metal::MTLSize {
                            width: width as usize,
                            height: height as usize,
                            depth: 1,
                        },
                    };
                    let bytes_per_row = width as usize * 4;
                    unsafe {
                        tex.inner.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                            region,
                            0,
                            std::ptr::NonNull::new(data.as_ptr() as *mut std::ffi::c_void).unwrap(),
                            bytes_per_row,
                        );
                    }
                    return;
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
