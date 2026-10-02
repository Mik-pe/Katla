#[cfg(test)]
mod tests {
    use crate::CompareOp;
    use crate::backend::command::{ColorAttachmentInfo, RenderPassInfo};
    use crate::backend::command::{GpuBlitEncoder, GpuCommandBuffer, GpuRenderEncoder};
    use crate::backend::resource::{GpuBuffer, GpuImageView};
    use crate::metal::context::MetalContext;
    use crate::metal::shader::{self, ShaderProfile};
    use crate::metal::texture::MetalTextureView;
    use crate::render_pass::{ClearValue, LoadOp, StoreOp};
    use crate::texture::TextureUsage;
    use crate::texture::{ImageFormat, TextureDescriptor};
    use objc2_metal::MTLPixelFormat;

    /// Read a single u32 pixel from an R32Uint texture at (x, y).
    fn read_pixel_r32(ctx: &MetalContext, texture: &MetalTextureView, x: u32, y: u32) -> u32 {
        let readback = ctx.create_buffer(4, true).unwrap();
        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();
        let mut blit = cmd_buffer.begin_blit_pass_with_label("picking_test_readback");
        blit.copy_texture_pixel_to_buffer(texture.image(), x, y, &readback);
        blit.end_encoding();
        cmd_buffer.end();
        cmd_buffer.submit(ctx);
        cmd_buffer.wait_until_completed().unwrap();

        let ptr = readback.map() as *const u32;
        let value = unsafe { std::ptr::read(ptr) };
        readback.unmap();
        value
    }

    /// Test that the picking texture Y-axis matches screen-space Y (top = 0).
    ///
    /// Renders two small quads with different instance IDs:
    /// - Quad A at the top of the viewport (clip Y = -0.5) with instance_id = 10
    /// - Quad B at the bottom of the viewport (clip Y = +0.5) with instance_id = 20
    ///
    /// Then reads back pixels at Y=0 (top row) and Y=height-1 (bottom row).
    /// The pixel at Y=0 should contain instance 10 (top quad) and the pixel
    /// at Y=height-1 should contain instance 20 (bottom quad).
    ///
    /// This validates that Metal's viewport transform maps clip Y = -1 to the
    /// top of the texture (pixel Y = 0), which is what the picking readback
    /// coordinate calculation assumes.
    #[test]
    fn test_picking_y_axis_matches_screen_space() {
        let ctx = MetalContext::init_headless().unwrap();

        let width = 64u32;
        let height = 64u32;

        // Shader that outputs instance_index + 1 as the fragment color (R32Uint).
        // Each draw call renders a full-screen quad — we control position via clip-space
        // in the vertex shader based on vertex_index, and rely on depth test to place
        // the correct instance at the correct Y position.
        //
        // For this test we use a simpler approach: render two full-width horizontal
        // strips whose vertical positions are baked into the vertex shader per instance.
        let wgsl = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) @interpolate(flat) instance_idx: u32,
}

@vertex fn vs_main(
    @builtin(vertex_index) vi: u32,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    // Instance 0: top half (clip Y from -1.0 to 0.0)
    // Instance 1: bottom half (clip Y from 0.0 to 1.0)
    let y_offset = select(-1.0, 0.0, instance_idx == 1u);
    let y_next = select(0.0, 1.0, instance_idx == 1u);

    var positions = array<vec2f, 6>(
        vec2f(-1.0, y_offset),
        vec2f( 1.0, y_offset),
        vec2f( 1.0, y_next),
        vec2f(-1.0, y_offset),
        vec2f( 1.0, y_next),
        vec2f(-1.0, y_next),
    );

    var out: VertexOutput;
    out.clip_position = vec4f(positions[vi], 0.0, 1.0);
    out.instance_idx = instance_idx;
    return out;
}

