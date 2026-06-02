use objc2_metal::MTLCommandBuffer;

use crate::error::RendererError;
use crate::texture::ImageFormat;

use super::metal_renderer::MetalRenderer;

impl MetalRenderer {
    pub(crate) fn wait_for_frame_impl(&mut self) -> Result<(), RendererError> {
        if let Some(cmd_buffer) = self.last_command_buffer.take() {
            cmd_buffer.waitUntilCompleted();
        }
        Ok(())
    }

    pub(crate) fn begin_frame_impl(&mut self) -> Result<u32, RendererError> {
        // If a headless drawable is already set, skip acquiring from the surface
        if self.current_drawable_texture.is_some() {
            return Ok(self.frame_index);
        }

        let texture = self.context.surface.acquire_next_drawable()?;
        self.drawable_texture_view = Some(super::texture::MetalTextureView::new(
            texture.clone(),
            super::texture::MetalTexture::new(texture.clone(), ImageFormat::B8G8R8A8Srgb),
        ));
        self.current_drawable_texture = Some(texture);
        Ok(self.frame_index)
    }

    pub(crate) fn end_frame_impl(&mut self) -> Result<(), RendererError> {
        // In headless mode, keep the drawable texture for readback.
        // It will be cleaned up by take_headless_texture() or destroy().
        // Just clear the view reference so the next frame gets a fresh view.
        self.drawable_texture_view = None;
        self.frame_index += 1;
        Ok(())
    }
}
