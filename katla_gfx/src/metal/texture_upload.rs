//! Staged texture uploads for Metal.
//!
//! Initial texture data never lives in a Shared texture: bytes go into a pooled
//! Shared staging buffer, a blit pass copies them into the Private destination
//! before any consumer pass of the frame, and staging slots are recycled only
//! after the consuming submission completes.

use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::metal::buffer::MetalBuffer;
use crate::metal::context::MetalContext;
use crate::metal::texture::MetalTexture;
use crate::texture::ImageFormat;

/// Validates an upload against its descriptor. Returns a typed error naming
/// the row pitch so callers can fix the data layout instead of guessing.
pub(crate) fn validate_upload(
    format: ImageFormat,
    width: u32,
    height: u32,
    len: usize,
) -> Result<(), RendererError> {
    if matches!(
        format,
        ImageFormat::D32Sfloat | ImageFormat::D32SfloatS8Uint | ImageFormat::D24UnormS8Uint
    ) {
        return Err(RendererError::InvalidOperation(format!(
            "initial data upload into depth format {format:?} is not supported; render targets are created empty"
        )));
    }
    if width == 0 || height == 0 {
        return Err(RendererError::InvalidOperation(format!(
            "texture upload for {width}x{height}: zero extent"
        )));
    }
    let bytes_per_pixel = format.bytes_per_pixel();
    let bytes_per_row = width * bytes_per_pixel;
    let expected = bytes_per_row as usize * height as usize;
    if len != expected {
        return Err(RendererError::InvalidOperation(format!(
            "texture upload for {width}x{height} {format:?} expects {expected} bytes (row pitch {bytes_per_row}), got {len}"
        )));
    }
    Ok(())
}

#[derive(Clone)]
struct StagingSlot {
    buffer: MetalBuffer,
}

/// One staged upload awaiting its blit into private storage.
#[derive(Clone)]
struct PendingTextureUpload {
    staging: MetalBuffer,
    dst: MetalTexture,
    bytes_per_row: usize,
    width: u32,
    height: u32,
}

/// Pooled staging buffers plus the batch of pending uploads.
#[derive(Default)]
pub(crate) struct TextureUploadQueue {
    pending: Vec<PendingTextureUpload>,
    in_flight: Vec<StagingSlot>,
    free: Vec<StagingSlot>,
    staged_bytes_this_batch: usize,
}

