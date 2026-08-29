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
            let mut blit = cmd_buffer.begin_blit_pass_with_label("texture_upload");
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
            let mut blit = copy_cmd.begin_blit_pass_with_label("texture_upload");
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
            let mut blit = cmd.begin_blit_pass_with_label("texture_upload");
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
            let mut blit = copy_cmd.begin_blit_pass_with_label("texture_upload");
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

    #[test]
    fn test_storage_mode_sampling_probe() {
        use crate::backend::command::GpuComputeEncoder;

        let ctx = headless_context();
        let width = 4u32;
        let height = 1u32;
        let pixels: Vec<u8> = vec![
            255, 0, 0, 255, 200, 0, 0, 255, 120, 0, 0, 255, 40, 0, 0, 255,
        ];
        let format = ImageFormat::R8G8B8A8Srgb;

        const WGSL: &str = r#"
            @group(0) @binding(0) var src_tex: texture_2d<f32>;
            @group(0) @binding(1) var samp: sampler;
            @group(0) @binding(2) var<storage, read_write> out: array<u32>;
            @compute @workgroup_size(1)
            fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
                let u = (f32(gid.x) + 0.5) / 4.0;
                let px = textureSampleLevel(src_tex, samp, vec2<f32>(u, 0.5), 0.0);
                out[gid.x] = pack4x8unorm(vec4<f32>(px.rgb, 1.0));
            }
        "#;

        let mut results: Vec<Vec<u32>> = Vec::new();
        for private in [false, true] {
            let desc = TextureDescriptor::new(width, height, format);
            let (texture, view) = if private {
                ctx.create_texture(&desc).unwrap()
            } else {
                ctx.create_texture_shared(&desc).unwrap()
            };
            let mut queue = TextureUploadQueue::default();
            queue
                .stage(&ctx, texture.clone(), format, width, height, &pixels)
                .unwrap();
            let mut cmd = ctx.create_command_buffer();
            cmd.begin();
            {
                let mut blit = cmd.begin_blit_pass_with_label("texture_upload");
                queue.encode_into(&mut blit);
                blit.end_encoding();
            }
            cmd.end();
            cmd.submit(&ctx);
            cmd.inner.waitUntilCompleted();
            queue.retire_completed();
            assert!(!queue.has_pending(), "upload must complete");

            let compiled = crate::metal::shader::compile_wgsl_to_metal(
                &ctx.device,
                WGSL,
                &["cs_main"],
                crate::metal::shader::ShaderProfile::Graphics,
            )
            .unwrap();
            let cs = compiled.module.entry_points.get("cs_main").unwrap();
            let pipeline = ctx.create_compute_pipeline(cs, [1, 1, 1]).unwrap();
            let out_buf = ctx.create_buffer(16, true).unwrap();
            let sampler = ctx.create_sampler().unwrap();

            let mut cmd2 = ctx.create_command_buffer();
            cmd2.begin();
            let mut enc = cmd2.begin_compute_pass_with_label("texture_upload");
            enc.bind_compute_pipeline(&pipeline);
            enc.bind_texture(&view, 0);
            enc.bind_sampler(&sampler, 1);
            enc.bind_storage_buffer(&out_buf, 0, 2);
            enc.dispatch(4, 1, 1);
            enc.end_encoding();
            cmd2.end();
            cmd2.submit(&ctx);
            cmd2.inner.waitUntilCompleted();

            let mapped = out_buf.map();
            let data = unsafe { std::slice::from_raw_parts(mapped as *const u32, 4) };
            results.push(data.to_vec());
            out_buf.unmap();
        }
        assert_eq!(
            results[0], results[1],
            "SHARED vs PRIVATE sampled differently: shared={:?} private={:?}",
            results[0], results[1]
        );
    }
    /// Reproduce the geometry pass sampling structure exactly: fragment shader
    /// reads a texture through the bindless argument buffer at index 9 while a
    /// render pass draws into an offscreen attachment. The only variable is the
    /// uploaded texture's storage mode (shared vs private, identical staged-blit
    /// bytes). If the outputs differ, the private-storage render anomaly lives in
    /// the argument-buffer path, not in raw sampling.
    #[test]
    fn test_bindless_argument_buffer_storage_probe() {
        use crate::backend::command::{ColorAttachmentInfo, GpuRenderEncoder, RenderPassInfo};
        use crate::metal::MetalBackend;
        use crate::metal::argument_buffer::MetalBindlessTextureManager;
        use crate::pipeline::CompareOp;
        use crate::render_pass::{ClearValue, LoadOp, StoreOp};
        use crate::texture::TextureDescriptor;
        use objc2_metal::MTLRenderCommandEncoder;

        let ctx = headless_context();
        let width = 4u32;
        let height = 1u32;
        let pixels: Vec<u8> = vec![
            255, 0, 0, 255, 200, 0, 0, 255, 120, 0, 0, 255, 40, 0, 0, 255,
        ];
        let format = ImageFormat::R8G8B8A8Srgb;

        const WGSL: &str = r#"
            @group(1) @binding(0) var bindless_textures: binding_array<texture_2d<f32>, 16>;
            @group(1) @binding(1) var shared_sampler: sampler;

            struct VsOut {
                @builtin(position) pos: vec4f,
            };

            @vertex
            fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
                var p = array<vec2f, 3>(
                    vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0)
                );
                var out: VsOut;
                out.pos = vec4f(p[vi], 0.0, 1.0);
                return out;
            }

            @fragment
            fn fs_main(in: VsOut) -> @location(0) vec4f {
                let uv = vec2f(in.pos.x, in.pos.y);
                return textureSampleLevel(
                    bindless_textures[0], shared_sampler, uv, 0.0
                );
            }
        "#;

        let compiled = crate::metal::shader::compile_wgsl_to_metal(
            &ctx.device,
            WGSL,
            &["vs_main", "fs_main"],
            crate::metal::shader::ShaderProfile::Graphics,
        )
        .unwrap();
        let vs = compiled.module.entry_points.get("vs_main").unwrap();
        let fs = compiled.module.entry_points.get("fs_main").unwrap();
        let pipeline = ctx
            .create_graphics_pipeline(
                vs,
                Some(fs),
                &[objc2_metal::MTLPixelFormat::RGBA8Unorm],
                None,
                false,
                CompareOp::Always,
                objc2_metal::MTLCullMode::None,
                objc2_metal::MTLWinding::Clockwise,
            )
            .unwrap();

        let mut results: Vec<Vec<u8>> = Vec::new();
        for private in [false, true] {
            let desc = TextureDescriptor::new(width, height, format);
            let (texture, view) = if private {
                ctx.create_texture(&desc).unwrap()
            } else {
                ctx.create_texture_shared(&desc).unwrap()
            };
            let mut queue = TextureUploadQueue::default();
            queue
                .stage(&ctx, texture.clone(), format, width, height, &pixels)
                .unwrap();
            let mut cmd = ctx.create_command_buffer();
            cmd.begin();
            {
                let mut blit = cmd.begin_blit_pass_with_label("texture_upload");
                queue.encode_into(&mut blit);
                blit.end_encoding();
            }
            cmd.end();
            cmd.submit(&ctx);
            cmd.inner.waitUntilCompleted();
            queue.retire_completed();
            assert!(!queue.has_pending(), "upload must complete");

            // (Re)build the argument buffer around this texture: the manager
            // writes resource IDs at flush time, so re-registering per mode is
            // the cheapest faithful approach.
            let mut local_manager = MetalBindlessTextureManager::new(16).unwrap();
            let default_desc = TextureDescriptor::new(1, 1, format);
            let (default_texture, _) = ctx.create_texture_shared(&default_desc).unwrap();
            local_manager.set_default_texture(default_texture.inner.as_ref());
            let fs_fn = compiled.module.entry_points.get("fs_main").unwrap();
            local_manager
                .initialize_from_function(fs_fn.as_ref())
                .unwrap();
            let _slot = local_manager.register_texture(view.inner.as_ref());
            local_manager.flush_argument_buffer();
            let arg_buffer = local_manager.argument_buffer().unwrap();

            // Offscreen color target.
            let mut target_desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Unorm);
            target_desc.usage = TextureUsage::COLOR_ATTACHMENT;
            let (target, target_view) = ctx.create_texture_shared(&target_desc).unwrap();

            let sampler = ctx.create_sampler().unwrap();

            let mut cmd2 = ctx.create_command_buffer();
            cmd2.begin();
            {
                let mut enc = cmd2.begin_render_pass(RenderPassInfo::<MetalBackend> {
                    color_attachments: vec![ColorAttachmentInfo::<MetalBackend> {
                        view: target_view,
                        load_op: LoadOp::Clear,
                        store_op: StoreOp::Store,
                        clear_value: ClearValue::Color([0.0, 0.0, 0.0, 1.0]),
                    }],
                    depth_attachment: None,
                    debug_label: Some("texture_upload_test"),
                });
                enc.bind_graphics_pipeline(&pipeline);
                unsafe {
                    enc.inner
                        .setFragmentBuffer_offset_atIndex(Some(arg_buffer), 0, 9);
                    enc.inner
                        .setFragmentSamplerState_atIndex(Some(&sampler.inner), 0);
                }
                enc.use_buffer(
                    arg_buffer,
                    objc2_metal::MTLResourceUsage::Read,
                    objc2_metal::MTLRenderStages::Fragment,
                );
                enc.use_texture(
                    view.inner.as_ref(),
                    objc2_metal::MTLResourceUsage::Read,
                    objc2_metal::MTLRenderStages::Fragment,
                );
                enc.draw(3, 1, 0, 0);
                enc.end_encoding();
            }
            cmd2.end();
            cmd2.submit(&ctx);
            cmd2.inner.waitUntilCompleted();

            // Read the target back.
            let bytes_per_row = width * 4;
            let region = MTLRegion {
                origin: MTLOrigin { x: 0, y: 0, z: 0 },
                size: MTLSize {
                    width: width as usize,
                    height: height as usize,
                    depth: 1,
                },
            };
            let mut out = vec![0u8; (bytes_per_row * height) as usize];
            unsafe {
                target.inner.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                    std::ptr::NonNull::new(out.as_mut_ptr() as *mut std::ffi::c_void).unwrap(),
                    bytes_per_row as usize,
                    region,
                    0,
                );
            }
            results.push(out);
        }

        assert_eq!(
            results[0], results[1],
            "bindless arg-buffer path: SHARED vs PRIVATE rendered differently: \
             shared={:?} private={:?}",
            results[0], results[1]
        );
        let clear_only = [0u8, 0, 0, 255].repeat(4);
        assert_ne!(
            results[0], clear_only,
            "target shows clear color only — probe rendered nothing, result is vacuous"
        );
    }
}
