//! Frame-scoped lifecycle contract tests for issue #89.
//!
//! One frame means one token: `acquire_frame` hands out the token and waits for
//! the slot's previous submission, frame-local calls accept only that token, and
//! `present`/`abort` consume it. These tests pin the observable contract:
//!
//! - a normal frame acquires, renders, presents, and reads back;
//! - frame-local calls without an open frame fail typed;
//! - a token that was already finished (or superseded by a later acquisition)
//!   is rejected instead of writing into another frame's slot;
//! - aborting and dropping a token both leave the slot reusable, never stranded;
//! - the slot index cycles across frames in flight and prior work completes
//!   before its slot is handed out again.
//!
//! Surface-unavailable and out-of-date acquisitions need a real presentation
//! surface (a minimized window or a stale swapchain); a headless renderer has
//! neither, so those two branches are covered by the app's swapchain-recreation
//! path and the Metal drawable path instead.
//!
//! Device tests need a Vulkan device (`#[ignore]`, run like the other GPU
//! contract suites:
//! `TMPDIR=$HOME/tmp cargo test -p katla_gfx --test frame_scope -- --ignored`).

use std::ffi::CString;

use katla_gfx::render_graph::{FrameGraph, FrameGraphBuilder, GeometryPass};
use katla_gfx::renderer::frame_scope::{FrameAcquisition, FrameToken};
use katla_gfx::texture::ImageFormat;
use katla_gfx::{GpuRenderer, RendererError, ValidationMode, VulkanRenderer};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;
/// The headless renderer's slot count (see `SwapData`).
const FRAMES_IN_FLIGHT: usize = 2;