impl TextureUploadQueue {
    /// Copies `data` into a staging buffer and records the pending blit.
    pub(crate) fn stage(
        &mut self,
        context: &MetalContext,
        dst: MetalTexture,
        format: ImageFormat,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<(), RendererError> {
        validate_upload(format, width, height, data.len())?;

        let bytes_per_row = (width * format.bytes_per_pixel()) as usize;
        let expected = bytes_per_row * height as usize;

        let slot = match self.free.pop() {
            Some(slot) if slot.buffer.size() as usize >= expected => slot,
            _ => StagingSlot {
                buffer: context.create_buffer(expected as u64, true)?,
            },
        };

        let map = slot.buffer.map();
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), map, expected);
        }
        slot.buffer.unmap();

        self.pending.push(PendingTextureUpload {
            staging: slot.buffer.clone(),
            dst,
            bytes_per_row,
            width,
            height,
        });
        self.staged_bytes_this_batch += expected;
        self.in_flight.push(slot);
        Ok(())
    }

    /// Encodes blits for every pending upload into the current blit pass.
    pub(crate) fn encode_into(
        &mut self,
        encoder: &mut crate::metal::blit_encoder::MetalBlitEncoder,
    ) {
        for upload in self.pending.drain(..) {
            encoder.copy_buffer_to_texture_staged(
                &upload.staging,
                &upload.dst,
                upload.bytes_per_row,
                upload.width,
                upload.height,
            );
        }
    }

    /// Returns staging slots whose consuming submission has completed to the pool.
    pub(crate) fn retire_completed(&mut self) {
        for slot in self.in_flight.drain(..) {
            self.free.push(slot);
        }
    }

    /// True while any staged upload awaits encoding.
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_metal::{MTLCommandBuffer, MTLOrigin, MTLRegion, MTLSize, MTLTexture};

    use crate::backend::command::{GpuBlitEncoder, GpuCommandBuffer};
    use crate::backend::resource::GpuBuffer;
    use crate::metal::blit_encoder::MetalBlitEncoder;
    use crate::texture::{TextureDescriptor, TextureUsage};

    fn headless_context() -> MetalContext {
        MetalContext::init_headless().unwrap()
    }

    #[test]
    fn test_validate_upload_rejects_depth_formats() {
        let result = validate_upload(ImageFormat::D32Sfloat, 4, 4, 64);
        assert!(result.is_err(), "depth upload must be rejected");
        let message = result.err().unwrap().to_string();
        assert!(
            message.contains("depth"),
            "error names the depth format: {message}"
        );
    }

    #[test]
    fn test_validate_upload_rejects_size_mismatch() {
        let result = validate_upload(ImageFormat::R8G8B8A8Unorm, 4, 4, 63);
        assert!(result.is_err(), "size mismatch must be rejected");
        let message = result.err().unwrap().to_string();
        assert!(
            message.contains("row pitch"),
            "error names row pitch: {message}"
        );
    }

    #[test]
    fn test_validate_upload_accepts_exact_size() {
        assert!(validate_upload(ImageFormat::R8G8B8A8Unorm, 4, 4, 64).is_ok());
        assert!(validate_upload(ImageFormat::R8Unorm, 3, 5, 15).is_ok());
    }

    #[test]
    fn test_roundtrip_preserves_bytes() {
        let ctx = headless_context();
        let width = 8;
        let height = 8;
        let pixels: Vec<u8> = (0..width * height * 4)
            .map(|i| (i * 37 % 256) as u8)
            .collect();

        let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Unorm);
        let (texture, _view) = ctx.create_texture(&desc).unwrap();

        let mut queue = TextureUploadQueue::default();
        queue
            .stage(
                &ctx,
                texture.clone(),
                ImageFormat::R8G8B8A8Unorm,
                width,
                height,
                &pixels,
            )
            .unwrap();

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();
        {
            let mut blit = cmd_buffer.begin_blit_pass();
            queue.encode_into(&mut blit);
            blit.end_encoding();
        }
        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        queue.retire_completed();
        unsafe { cmd_buffer.inner.waitUntilCompleted() };

        // Copy private result into a shared mirror and read back.
        let (mirror, _) = ctx.create_texture_shared(&desc).unwrap();
        let mut copy_cmd = ctx.create_command_buffer();
        copy_cmd.begin();
        {
            let mut blit = copy_cmd.begin_blit_pass();
            blit.copy_texture_to_texture(&texture, &mirror);
            blit.end_encoding();
        }
        copy_cmd.end();
        copy_cmd.submit(&ctx);
        unsafe { copy_cmd.inner.waitUntilCompleted() };

        let region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize {
                width: width as usize,
                height: height as usize,
                depth: 1,
            },
        };
        let mut out = vec![0u8; pixels.len()];
        unsafe {
            mirror.inner.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                std::ptr::NonNull::new(out.as_mut_ptr() as *mut std::ffi::c_void).unwrap(),
                width as usize * 4,
                region,
                0,
            );
        }
        assert_eq!(out, pixels, "roundtrip must preserve every byte");
    }

    #[test]
    fn test_replace_region_vs_blit_content_identical() {
        let ctx = headless_context();
        let width = 8;
        let height = 8;
        let data: Vec<u8> = (0..width * height * 4)
            .map(|i| (i * 37 % 256) as u8)
            .collect();

        let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Unorm);

        // Path A: CPU write into a shared texture (legacy equivalent).
        let (shared_tex, _) = ctx.create_texture_shared(&desc).unwrap();
        let region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize {
                width: width as usize,
                height: height as usize,
                depth: 1,
            },
        };
        unsafe {
            shared_tex
                .inner
                .replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                    region,
                    0,
                    std::ptr::NonNull::new(data.as_ptr() as *mut std::ffi::c_void).unwrap(),
                    width as usize * 4,
                );
        }

        // Path B: private texture + staged blit (current path).
        let (private_tex, _) = ctx.create_texture(&desc).unwrap();
        let mut queue = TextureUploadQueue::default();
        queue
            .stage(
                &ctx,
                private_tex.clone(),
                ImageFormat::R8G8B8A8Unorm,
                width,
                height,
                &data,
            )
            .unwrap();
        let mut cmd = ctx.create_command_buffer();
        cmd.begin();
        {
            let mut blit = cmd.begin_blit_pass();
            queue.encode_into(&mut blit);
            blit.end_encoding();
        }
        cmd.end();
        cmd.submit(&ctx);
        unsafe { cmd.inner.waitUntilCompleted() };

        let (mirror, _) = ctx.create_texture_shared(&desc).unwrap();
        let mut copy_cmd = ctx.create_command_buffer();
        copy_cmd.begin();
        {
            let mut blit = copy_cmd.begin_blit_pass();
            blit.copy_texture_to_texture(&private_tex, &mirror);
            blit.end_encoding();
        }
        copy_cmd.end();
        copy_cmd.submit(&ctx);
        unsafe { copy_cmd.inner.waitUntilCompleted() };

        let read = |tex: &MetalTexture| -> Vec<u8> {
            let mut out = vec![0u8; data.len()];
            unsafe {
                tex.inner.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                    std::ptr::NonNull::new(out.as_mut_ptr() as *mut std::ffi::c_void).unwrap(),
                    width as usize * 4,
                    region,
                    0,
                );
            }
            out
        };
        assert_eq!(read(&shared_tex), data, "replaceRegion content");
        assert_eq!(read(&mirror), data, "blit content");
    }
}