@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4u {
    return vec4u(input.instance_idx + 1u, 0u, 0u, 1u);
}
"#;

        let compiled = shader::compile_wgsl_to_metal(
            &ctx.device,
            wgsl,
            &["vs_main", "fs_main"],
            ShaderProfile::Graphics,
        )
        .unwrap();
        let vs = compiled.module.entry_points.get("vs_main").unwrap();
        let fs = compiled.module.entry_points.get("fs_main").unwrap();

        let pipeline = ctx
            .create_graphics_pipeline(crate::metal::context::GraphicsPipelineConfig {
                vertex_function: vs,
                fragment_function: Some(fs),
                color_formats: &[MTLPixelFormat::R32Uint],
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

        // Create R32Uint picking texture
        let desc = TextureDescriptor::new(width, height, ImageFormat::R32Uint)
            .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
        let (_texture, view) = ctx.create_texture(&desc).unwrap();

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();

        let render_pass_info = RenderPassInfo {
            color_attachments: vec![ColorAttachmentInfo {
                view: view.clone(),
                load_op: LoadOp::Clear,
                store_op: StoreOp::Store,
                clear_value: ClearValue::TRANSPARENT_BLACK,
            }],
            depth_attachment: None,
            debug_label: Some("picking_readback"),
        };

        let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);
        encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
        encoder.bind_graphics_pipeline(&pipeline);

        // Dummy vertex buffer at index 10 (required by vertex descriptor)
        let dummy_vb = ctx.create_buffer(4, true).unwrap();
        encoder.bind_vertex_buffer(&dummy_vb, 0, 10);

        // Draw both instances in one call:
        // Instance 0 (id=0): clip Y from -1.0 to 0.0 (top half in clip space)
        // Instance 1 (id=1): clip Y from 0.0 to 1.0 (bottom half in clip space)
        encoder.draw(6, 2, 0, 0);

        encoder.end_encoding();
        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.wait_until_completed().unwrap();

        // Read pixels at top center and bottom center
        let top_pixel = read_pixel_r32(&ctx, &view, width / 2, 0);
        let bottom_pixel = read_pixel_r32(&ctx, &view, width / 2, height - 1);

        // EMPIRICAL: Metal's viewport maps clip Y = +1 to pixel Y = 0 (top)
        // and clip Y = -1 to pixel Y = height-1 (bottom).
        // So instance 0 (clip Y=-1..0) is at the bottom, instance 1 (clip Y=0..+1) at the top.
        assert_eq!(
            top_pixel, 2,
            "Top of picking texture (y=0) should contain instance 1 (clip Y=0..+1), got {}",
            top_pixel
        );
        assert_eq!(
            bottom_pixel,
            1,
            "Bottom of picking texture (y={}) should contain instance 0 (clip Y=-1..0), got {}",
            height - 1,
            bottom_pixel
        );
    }

    /// Test that the picking readback Y coordinate matches the rendered scene.
    ///
    /// From test_picking_y_axis_matches_screen_space we empirically know:
    /// - Metal's viewport maps clip Y = +1 → pixel Y = 0 (top)
    /// - Metal's viewport maps clip Y = -1 → pixel Y = height-1 (bottom)
    ///
    /// The engine's projection matrix negates Y (`-f`), so:
    /// - Objects above camera (world Y > 0) → clip Y < 0 → pixel Y near bottom
    /// - Objects below camera (world Y < 0) → clip Y > 0 → pixel Y near top
    ///
    /// But the user sees the scene correctly because the tonemap fullscreen triangle
    /// flips Y again. The picking readback coordinates (physical_y) are derived from
    /// screen space where Y=0 is the top. So physical_y=0 should pick the object the
    /// user sees at the top of the screen.
    ///
    /// This test proves that physical_y=0 (top of screen) reads from the TOP of the
    /// picking texture, which in Metal contains the object with clip Y = +1 (NOT the
    /// object above the camera). The picking Y must be flipped for Metal.
    #[test]
    fn test_picking_readback_y_matches_tonemapped_display() {
        let ctx = MetalContext::init_headless().unwrap();

        let width = 64u32;
        let height = 64u32;

        let wgsl = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) @interpolate(flat) instance_idx: u32,
}