fn headless_renderer(label: &str) -> VulkanRenderer {
    VulkanRenderer::init_headless(
        WIDTH,
        HEIGHT,
        ValidationMode::Disabled,
        CString::new(label).unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

fn backbuffer_graph() -> FrameGraph<VulkanRenderer> {
    FrameGraphBuilder::new()
        .add_pass(GeometryPass::new("clear").write_color("backbuffer", ImageFormat::B8G8R8A8Srgb))
        .build::<VulkanRenderer>()
        .unwrap_or_else(|error| panic!("backbuffer graph must build: {error}"))
}

fn acquire(renderer: &mut VulkanRenderer) -> FrameToken {
    match renderer.acquire_frame().expect("acquire_frame") {
        FrameAcquisition::Ready(token) => token,
        other => panic!("headless renderer must acquire a frame, got {other:?}"),
    }
}

/// A frame-local call must name the open frame; using it with no acquisition
/// fails typed instead of silently targeting whatever slot the renderer holds.
#[test]
#[ignore = "requires a Vulkan device"]
fn test_frame_local_calls_require_an_open_frame() {
    let mut renderer = headless_renderer("Frame scope: no open frame");
    let stale = FrameToken::new(0, 0);

    let error = renderer
        .set_frame_uniforms(&stale, Default::default())
        .expect_err("no frame is open");
    assert!(
        matches!(&error, RendererError::InvalidOperation(message) if message.contains("no frame is currently acquired")),
        "unexpected error: {error:?}"
    );
    assert!(
        renderer.upload_shadow_cascades(&stale).is_err(),
        "shadow cascade upload must require an open frame"
    );

    renderer.destroy();
}

/// The documented normal frame: acquire, write frame-local data, execute the
/// graph, present, and read the result back.
#[test]
#[ignore = "requires a Vulkan device"]
fn test_normal_frame_acquires_renders_and_presents() {
    let mut renderer = headless_renderer("Frame scope: normal frame");
    let mut graph = backbuffer_graph();

    let token = acquire(&mut renderer);
    renderer
        .set_frame_uniforms(&token, Default::default())
        .expect("frame-local write on the open frame");
    renderer
        .render(&token, &mut graph, |_| {})
        .expect("graph execution");
    renderer.present(token).expect("present");

    renderer.queue_async_readback(0).expect("queue readback");
    let (_, pixels) = renderer
        .wait_for_pending_readback()
        .expect("wait for readback")
        .expect("readback completes");
    assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);

    graph.cleanup();
    renderer.destroy();
}

/// Presenting consumes the token: the same token must not address a later
/// frame. This is what stops a caller from writing per-object data into a slot
/// a subsequent acquisition already owns.
#[test]
#[ignore = "requires a Vulkan device"]
fn test_finished_and_superseded_tokens_are_rejected() {
    let mut renderer = headless_renderer("Frame scope: stale tokens");
    let mut graph = backbuffer_graph();

    let presented = acquire(&mut renderer);
    renderer
        .render(&presented, &mut graph, |_| {})
        .expect("graph execution");
    renderer.present(presented).expect("present");

    // `FrameToken` is `Copy`, so a caller can hold on to it; the renderer must
    // refuse it now that the frame is closed.
    let error = renderer
        .execute_draw_calls(&presented, &Default::default())
        .expect_err("a presented token is no longer the open frame");
    assert!(
        matches!(&error, RendererError::InvalidOperation(message) if message.contains("no frame is currently acquired")),
        "unexpected error: {error:?}"
    );
    assert!(
        renderer.present(presented).is_err(),
        "presenting the same frame twice must fail"
    );

    // A later acquisition supersedes the old token even when it lands on the
    // same reusable slot: ownership is per acquisition, not per slot index.
    let superseded = acquire(&mut renderer);
    let current = acquire(&mut renderer);
    assert_eq!(
        superseded.slot(),
        current.slot(),
        "an unfinished frame keeps its slot; only present advances it"
    );
    let error = renderer
        .set_frame_uniforms(&superseded, Default::default())
        .expect_err("a superseded token must be rejected");
    assert!(
        matches!(&error, RendererError::InvalidOperation(message) if message.contains("stale frame token")),
        "unexpected error: {error:?}"
    );
    renderer
        .set_frame_uniforms(&current, Default::default())
        .expect("the current token is still accepted");

    renderer.abort(current).expect("abort the open frame");
    graph.cleanup();
    renderer.destroy();
}

/// Aborting submits and presents nothing, leaves the slot reusable, and the
/// next acquisition hands the same slot back — an abandoned frame can never
/// strand a slot.
#[test]
#[ignore = "requires a Vulkan device"]
fn test_aborted_frame_leaves_its_slot_reusable() {
    let mut renderer = headless_renderer("Frame scope: abort");
    let mut graph = backbuffer_graph();

    let aborted = acquire(&mut renderer);
    renderer.abort(aborted).expect("abort");

    let reused = acquire(&mut renderer);
    assert_eq!(
        reused.slot(),
        aborted.slot(),
        "abort must not advance the slot, so the next frame reuses it"
    );
    renderer
        .render(&reused, &mut graph, |_| {})
        .expect("the reused slot still renders");
    renderer.present(reused).expect("present");

    graph.cleanup();
    renderer.destroy();
}

/// Abandoning an acquisition — acquiring again without finishing — has the same
/// effect as `abort`: the abandoned slot is released and rendering continues
/// normally. (`FrameToken` is `Copy`, so there is no destructor to hang this
/// on; abandonment is simply the next acquisition.)
#[test]
#[ignore = "requires a Vulkan device"]
fn test_abandoned_acquisition_reuses_the_slot_normally() {
    let mut renderer = headless_renderer("Frame scope: abandoned frame");
    let mut graph = backbuffer_graph();

    let abandoned = acquire(&mut renderer);

    let reused = acquire(&mut renderer);
    assert_eq!(
        reused.slot(),
        abandoned.slot(),
        "an abandoned frame strands no slot"
    );
    renderer
        .render(&reused, &mut graph, |_| {})
        .expect("graph execution");
    renderer.present(reused).expect("present");

    graph.cleanup();
    renderer.destroy();
}

/// Frames in flight cycle through the reusable slots, and re-acquiring a slot
/// whose previous submission is still in flight waits for that submission
/// before handing the token out.
#[test]
#[ignore = "requires a Vulkan device"]
fn test_slots_cycle_and_reuse_completes_prior_work() {
    let mut renderer = headless_renderer("Frame scope: slots in flight");
    let mut graph = backbuffer_graph();

    let mut slots = Vec::new();
    for frame in 0..(FRAMES_IN_FLIGHT * 2) {
        // No explicit wait: the previous owner of this slot may still be
        // executing, and acquire_frame must not hand out an unfinished slot.
        let token = acquire(&mut renderer);
        slots.push(token.slot());
        renderer
            .set_frame_uniforms(&token, Default::default())
            .expect("frame-local write");
        renderer
            .render(&token, &mut graph, |_| {})
            .expect("graph execution");
        renderer.present(token).expect("present");

        renderer.queue_async_readback(frame).unwrap();
        let (captured, pixels) = renderer
            .wait_for_pending_readback()
            .expect("wait for readback")
            .expect("readback completes");
        assert_eq!(captured, frame, "readback belongs to the presenting frame");
        assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
        assert!(
            pixels.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 255]),
            "frame {frame} rendered the declared clear"
        );
    }

    assert_eq!(
        slots,
        (0..(FRAMES_IN_FLIGHT * 2))
            .map(|frame| frame % FRAMES_IN_FLIGHT)
            .collect::<Vec<_>>(),
        "slots must cycle through the reusable frame slots"
    );

    graph.cleanup();
    renderer.destroy();
}
