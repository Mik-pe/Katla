//! Resource-lifetime contracts: stale handles can never alias reused slots,
//! double destroys are harmless, a destroyed texture's slot stays withheld
//! while frames are in flight, and recycled frame slots never bleed state
//! between frames.

use katla_gfx::render_graph::{GeometryPass, UIPass};
use katla_gfx::render_pass::{AttachmentOps, ClearValue};
use katla_gfx::texture::ImageFormat;
use katla_gfx::vertex::VertexUIInstance;
use katla_gfx::{GpuRenderer, RendererError, UIDrawList, UiDrawCommand};

use crate::harness::{self, CHANNEL_GREEN, CHANNEL_RED, pixel_offset};
use harness::ContractRenderer;

/// A full-frame quad submitting a texture color through the UI material.
fn ui_quad(texture: katla_gfx::TextureHandle, slot: u32, color: [u8; 4]) -> UIDrawList {
    let mut ui = UIDrawList {
        screen_size: [64.0, 48.0],
        scale_factor: 1.0,
        ..Default::default()
    };
    ui.instances.push(VertexUIInstance {
        position: [0.0, 0.0],
        size: [64.0, 48.0],
        uv_min: [0.0; 2],
        uv_max: [1.0; 2],
        color,
        texture_index: slot,
        clip_rect: [0.0, 0.0, 64.0, 48.0],
    });
    ui.commands = vec![UiDrawCommand::instanced(0, 1, None, texture)];
    ui
}

fn single_color_graph(
    scenario_material: katla_gfx::MaterialHandle,
) -> katla_gfx::render_graph::any_frame_graph::AnyFrameGraph {
    harness::build_graph(|builder| {
        builder
            .add_pass(GeometryPass::new("background").write_color_ops(
                "backbuffer",
                ImageFormat::B8G8R8A8Srgb,
                AttachmentOps::clear(ClearValue::Color([0.0, 0.0, 0.0, 1.0])),
            ))
            .add_pass(
                UIPass::new("ui")
                    .write("backbuffer")
                    .material(scenario_material),
            )
    })
}

fn render_ui(
    renderer: &mut ContractRenderer,
    graph: &mut katla_gfx::render_graph::any_frame_graph::AnyFrameGraph,
    ui: &UIDrawList,
) -> Vec<u8> {
    let ui_pass = harness::pass_id(graph, "ui");
    renderer.render_frame(graph, None, None, |frame| {
        frame.submit_ui(ui_pass, ui);
    })
}

fn solid_texture(renderer: &mut ContractRenderer, color: [u8; 4]) -> katla_gfx::TextureHandle {
    renderer
        .gfx()
        .create_texture(&katla_gfx::TextureDescriptor::rgba8_unorm(1, 1), &color)
        .expect("solid contract texture")
}

/// After a texture is destroyed and its slot is recycled, the stale handle
/// must be inert: no bindless resolution, no updates, no aliasing into the
/// slot's new owner — which itself must keep rendering.
#[test]
#[ignore = "requires a graphics device"]
fn test_contract_stale_texture_handles_never_alias_reused_slots() {
    let mut renderer = ContractRenderer::open("contract: stale texture handles");
    renderer.init_frame_pipelines();
    let scene = harness::init_ui_scene(renderer.gfx());

    let red = solid_texture(&mut renderer, [255, 0, 0, 255]);
    renderer.gfx().destroy_texture(red);

    let green = solid_texture(&mut renderer, [0, 255, 0, 255]);
    let green_slot = renderer.gfx().get_bindless_slot(green).expect("green slot");
    assert_eq!(
        renderer.gfx().get_texture_at_slot(green_slot),
        Some(green),
        "the recycled slot's new owner must be resolvable"
    );
    assert_eq!(
        renderer.gfx().get_bindless_slot(red),
        None,
        "a destroyed handle must not resolve to any slot"
    );
    assert!(
        matches!(
            renderer.gfx().update_texture(red, &[1, 2, 3, 4]),
            Err(RendererError::StaleHandle { .. })
        ),
        "updates through a destroyed handle must fail typed"
    );

    let ui = ui_quad(green, green_slot, [255, 255, 255, 255]);
    let mut graph = single_color_graph(scene.material);
    let pixels = render_ui(&mut renderer, &mut graph, &ui);
    assert_eq!(
        harness::dominant_channel(&pixels, pixel_offset(0.0, 0.0)),
        Some(CHANNEL_GREEN),
        "the slot's new owner must sample green, never the destroyed texture"
    );
    harness::cleanup_graph(graph);
    renderer.finish();
}

/// Destroying a handle twice — with the slot recycled in between — must be a
/// harmless no-op that leaves the slot's new owner untouched.
#[test]
#[ignore = "requires a graphics device"]
fn test_contract_double_destroy_after_slot_reuse_is_harmless() {
    let mut renderer = ContractRenderer::open("contract: double destroy after reuse");
    renderer.init_frame_pipelines();
    let scene = harness::init_ui_scene(renderer.gfx());

    let red = solid_texture(&mut renderer, [255, 0, 0, 255]);
    renderer.gfx().destroy_texture(red);
    let green = solid_texture(&mut renderer, [0, 255, 0, 255]);
    let green_slot = renderer.gfx().get_bindless_slot(green).expect("green slot");

    renderer.gfx().destroy_texture(red);

    assert_eq!(renderer.gfx().get_bindless_slot(red), None);
    assert_eq!(renderer.gfx().get_bindless_slot(green), Some(green_slot));
    assert_eq!(renderer.gfx().get_texture_at_slot(green_slot), Some(green));

    let ui = ui_quad(green, green_slot, [255, 255, 255, 255]);
    let mut graph = single_color_graph(scene.material);
    let pixels = render_ui(&mut renderer, &mut graph, &ui);
    assert_eq!(
        harness::dominant_channel(&pixels, pixel_offset(0.0, 0.0)),
        Some(CHANNEL_GREEN),
        "the second destroy must not disturb the slot's new owner"
    );

    renderer.gfx().destroy_texture(green);
    assert_eq!(renderer.gfx().get_bindless_slot(green), None);
    harness::cleanup_graph(graph);
    renderer.finish();
}

