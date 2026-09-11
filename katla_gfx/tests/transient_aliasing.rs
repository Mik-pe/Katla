//! Transient allocation aliasing contract tests for issue #35.
//!
//! Compatible, non-overlapping transient textures share one physical
//! allocation per frame slot. With aliasing on, the later member's clear
//! overwrites the shared storage, so the earlier member's readback shows
//! the later member's color; with aliasing off, every member keeps its own
//! storage and color. Graph semantics — pass order and the observable
//! content of the live resource — are identical in both modes.
//!
//! Device tests need a Vulkan device (`#[ignore]`, run like the other GPU
//! contract suites:
//! `TMPDIR=$HOME/tmp cargo test -p katla_gfx --test transient_aliasing -- --ignored`).

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use katla_gfx::render_graph::{FrameGraph, FrameGraphBuilder, GeometryPass, GraphResourceDesc};
use katla_gfx::render_pass::{AttachmentOps, ClearValue};
use katla_gfx::texture::ImageFormat;
use katla_gfx::{ValidationMode, VulkanRenderer};

const BLUE: [u8; 4] = [255, 0, 0, 255];
const RED: [u8; 4] = [0, 0, 255, 255];

fn headless_renderer(label: &str) -> (VulkanRenderer, Arc<Mutex<Vec<String>>>) {
    let renderer = VulkanRenderer::init_headless(
        64,
        64,
        ValidationMode::Enabled,
        CString::new(label).unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured = errors.clone();
    renderer
        .context()
        .set_validation_callback(move |message, level| {
            if level == katla_gfx::ValidationLevel::Error {
                captured.lock().unwrap().push(message.to_owned());
            }
        });
    (renderer, errors)
}

fn transient_desc(name: &str) -> GraphResourceDesc {
    GraphResourceDesc {
        name: name.to_string(),
        resource_type: katla_gfx::render_graph::GraphResourceType::ColorAttachment {
            clear_value: None,
        },
        format: ImageFormat::B8G8R8A8Srgb,
        width: 64,
        height: 64,
        tracks_swapchain_size: false,
    }
}

/// `fill_a` clears mid_a to target-order BGRA red, then `fill_b` clears
/// mid_b to blue. The live intervals are disjoint, so the compiled plan
/// aliases the two into one physical slot per frame slot.
fn build_aliased_graph() -> FrameGraph<VulkanRenderer> {
    FrameGraphBuilder::new()
        .create_resource(transient_desc("mid_a"))
        .create_resource(transient_desc("mid_b"))
        .add_side_effect_pass(GeometryPass::new("fill_a").without_depth().write_color_ops(
            "mid_a",
            ImageFormat::B8G8R8A8Srgb,
            AttachmentOps::clear(ClearValue::Color([1.0, 0.0, 0.0, 1.0])),
        ))
        .add_side_effect_pass(GeometryPass::new("fill_b").without_depth().write_color_ops(
            "mid_b",
            ImageFormat::B8G8R8A8Srgb,
            AttachmentOps::clear(ClearValue::Color([0.0, 0.0, 1.0, 1.0])),
        ))
        .build::<VulkanRenderer>()
        .unwrap()
}

/// Read back the center pixel of one transient texture as BGRA bytes.
///
/// The picking readback transitions the image to `TRANSFER_SRC` and back to
/// its tracked layout, so the next frame's compiled barriers stay valid.
fn readback_pixel(
    renderer: &mut VulkanRenderer,
    graph: &FrameGraph<VulkanRenderer>,
    name: &str,
    frame_slot: usize,
) -> [u8; 4] {
    let texture = graph
        .transient_texture(name, frame_slot)
        .unwrap_or_else(|| panic!("{name} exists in frame slot {frame_slot}"));
    renderer
        .queue_picking_readback(frame_slot, texture.image, texture.current_layout(), 32, 32)
        .expect("queue picking readback");
    let (_, pixel) = renderer
        .wait_for_picking_readback()
        .expect("wait for picking readback")
        .expect("readback completes");
    pixel.to_le_bytes()
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_aliased_transients_share_storage_and_render_independently() {
    let (mut renderer, errors) = headless_renderer("Transient aliasing test");
    let mut graph = build_aliased_graph();

    let diagnostics = graph.diagnostics().unwrap();
    assert_eq!(
        diagnostics.summary.physical_transient_allocations, 1,
        "the two disjoint transients must compile into one physical slot"
    );

    for frame in 0..3 {
        // render() advances the frame counter, so capture the slot it will
        // use before rendering.
        let frame_slot = renderer.current_frame();
        renderer.render(&mut graph, |_| {}).unwrap();
        renderer.wait_for_frame().unwrap();

        // The live resource shows its own clear color in every frame.
        let mid_b = readback_pixel(&mut renderer, &graph, "mid_b", frame_slot);
        assert_eq!(mid_b, BLUE, "frame {frame}: mid_b must hold its clear");

        // Aliasing makes mid_a's storage physically identical to mid_b's:
        // by readback time the later member's clear has overwritten it.
        let mid_a = readback_pixel(&mut renderer, &graph, "mid_a", frame_slot);
        assert_eq!(
            mid_a, BLUE,
            "frame {frame}: aliased mid_a must show the later member's clear"
        );
    }

    graph.cleanup();
    renderer.destroy();
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_aliasing_disabled_keeps_standalone_storage() {
    let (mut renderer, errors) = headless_renderer("Transient aliasing disabled test");
    let mut graph = build_aliased_graph();
    // Textures initialize lazily at first render; the switch is observed
    // because it is set before that happens.
    graph.set_transient_aliasing(false);

    let frame_slot = renderer.current_frame();
    renderer.render(&mut graph, |_| {}).unwrap();
    renderer.wait_for_frame().unwrap();

    let mid_b = readback_pixel(&mut renderer, &graph, "mid_b", frame_slot);
    assert_eq!(mid_b, BLUE, "mid_b must hold its clear");

    let mid_a = readback_pixel(&mut renderer, &graph, "mid_a", frame_slot);
    assert_eq!(mid_a, RED, "standalone mid_a must keep its own clear color");

    graph.cleanup();
    renderer.destroy();
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
