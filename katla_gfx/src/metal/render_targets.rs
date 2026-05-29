use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};

use super::metal_renderer::MetalRenderer;

impl MetalRenderer {
    pub(crate) fn recreate_render_targets(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        {
            let desc = TextureDescriptor::new(width, height, ImageFormat::D32Sfloat)
                .with_usage(TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED);
            if let Ok((_tex, view)) = self.context.create_texture(&desc) {
                self.depth_texture_view = Some(view);
            }
        }

        {
            let desc = TextureDescriptor::new(width, height, ImageFormat::R16G16B16A16Sfloat)
                .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
            if let Ok((_tex, view)) = self.context.create_texture(&desc) {
                self.hdr_color_view = Some(view);
            }
        }

        {
            let desc = TextureDescriptor::new(width, height, ImageFormat::D32SfloatS8Uint)
                .with_usage(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
            if let Ok((_tex, view)) = self.context.create_texture(&desc) {
                self.depth_stencil_view = Some(view);
            }
        }

        if let Err(e) = self.picking.resize(&self.context, width, height) {
            log::warn!("Failed to resize picking object-ID texture: {}", e);
        }
    }
}
