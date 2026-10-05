use super::*;
use objc2_metal::{MTLOrigin, MTLRegion, MTLResource, MTLSize, MTLTexture};

use crate::backend::command::{GpuBlitEncoder, GpuCommandBuffer};
use crate::backend::resource::GpuBuffer;
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
        queue.encode_into(&mut blit, 1);
        blit.end_encoding();
    }
    cmd_buffer.end();
    queue.mark_submitted(1);
    cmd_buffer.submit(&ctx);
    cmd_buffer.wait_until_completed().unwrap();
    queue.retire_completed(1);

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
    copy_cmd.wait_until_completed().unwrap();

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
        queue.encode_into(&mut blit, 1);
        blit.end_encoding();
    }
    cmd.end();
    queue.mark_submitted(1);
    cmd.submit(&ctx);
    cmd.wait_until_completed().unwrap();

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
    copy_cmd.wait_until_completed().unwrap();

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
            queue.encode_into(&mut blit, 1);
            blit.end_encoding();
        }
        cmd.end();
        queue.mark_submitted(1);
        cmd.submit(&ctx);
        cmd.wait_until_completed().unwrap();
        queue.retire_completed(1);
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
        enc.bind_sampler(&sampler, 0);
        enc.bind_storage_buffer(&out_buf, 0, 0);
        enc.dispatch(4, 1, 1);
        enc.end_encoding();
        cmd2.end();
        cmd2.submit(&ctx);
        cmd2.wait_until_completed().unwrap();

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
    assert_eq!(results[0][0], 0xff0000ff, "probe must sample the red texel");
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
        .create_graphics_pipeline(crate::metal::context::GraphicsPipelineConfig {
            vertex_function: vs,
            fragment_function: Some(fs),
            color_formats: &[objc2_metal::MTLPixelFormat::RGBA8Unorm],
            depth_format: None,
            depth_write_enabled: false,
            depth_compare: CompareOp::Always,
            cull_mode: objc2_metal::MTLCullMode::None,
            front_face: objc2_metal::MTLWinding::Clockwise,
            vertex_descriptor: &objc2_metal::MTLVertexDescriptor::new(),
            alpha_blended: false,
            portable: None,
        })
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
            queue.encode_into(&mut blit, 1);
            blit.end_encoding();
        }
        cmd.end();
        queue.mark_submitted(1);
        cmd.submit(&ctx);
        cmd.wait_until_completed().unwrap();
        queue.retire_completed(1);
        assert!(!queue.has_pending(), "upload must complete");

        // (Re)build the argument buffer around this texture: the manager
        // writes resource IDs at flush time, so re-registering per mode is
        // the cheapest faithful approach.
        let mut local_manager = MetalBindlessTextureManager::new(16).unwrap();
        let default_desc = TextureDescriptor::new(1, 1, format);
        let (default_texture, _) = ctx.create_texture_shared(&default_desc).unwrap();
        local_manager.set_default_texture(default_texture.inner.as_ref());
        local_manager.initialize(&ctx.device).unwrap();
        let _slot = local_manager.register_texture(view.inner.as_ref());
        local_manager.publish_snapshot().unwrap();
        let snapshot = local_manager.snapshot().unwrap();

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
            enc.bind_bindless(snapshot.clone());
            enc.bind_native_sampler(
                &sampler.inner,
                0,
                crate::backend::command::ShaderStages::FRAGMENT,
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
        cmd2.wait_until_completed().unwrap();

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
fn submit_uploads(ctx: &MetalContext, queue: &mut TextureUploadQueue, id: u64) {
    let mut cmd = ctx.create_command_buffer();
    cmd.begin();
    let mut encoder = cmd.begin_blit_pass_with_label("texture_upload_test");
    queue.encode_into(&mut encoder, id);
    encoder.end_encoding();
    cmd.end();
    queue.mark_submitted(id);
    cmd.submit(ctx);
    cmd.wait_until_completed().unwrap();
    queue.retire_completed(id);
}

fn read_subresource(
    ctx: &MetalContext,
    texture: &MetalTexture,
    mip: u32,
    layer: u32,
    extent: [u32; 3],
) -> Vec<u8> {
    use objc2_metal::{MTL4ComputeCommandEncoder, MTLBuffer};
    let block = texture.descriptor().format.block_extent();
    let row_bytes = extent[0].div_ceil(block[0]) as usize
        * texture.descriptor().format.bytes_per_block() as usize;
    let rows = extent[1].div_ceil(block[1]) as usize;
    let pitch = row_bytes.div_ceil(256) * 256;
    let image_pitch = pitch * rows;
    let output = ctx
        .create_buffer((image_pitch * extent[2] as usize) as u64, true)
        .unwrap();
    let mut cmd = ctx.create_command_buffer();
    cmd.resources.residency.add_buffer(&output.inner).unwrap();
    cmd.resources.residency.add_texture(&texture.inner).unwrap();
    cmd.begin();
    let encoder = cmd.begin_blit_pass_with_label("texture_upload_readback");
    use objc2_metal::MTL4CommandEncoder;
    encoder
        .inner
        .barrierAfterQueueStages_beforeStages_visibilityOptions(
            objc2_metal::MTLStages::All,
            objc2_metal::MTLStages::Blit,
            objc2_metal::MTL4VisibilityOptions::Device,
        );
    unsafe {
        encoder.inner.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
        &texture.inner,layer as usize,mip as usize,MTLOrigin{x:0,y:0,z:0},MTLSize{width:extent[0] as usize,height:extent[1] as usize,depth:extent[2] as usize},
        &output.inner,0,pitch,image_pitch);
    }
    encoder.end_encoding();
    cmd.end();
    cmd.submit(ctx);
    cmd.completion.result("texture_upload_readback").unwrap();
    let mut packed = Vec::new();
    for z in 0..extent[2] as usize {
        for row in 0..rows {
            let ptr = unsafe { output.map().add(z * image_pitch + row * pitch) };
            packed.extend_from_slice(unsafe { std::slice::from_raw_parts(ptr, row_bytes) });
        }
    }
    output.unmap();
    assert_eq!(output.inner.length(), image_pitch * extent[2] as usize);
    packed
}