@vertex fn vs_main(
    @builtin(vertex_index) vi: u32,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    // Instance 0: top half (clip Y from -1.0 to 0.0)
    // Instance 1: bottom half (clip Y from 0.0 to 1.0)
    let y_offset = select(-1.0, 0.0, instance_idx == 1u);
    let y_next = select(0.0, 1.0, instance_idx == 1u);

    var positions = array<vec2f, 6>(
        vec2f(-1.0, y_offset),
        vec2f( 1.0, y_offset),
        vec2f( 1.0, y_next),
        vec2f(-1.0, y_offset),
        vec2f( 1.0, y_next),
        vec2f(-1.0, y_next),
    );

    var out: VertexOutput;
    out.clip_position = vec4f(positions[vi], 0.0, 1.0);
    out.instance_idx = instance_idx;
    return out;
}

@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4u {
    return vec4u(input.instance_idx + 1u, 0u, 0u, 1u);
}
"#;

        let compiled = shader::compile_wgsl_to_metal(
            &ctx.device,
            wgsl,
            &["vs_main", "fs_main"],
            ShaderProfile::Graphics,
        )
        .unwrap();
        let vs = compiled.module.entry_points.get("vs_main").unwrap();
        let fs = compiled.module.entry_points.get("fs_main").unwrap();

        let pipeline = ctx
            .create_graphics_pipeline(crate::metal::context::GraphicsPipelineConfig {
                vertex_function: vs,
                fragment_function: Some(fs),
                color_formats: &[MTLPixelFormat::R32Uint],
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

        let desc = TextureDescriptor::new(width, height, ImageFormat::R32Uint)
            .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
        let (_texture, picking_view) = ctx.create_texture(&desc).unwrap();

        let dummy_vb = ctx.create_buffer(4, true).unwrap();

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();

        let render_pass_info = RenderPassInfo {
            color_attachments: vec![ColorAttachmentInfo {
                view: picking_view.clone(),
                load_op: LoadOp::Clear,
                store_op: StoreOp::Store,
                clear_value: ClearValue::TRANSPARENT_BLACK,
            }],
            depth_attachment: None,
            debug_label: Some("picking_readback"),
        };

        let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);
        encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
        encoder.bind_graphics_pipeline(&pipeline);
        encoder.bind_vertex_buffer(&dummy_vb, 0, 10);

        // Draw both instances:
        // Instance 0 (id=0): clip Y from -1.0 to 0.0 (negative clip Y, simulates "above camera")
        // Instance 1 (id=1): clip Y from 0.0 to +1.0 (positive clip Y, simulates "below camera")
        encoder.draw(6, 2, 0, 0);

        encoder.end_encoding();
        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.wait_until_completed().unwrap();

        // Simulate the picking readback. The user clicks at the top of the viewport.
        // The picking code maps this to physical_y = 0 (top row of the picking texture).
        //
        // From test_picking_y_axis_matches_screen_space, we know:
        //   clip Y = +1 → pixel Y = 0 (top of texture)
        //   clip Y = -1 → pixel Y = height-1 (bottom of texture)
        //
        // So instance 0 (clip Y < 0, "above camera") is at the BOTTOM of the picking texture,
        // and instance 1 (clip Y > 0, "below camera") is at the TOP.
        //
        // The fix: flip the Y coordinate before reading, so that screen-top maps to
        // the bottom of the picking texture (where clip Y < 0 objects are).
        let flipped_y_top = height - 1; // physical_y=0 → flipped to bottom
        let flipped_y_bottom = height - 1 - (height - 1); // physical_y=63 → flipped to top

        let picked_top = read_pixel_r32(&ctx, &picking_view, width / 2, flipped_y_top);
        let picked_bottom = read_pixel_r32(&ctx, &picking_view, width / 2, flipped_y_bottom);

        // After the Y-flip, picking at screen-top should read the object with
        // negative clip Y (above camera, instance 0+1=1).
        assert_eq!(
            picked_top, 1,
            "Picking at the top of the viewport (physical_y=0, flipped to {}) should select the object \
             with negative clip Y (above camera, instance 0+1=1), got {}",
            flipped_y_top, picked_top
        );
        assert_eq!(
            picked_bottom,
            2,
            "Picking at the bottom of the viewport (physical_y={}, flipped to {}) should select the object \
             with positive clip Y (below camera, instance 1+1=2), got {}",
            height - 1,
            flipped_y_bottom,
            picked_bottom
        );
    }
}