/// A destroyed texture's bindless slot must stay withheld while its frames
/// may still be in flight: a replacement texture must not silently land in
/// the recycled slot, and only after the pending retirement drains may the
/// exact slot be handed out again. Destroyed-content pixels must never
/// reappear.
#[test]
#[ignore = "requires a graphics device"]
fn test_contract_destroyed_texture_slot_is_withheld_until_frames_drain() {
    let mut renderer = ContractRenderer::open("contract: slot withholding");
    renderer.init_frame_pipelines();
    let scene = harness::init_ui_scene(renderer.gfx());
    let mut graph = single_color_graph(scene.material);

    let red = solid_texture(&mut renderer, [255, 0, 0, 255]);
    let red_slot = renderer.gfx().get_bindless_slot(red).expect("red slot");
    let ui_red = ui_quad(red, red_slot, [255, 255, 255, 255]);
    let pixels = render_ui(&mut renderer, &mut graph, &ui_red);
    assert_eq!(
        harness::dominant_channel(&pixels, pixel_offset(0.0, 0.0)),
        Some(CHANNEL_RED),
        "the red texture must render before destruction"
    );

    renderer.gfx().destroy_texture(red);
    let green = solid_texture(&mut renderer, [0, 255, 0, 255]);
    if renderer.caps().retirement_diagnostics {
        let green_slot = renderer.gfx().get_bindless_slot(green).expect("green slot");
        assert_ne!(
            green_slot, red_slot,
            "the destroyed texture's slot must stay withheld while frames are pending"
        );
    }

    let green_slot = renderer.gfx().get_bindless_slot(green).expect("green slot");
    let ui_green = ui_quad(green, green_slot, [255, 255, 255, 255]);
    let pixels = render_ui(&mut renderer, &mut graph, &ui_green);
    assert_eq!(
        harness::dominant_channel(&pixels, pixel_offset(0.0, 0.0)),
        Some(CHANNEL_GREEN),
        "the replacement texture must render"
    );
    assert!(
        harness::no_pixel_dominates(&pixels, CHANNEL_RED),
        "destroyed content must never reappear through a recycled slot"
    );

    // Drain the pending retirement across rendered frames, then verify the
    // exact slot is released back into circulation.
    if renderer.caps().retirement_diagnostics {
        let mut snapshot = renderer
            .pending_retirements()
            .expect("snapshots available on this backend");
        for _ in 0..8 {
            if snapshot.bindless_slots == 0 && snapshot.textures == 0 {
                break;
            }
            render_ui(&mut renderer, &mut graph, &ui_green);
            snapshot = renderer
                .pending_retirements()
                .expect("snapshots available on this backend");
        }
        assert_eq!(
            (snapshot.textures, snapshot.bindless_slots),
            (0, 0),
            "the destroyed texture and its slot must retire: {}",
            snapshot.summary()
        );
        let recycled = solid_texture(&mut renderer, [0, 0, 255, 255]);
        assert_eq!(
            renderer.gfx().get_bindless_slot(recycled),
            Some(red_slot),
            "the released slot must return to circulation exactly"
        );
    }

    harness::cleanup_graph(graph);
    renderer.finish();
}

/// Recycled frame slots must never bleed state between frames: alternating
/// frames between two graphs with different declared clears must return each
/// frame's own color, for more than a full frames-in-flight cycle.
#[test]
#[ignore = "requires a graphics device"]
fn test_contract_frame_slots_keep_frames_independent() {
    let mut renderer = ContractRenderer::open_without_api_validation("contract: frame slots");
    renderer.init_frame_pipelines();

    let mut red_graph = harness::build_graph(|builder| {
        builder.add_pass(GeometryPass::new("background").write_color_ops(
            "backbuffer",
            ImageFormat::B8G8R8A8Srgb,
            AttachmentOps::clear(ClearValue::Color([1.0, 0.0, 0.0, 1.0])),
        ))
    });
    let mut black_graph = harness::build_graph(|builder| {
        builder.add_pass(GeometryPass::new("background").write_color_ops(
            "backbuffer",
            ImageFormat::B8G8R8A8Srgb,
            AttachmentOps::clear(ClearValue::Color([0.0, 0.0, 0.0, 1.0])),
        ))
    });
    let cycles = (renderer.gfx().num_images() * 2).max(4);

    for frame in 0..cycles {
        let expect_red = frame % 2 == 0;
        let pixels = if expect_red {
            renderer.render_frame(&mut red_graph, None, None, |_| {})
        } else {
            renderer.render_frame(&mut black_graph, None, None, |_| {})
        };
        let probe = harness::dominant_channel(&pixels, pixel_offset(0.0, 0.0));
        if expect_red {
            assert_eq!(
                probe,
                Some(CHANNEL_RED),
                "frame {frame}: the red graph's clear must not bleed into other frames"
            );
        } else {
            assert_eq!(
                probe, None,
                "frame {frame}: the black graph's clear must stay black"
            );
        }
    }

    harness::cleanup_graph(red_graph);
    harness::cleanup_graph(black_graph);
    renderer.finish();
}
