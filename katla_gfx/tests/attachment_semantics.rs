//! Device tests for declared attachment semantics (#95).
//!
//! The declared load/store/clear ops of each pass are the only source of
//! attachment behavior: a Clear pass replaces the target's contents, and a
//! Load pass extends them. Both graphs below draw the same green UI quad on
//! the left half; they differ only in the first pass's declared load op.

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use katla_gfx::render_graph::{FrameGraphBuilder, GeometryPass, UIPass};
use katla_gfx::render_pass::{AttachmentOps, ClearValue};
use katla_gfx::texture::ImageFormat;
use katla_gfx::vertex::VertexUIInstance;
use katla_gfx::{
    GpuRenderer, PipelineDescriptor, UIDrawList, UiDrawCommand, ValidationMode, VulkanRenderer,
};

#[test]
#[ignore = "requires a Vulkan device"]
fn declared_clear_replaces_and_declared_load_extends_attachments() {
    let mut renderer = VulkanRenderer::init_headless(
        64,
        48,
        ValidationMode::Enabled,
        CString::new("Attachment semantics test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured_errors = errors.clone();
    renderer
        .context()
        .set_validation_callback(move |message, level| {
            if level == katla_gfx::ValidationLevel::Error {
                captured_errors.lock().unwrap().push(message.to_owned());
            }
        });

    // The UI renderer requires a font atlas to be registered.
    let _atlas = renderer
        .create_ui_font_atlas(1, 1, &[255, 0, 0, 255])
        .expect("test font atlas creation");
    let white = renderer
        .create_texture(&katla_gfx::TextureDescriptor::rgba8_unorm(1, 1), &[255; 4])
        .expect("test texture creation");
    let white_slot = renderer.get_bindless_slot(white).unwrap();
    let shaders = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders");
    let material = renderer
        .compile_material(&PipelineDescriptor::ui(
            shaders.join("ui/ui.wgsl").to_string_lossy().into_owned(),
        ))
        .unwrap();
    renderer
        .init_light_culling(64, 48, &shaders.join("lighting/light_cull.wgsl"))
        .unwrap();

    let ui = {
        let mut ui = UIDrawList {
            screen_size: [64.0, 48.0],
            scale_factor: 1.0,
            ..Default::default()
        };
        ui.instances.push(VertexUIInstance {
            position: [4.0, 4.0],
            size: [12.0, 16.0],
            uv_min: [0.0; 2],
            uv_max: [1.0; 2],
            color: [0, 255, 0, 255],
            texture_index: white_slot,
            clip_rect: [0.0, 0.0, 64.0, 48.0],
        });
        ui.commands = vec![UiDrawCommand::instanced(0, 1, None)];
        ui
    };

    let pixel = |pixels: &[u8], x: u32, y: u32| -> [u8; 4] {
        let offset = ((y * 64 + x) * 4) as usize;
        [
            pixels[offset],
            pixels[offset + 1],
            pixels[offset + 2],
            pixels[offset + 3],
        ]
    };

    let red: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    let green: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    for (background, right_expected) in [
        // The UI pass declares Load: the red painted by the first pass must
        // survive wherever the UI did not draw — accumulation across passes.
        (red, [0, 0, 255, 255]),
        // Same graph with a green declared clear: the Clear->Store path
        // replaces the target, and the declared value reaches every pixel
        // the UI did not draw.
        (green, [0, 255, 0, 255]),
    ] {
        let mut graph = FrameGraphBuilder::new()
            .add_pass(GeometryPass::new("background").write_color_ops(
                "backbuffer",
                ImageFormat::B8G8R8A8Srgb,
                AttachmentOps::clear(ClearValue::Color(background)),
            ))
            .add_pass(UIPass::new("ui").write("backbuffer").material(material))
            .build::<VulkanRenderer>()
            .unwrap();
        let ui_pass = graph.pass_id("ui").unwrap();

        for frame in 0..2 {
            renderer.wait_for_frame().unwrap();
            renderer
                .render(&mut graph, |frame| {
                    frame.submit_ui(ui_pass, &ui);
                })
                .unwrap();
            renderer.queue_async_readback(frame).unwrap();
            let (_, pixels) = renderer.wait_for_pending_readback().unwrap().unwrap();

            // The UI quad covers the left probe; the right probe shows what
            // the UI pass's declared Load op preserved from the first pass.
            assert_eq!(pixel(&pixels, 8, 8), [0, 255, 0, 255], "bg={background:?}");
            assert_eq!(pixel(&pixels, 40, 24), right_expected, "bg={background:?}");
            assert_eq!(pixel(&pixels, 8, 40), right_expected, "bg={background:?}");
        }
        graph.cleanup();
        drop(graph);
    }

    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