#[test]
fn test_staged_formats_non_tight_and_partial_regions() {
    let ctx = headless_context();
    for format in [
        ImageFormat::R8G8B8A8Unorm,
        ImageFormat::B8G8R8A8Srgb,
        ImageFormat::R8Unorm,
        ImageFormat::Rg8Unorm,
        ImageFormat::R32Sfloat,
        ImageFormat::R32Uint,
        ImageFormat::R16G16B16A16Sfloat,
    ] {
        let desc = TextureDescriptor::new(7, 4, format);
        let bytes = format.bytes_per_block() as usize;
        let initial = vec![0; 7 * 4 * bytes];
        let (texture, _) = ctx.create_texture(&desc).unwrap();
        assert_eq!(
            texture.inner.storageMode(),
            objc2_metal::MTLStorageMode::Private
        );
        let mut queue = TextureUploadQueue::default();
        queue
            .stage_region(
                &ctx,
                texture.clone(),
                &desc,
                TextureUploadRegion::base(&desc),
                &initial,
            )
            .unwrap();
        submit_uploads(&ctx, &mut queue, 1);
        let region = TextureUploadRegion {
            mip_level: 0,
            array_layer: 0,
            origin: [2, 1, 0],
            extent: [3, 2, 1],
            bytes_per_row: 8 * bytes,
            bytes_per_image: 0,
        };
        let mut data = vec![0xEE; 11 * bytes];
        data[..3 * bytes].fill(0x11);
        data[8 * bytes..11 * bytes].fill(0x22);
        queue
            .stage_region(&ctx, texture.clone(), &desc, region, &data)
            .unwrap();
        let overlapping = TextureUploadRegion {
            origin: [3, 2, 0],
            extent: [2, 1, 1],
            bytes_per_row: 0,
            ..region
        };
        queue
            .stage_region(
                &ctx,
                texture.clone(),
                &desc,
                overlapping,
                &vec![0x42; 2 * bytes],
            )
            .unwrap();
        submit_uploads(&ctx, &mut queue, 2);
        let actual = read_subresource(&ctx, &texture, 0, 0, [7, 4, 1]);
        let mut expected = initial;
        expected[(7 + 2) * bytes..(7 + 5) * bytes].fill(0x11);
        expected[(14 + 2) * bytes..(14 + 5) * bytes].fill(0x22);
        expected[(14 + 3) * bytes..(14 + 5) * bytes].fill(0x42);
        assert_eq!(actual, expected, "{format:?} partial padded rows");
    }
}

