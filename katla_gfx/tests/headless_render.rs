//! Full Vulkan frame submission and readback without a presentation surface.

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use katla_gfx::render_graph::{FrameGraphBuilder, GeometryPass, UIPass};
use katla_gfx::texture::ImageFormat;
use katla_gfx::vertex::{VertexUI, VertexUIInstance};
use katla_gfx::{
    MaterialOptions, UIDrawList, UiDrawCommand, ValidationMode, VertexType, VulkanRenderer,
};

#[test]
#[ignore = "requires a Vulkan device"]
fn test_headless_render_and_readback_across_frame_slots() {
    let mut renderer = VulkanRenderer::init_headless(
        64,
        48,
        ValidationMode::Enabled,
        CString::new("Headless render test").unwrap(),
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
    let mut graph = FrameGraphBuilder::new()
        .add_pass(GeometryPass::new("clear").write_color("backbuffer", ImageFormat::B8G8R8A8Srgb))
        .build::<VulkanRenderer>()
        .unwrap();

    let mut previous = None;
    for frame in 0..5 {
        renderer.wait_for_frame().unwrap();
        renderer.wait_for_frame().unwrap();
        renderer.render(&mut graph, |_| {}).unwrap();
        renderer.queue_async_readback(frame).unwrap();
        let (captured_frame, pixels) = renderer.wait_for_pending_readback().unwrap().unwrap();
        assert_eq!(captured_frame, frame);
        assert_eq!(pixels.len(), 64 * 48 * 4);
        assert!(pixels.chunks_exact(4).all(|p| p[0] > 0 && p[3] == 255));
        if let Some(previous) = &previous {
            assert_eq!(&pixels, previous);
        }
        previous = Some(pixels);
    }
    graph.cleanup();
    drop(graph);
    let atlas = renderer.create_ui_font_atlas(1, 1, &[255, 0, 0, 255]);
    let white =
        renderer.create_texture(&katla_gfx::TextureDescriptor::rgba8_unorm(1, 1), &[255; 4]);
    let red_slot = renderer.get_bindless_slot(atlas).unwrap();
    let white_slot = renderer.get_bindless_slot(white).unwrap();
    let shaders = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders");
    let material = renderer
        .compile_material(
            shaders.join("ui/ui.wgsl"),
            MaterialOptions {
                vertex_type: VertexType::Ui,
                color_format: ImageFormat::B8G8R8A8Srgb,
                depth_test: false,
                alpha_blended: true,
                double_sided: true,
                ..Default::default()
            },
        )
        .unwrap();
    renderer
        .init_light_culling(64, 48, &shaders.join("lighting/light_cull.wgsl"))
        .unwrap();
    let mut graph = FrameGraphBuilder::new()
        .add_pass(GeometryPass::new("clear").write_color("backbuffer", ImageFormat::B8G8R8A8Srgb))
        .add_pass(UIPass::new("ui").write("backbuffer").material(material))
        .build::<VulkanRenderer>()
        .unwrap();
    let ui_pass = graph.pass_id("ui").unwrap();
    let mut ui = UIDrawList {
        screen_size: [64.0, 48.0],
        scale_factor: 1.0,
        ..Default::default()
    };
    for (x, color) in [(4.0, [0, 255, 0, 255]), (36.0, [0, 0, 255, 255])] {
        ui.instances.push(VertexUIInstance {
            position: [x, 4.0],
            size: [12.0, 16.0],
            uv_min: [0.0; 2],
            uv_max: [1.0; 2],
            color,
            texture_index: white_slot,
            clip_rect: [0.0, 0.0, 64.0, 48.0],
        });
    }
    for (x, slot) in [(20.0, red_slot), (52.0, white_slot)] {
        let base = ui.vertices.len() as u32;
        for position in [[x, 4.0], [x + 12.0, 4.0], [x + 12.0, 20.0], [x, 20.0]] {
            ui.vertices
                .push(VertexUI::new(position, [0.5; 2], [255; 4], slot));
        }
        ui.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    ui.commands = vec![
        UiDrawCommand::instanced(0, 1, None, white),
        UiDrawCommand::vertex(0, 6, None, atlas),
        UiDrawCommand::instanced(1, 1, Some([36.0, 4.0, 8.0, 16.0]), white),
        UiDrawCommand::vertex(6, 6, None, white),
    ];
    for frame in 0..4 {
        if frame == 2 {
            // Resizing lighting invalidates material layouts, including both UI pipelines.
            renderer.resize_light_culling(32, 32);
        }
        renderer
            .render(&mut graph, |frame| {
                frame.submit_ui(ui_pass, &ui);
            })
            .unwrap();
        renderer.queue_async_readback(frame).unwrap();
        let (_, pixels) = renderer.wait_for_pending_readback().unwrap().unwrap();
        for (x, bgra) in [
            (8, [0, 255, 0, 255]),
            (24, [0, 0, 255, 255]),
            (40, [255, 0, 0, 255]),
            (56, [255; 4]),
        ] {
            let offset = (8 * 64 + x) * 4;
            assert_eq!(&pixels[offset..offset + 4], &bgra, "frame {frame}, x={x}");
        }
        let clipped = (8 * 64 + 46) * 4;
        let background = (24 * 64 + 46) * 4;
        assert_eq!(
            &pixels[clipped..clipped + 4],
            &pixels[background..background + 4]
        );
    }
    graph.cleanup();
    drop(graph);
    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn test_headless_rejects_empty_extent() {
    for (width, height) in [(0, 48), (64, 0)] {
        let result = VulkanRenderer::init_headless(
            width,
            height,
            ValidationMode::Disabled,
            CString::new("Headless render test").unwrap(),
            CString::new("Katla").unwrap(),
        );
        assert!(result.is_err());
    }
}
