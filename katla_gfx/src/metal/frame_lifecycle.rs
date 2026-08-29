use objc2_metal::{MTLCommandBuffer, MTLCommandBufferStatus};

use crate::error::{GpuExecutionFailure, RendererError};
use crate::texture::ImageFormat;

use super::metal_renderer::MetalRenderer;

impl MetalRenderer {
    pub(crate) fn wait_for_frame_impl(&mut self) -> Result<(), RendererError> {
        if let Some(cmd_buffer) = self.last_command_buffer.take() {
            cmd_buffer.waitUntilCompleted();
            self.texture_uploads.retire_completed();

            let status = cmd_buffer.status();
            if status != MTLCommandBufferStatus::Completed {
                let label = cmd_buffer
                    .label()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "<unlabeled>".to_string());
                let error = cmd_buffer.error();
                let code = error.as_ref().map(|value| value.code() as i64);
                let domain = error.as_ref().map(|value| value.domain().to_string());
                let description = error
                    .as_ref()
                    .map(|value| value.localizedDescription().to_string());

                return Err(RendererError::GpuExecutionFailed(Box::new(
                    GpuExecutionFailure {
                        backend: "Metal",
                        label,
                        status: format!("{status:?}"),
                        code,
                        domain,
                        description,
                    },
                )));
            }
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
        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }
}