#[test]
fn test_staged_mip_array_layers_and_3d_slices() {
    let ctx = headless_context();
    let mut desc = TextureDescriptor::new(8, 4, ImageFormat::R8Unorm);
    desc.array_layers = 3;
    desc.mip_levels = 4;
    let (texture, _) = ctx.create_texture(&desc).unwrap();
    let mut queue = TextureUploadQueue::default();
    for layer in 0..3 {
        for mip in 0..4 {
            let extent = [(8 >> mip).max(1), (4 >> mip).max(1), 1];
            let bytes = vec![(layer * 10 + mip + 1) as u8; (extent[0] * extent[1]) as usize];
            let region = TextureUploadRegion {
                mip_level: mip,
                array_layer: layer,
                origin: [0; 3],
                extent,
                bytes_per_row: 0,
                bytes_per_image: 0,
            };
            queue
                .stage_region(&ctx, texture.clone(), &desc, region, &bytes)
                .unwrap();
        }
    }
    submit_uploads(&ctx, &mut queue, 1);
    for layer in 0..3 {
        for mip in 0..4 {
            let extent = [(8 >> mip).max(1), (4 >> mip).max(1), 1];
            assert_eq!(
                read_subresource(&ctx, &texture, mip, layer, extent),
                vec![(layer * 10 + mip + 1) as u8; (extent[0] * extent[1]) as usize]
            );
        }
    }
    let mut desc = TextureDescriptor::new(3, 2, ImageFormat::R8Unorm);
    desc.depth = 3;
    let (texture, _) = ctx.create_texture(&desc).unwrap();
    let region = TextureUploadRegion {
        bytes_per_row: 8,
        bytes_per_image: 32,
        ..TextureUploadRegion::base(&desc)
    };
    let mut data = vec![0xEE; 75];
    let mut expected = Vec::new();
    for z in 0..3 {
        for row in 0..2 {
            let value = (z * 10 + row + 1) as u8;
            data[z * 32 + row * 8..z * 32 + row * 8 + 3].fill(value);
            expected.extend([value; 3]);
        }
    }
    queue
        .stage_region(&ctx, texture.clone(), &desc, region, &data)
        .unwrap();
    submit_uploads(&ctx, &mut queue, 2);
    assert_eq!(read_subresource(&ctx, &texture, 0, 0, [3, 2, 3]), expected);
}

#[test]
fn test_staged_generated_mip_chain() {
    let ctx = headless_context();
    let mut desc = TextureDescriptor::new(8, 8, ImageFormat::R8G8B8A8Unorm);
    desc.mip_levels = 4;
    desc.generate_mips = true;
    let pixels = [17, 34, 51, 255].repeat(64);
    let (texture, _) = ctx.create_texture(&desc).unwrap();
    let mut queue = TextureUploadQueue::default();
    queue
        .stage_region(
            &ctx,
            texture.clone(),
            &desc,
            TextureUploadRegion::base(&desc),
            &pixels,
        )
        .unwrap();
    submit_uploads(&ctx, &mut queue, 1);
    for mip in 0..4 {
        let side = 8 >> mip;
        assert_eq!(
            read_subresource(&ctx, &texture, mip, 0, [side, side, 1]),
            [17, 34, 51, 255].repeat((side * side) as usize)
        );
    }
}

#[test]
fn test_staged_compressed_blocks_or_typed_capability_rejection() {
    use objc2_metal::MTLDevice;
    let ctx = headless_context();
    for format in [ImageFormat::Bc1RgbaUnorm, ImageFormat::Bc3RgbaUnorm] {
        let desc = TextureDescriptor::new(8, 8, format);
        let result = ctx.create_texture(&desc);
        if !ctx.device.supportsBCTextureCompression() {
            assert!(matches!(result, Err(RendererError::UnsupportedFeature(_))));
            continue;
        }
        let (texture, _) = result.unwrap();
        let mut queue = TextureUploadQueue::default();
        let data = vec![0x3A; 4 * format.bytes_per_block() as usize];
        queue
            .stage_region(
                &ctx,
                texture.clone(),
                &desc,
                TextureUploadRegion::base(&desc),
                &data,
            )
            .unwrap();
        submit_uploads(&ctx, &mut queue, 1);
        assert_eq!(read_subresource(&ctx, &texture, 0, 0, [8, 8, 1]), data);
    }
}

#[test]
fn test_staging_ownership_retirement_and_budgets() {
    use objc2_metal::MTLBuffer;
    let ctx = headless_context();
    let desc = TextureDescriptor::new(1, 1, ImageFormat::R8Unorm);
    let (texture, _) = ctx.create_texture(&desc).unwrap();
    let mut queue = TextureUploadQueue {
        budget: TextureUploadBudget {
            max_queued_bytes: 256,
            max_bytes_per_submission: 256,
            max_staging_bytes: 512,
            max_uploads_per_submission: 1,
        },
        ..Default::default()
    };
    queue
        .stage_region(
            &ctx,
            texture.clone(),
            &desc,
            TextureUploadRegion::base(&desc),
            &[7],
        )
        .unwrap();
    let first_address = queue.pending[0].staging.inner.gpuAddress();
    assert!(
        queue
            .stage_region(
                &ctx,
                texture.clone(),
                &desc,
                TextureUploadRegion::base(&desc),
                &[8]
            )
            .is_err()
    );
    let mut first = ctx.create_command_buffer();
    first.begin();
    let mut encoder = first.begin_blit_pass_with_label("ownership.first");
    queue.encode_into(&mut encoder, 10);
    encoder.end_encoding();
    first.end();
    queue.mark_submitted(10);
    first.submit(&ctx);
    queue.retire_completed(9);
    assert!(queue.free.is_empty());
    queue
        .stage_region(
            &ctx,
            texture.clone(),
            &desc,
            TextureUploadRegion::base(&desc),
            &[8],
        )
        .unwrap();
    assert_ne!(first_address, queue.pending[0].staging.inner.gpuAddress());
    assert_eq!(unsafe { *queue.in_flight[0].uploads[0].staging.map() }, 7);
    first.completion.result("ownership.first").unwrap();
    queue.retire_completed(10);
    assert_eq!(queue.free.len(), 1);
    assert_eq!(queue.metrics().staging_high_water_mark, 512);
    submit_uploads(&ctx, &mut queue, 11);
    assert_eq!(read_subresource(&ctx, &texture, 0, 0, [1, 1, 1]), [8]);
    assert_eq!(queue.metrics().submitted_bytes, 512);
    assert_eq!(queue.metrics().failure_count, 1);
}

#[test]
fn test_unsubmitted_upload_abort_replays_without_early_reuse() {
    let ctx = headless_context();
    let desc = TextureDescriptor::new(1, 1, ImageFormat::R8Unorm);
    let (texture, _) = ctx.create_texture(&desc).unwrap();
    let mut queue = TextureUploadQueue {
        budget: TextureUploadBudget {
            max_queued_bytes: 512,
            max_bytes_per_submission: 512,
            max_staging_bytes: 512,
            max_uploads_per_submission: 1,
        },
        ..Default::default()
    };
    queue
        .stage_region(
            &ctx,
            texture.clone(),
            &desc,
            TextureUploadRegion::base(&desc),
            &[13],
        )
        .unwrap();
    let mut abandoned = ctx.create_command_buffer();
    abandoned.begin();
    let mut encoder = abandoned.begin_blit_pass_with_label("aborted.upload");
    queue.encode_into(&mut encoder, 10);
    encoder.end_encoding();
    abandoned.end();
    queue.retire_completed(10);
    assert!(queue.free.is_empty());
    assert!(!queue.has_pending());
    assert_eq!(queue.metrics().queued_bytes, 256);
    // The encoded upload still occupies its admission slot until committed or replayed.
    assert!(
        queue
            .stage_region(
                &ctx,
                texture.clone(),
                &desc,
                TextureUploadRegion::base(&desc),
                &[19]
            )
            .is_err()
    );
    drop(abandoned);
    queue.cancel_unsubmitted(10);
    assert!(queue.has_pending());
    assert!(queue.in_flight.is_empty());
    submit_uploads(&ctx, &mut queue, 11);
    assert_eq!(read_subresource(&ctx, &texture, 0, 0, [1, 1, 1]), [13]);
    assert_eq!(queue.metrics().submitted_bytes, 256);
    assert_eq!(queue.metrics().queued_bytes, 0);
}

#[test]
#[ignore = "physical-device upload throughput benchmark; run explicitly with --nocapture"]
fn test_texture_upload_benchmark() {
    let ctx = headless_context();
    for (name, count, width, height) in
        [("many_small", 512, 32, 32), ("large_stream", 4, 2048, 2048)]
    {
        let desc = TextureDescriptor::new(width, height, ImageFormat::R8G8B8A8Unorm);
        let data = vec![127; (width * height * 4) as usize];
        for private in [false, true] {
            let mut times = Vec::new();
            let mut queue = TextureUploadQueue::default();
            for iteration in 0..6 {
                let start = Instant::now();
                let mut textures = Vec::new();
                for _ in 0..count {
                    let (texture, _) = if private {
                        ctx.create_texture(&desc).unwrap()
                    } else {
                        ctx.create_texture_shared(&desc).unwrap()
                    };
                    if private {
                        queue
                            .stage_region(
                                &ctx,
                                texture.clone(),
                                &desc,
                                TextureUploadRegion::base(&desc),
                                &data,
                            )
                            .unwrap();
                    } else {
                        unsafe {
                            texture
                                .inner
                                .replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                                    MTLRegion {
                                        origin: MTLOrigin { x: 0, y: 0, z: 0 },
                                        size: MTLSize {
                                            width: width as usize,
                                            height: height as usize,
                                            depth: 1,
                                        },
                                    },
                                    0,
                                    std::ptr::NonNull::new(data.as_ptr() as *mut std::ffi::c_void)
                                        .unwrap(),
                                    width as usize * 4,
                                );
                        }
                    }
                    textures.push(texture);
                }
                let cpu_ns = start.elapsed().as_nanos();
                if private {
                    submit_uploads(&ctx, &mut queue, iteration + 1);
                }
                let ready_ns = start.elapsed().as_nanos();
                // Every trial proves uploaded content through an actual GPU copy/readback.
                assert!(
                    read_subresource(&ctx, textures.last().unwrap(), 0, 0, [width, height, 1])
                        .iter()
                        .all(|byte| *byte == 127)
                );
                if iteration > 0 {
                    times.push((cpu_ns, ready_ns));
                }
            }
            let mut cpu_times: Vec<_> = times.iter().map(|sample| sample.0).collect();
            let mut ready_times: Vec<_> = times.iter().map(|sample| sample.1).collect();
            cpu_times.sort_unstable();
            ready_times.sort_unstable();
            let median = (cpu_times[2], ready_times[2]);
            eprintln!(
                "UPLOAD_BENCH name={name} mode={} textures={count} payload_bytes={} cpu_ns={} ready_ns={} staging_high_water={}",
                if private {
                    "staged_private"
                } else {
                    "synchronous_shared"
                },
                count as usize * data.len(),
                median.0,
                median.1,
                queue.metrics().staging_high_water_mark
            );
        }
    }
}
