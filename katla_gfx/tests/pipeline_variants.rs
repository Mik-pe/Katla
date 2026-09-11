//! Pipeline variant contract tests for issue #88.
//!
//! A material owns its compilation identity and a cache of pipeline
//! variants keyed by the canonical target configuration. One material must
//! serve every render-target configuration it is used with: each
//! configuration compiles its own variant on first use, repeated uses hit
//! the cache, shader hot reload drops exactly the affected variants, and
//! descriptor-layout invalidation drops every variant for recompilation.
//!
//! Device tests need a Vulkan device (`#[ignore]`, run like the other GPU
//! contract suites:
//! `TMPDIR=$HOME/tmp cargo test -p katla_gfx --test pipeline_variants -- --ignored`).

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use katla_gfx::handle::PipelineHandle;
use katla_gfx::render_graph::PassId;
use katla_gfx::render_graph::{FrameGraph, FrameGraphBuilder, GeometryPass, UIPass};
use katla_gfx::renderer::pipeline_variant::PipelineVariantKey;
use katla_gfx::renderer::{DrawCall, DrawList};
use katla_gfx::texture::ImageFormat;
use katla_gfx::vertex::{VertexPBR, VertexUIInstance};
use katla_gfx::{
    CullMode, DepthState, FrameUniforms, GpuRenderer, PipelineDescriptor, UIDrawList,
    UiDrawCommand, ValidationMode, VulkanRenderer,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

fn headless_renderer() -> VulkanRenderer {
    // ValidationMode::Disabled: compiling the PBR pipeline under the system
    // validation layer segfaults the Intel driver on this machine (same
    // trade-off as the instancing suite).
    VulkanRenderer::init_headless(
        WIDTH,
        HEIGHT,
        ValidationMode::Disabled,
        CString::new("Pipeline variants test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

fn shaders() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders")
}

fn pbr_descriptor(shader: std::path::PathBuf) -> PipelineDescriptor {
    PipelineDescriptor::pbr(shader.to_string_lossy().into_owned())
        .with_depth(DepthState::disabled())
        .with_cull(CullMode::None)
}

fn variant_pipeline(
    renderer: &VulkanRenderer,
    material: katla_gfx::MaterialHandle,
    format: ImageFormat,
) -> Option<PipelineHandle> {
    let descriptor = renderer
        .asset_registry
        .get_material(material)
        .expect("material exists")
        .descriptor
        .clone();
    let key = PipelineVariantKey::resolve(&descriptor, format);
    renderer
        .asset_registry
        .material_variant(material, &key)
        .map(|variant| variant.pipeline)
}

fn variant_count(renderer: &VulkanRenderer) -> usize {
    renderer.asset_registry.material_variant_count()
}

/// Handles carry no `PartialEq` (generational identity is opaque); tests
/// compare the (index, generation) pair instead.
fn handle_id(handle: katla_gfx::handle::PipelineHandle) -> (u32, u32) {
    (handle.index(), handle.generation())
}

fn identity() -> [f32; 16] {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

fn triangle_vertices() -> Vec<VertexPBR> {
    vec![
        VertexPBR {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [0.0, 0.0],
        },
        VertexPBR {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [1.0, 0.0],
        },
        VertexPBR {
            position: [0.0, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [0.5, 1.0],
        },
    ]
}

fn triangle_draw_list(
    renderer: &mut VulkanRenderer,
    material: katla_gfx::MaterialHandle,
) -> DrawList {
    let mesh = renderer
        .create_mesh(
            &triangle_vertices(),
            &vec![0u32, 1, 2],
            katla_gfx::PrimitiveTopology::TriangleList,
        )
        .expect("test mesh creation");
    let mut list = DrawList::new();
    list.push(
        DrawCall::new(mesh, material)
            .with_transform(identity())
            .with_color([1.0, 0.0, 0.0, 1.0]),
    );
    list
}

fn render_once(
    renderer: &mut VulkanRenderer,
    graph: &mut FrameGraph<VulkanRenderer>,
    pass: PassId,
    draw_list: Option<&DrawList>,
    frame: usize,
) -> Vec<u8> {
    let uniforms = FrameUniforms {
        view_matrix: identity(),
        proj_matrix: identity(),
        inv_view_proj_matrix: identity(),
        ..Default::default()
    };
    renderer.wait_for_frame().unwrap();
    renderer.set_frame_uniforms(uniforms);
    if let Some(draw_list) = draw_list {
        renderer.execute_draw_calls(draw_list).unwrap();
    }
    renderer
        .render(graph, |frame_context| {
            if let Some(draw_list) = draw_list {
                frame_context.submit(pass, draw_list);
            }
        })
        .unwrap();
    renderer.queue_async_readback(frame).unwrap();
    let (_, pixels) = renderer.wait_for_pending_readback().unwrap().unwrap();
    assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
    pixels
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_deferred_material_compiles_variant_per_target_format() {
    let mut renderer = headless_renderer();
    let shaders = shaders();
    renderer
        .init_light_culling(WIDTH, HEIGHT, &shaders.join("lighting/light_cull.wgsl"))
        .unwrap();
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();

    // `Auto` color format: nothing compiles until a pass uses the material.
    let material = renderer
        .compile_material(&pbr_descriptor(shaders.join("model_pbr.wgsl")))
        .unwrap();
    assert_eq!(
        variant_count(&renderer),
        0,
        "Auto material starts uncompiled"
    );

    let hdr = render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R16G16B16A16Sfloat,
        None,
        0,
    );
    let ldr = render_variant_graph(&mut renderer, material, ImageFormat::B8G8R8A8Srgb, None, 1);

    assert_eq!(
        variant_count(&renderer),
        2,
        "one variant per used target configuration"
    );
    let hdr_pipeline = variant_pipeline(&renderer, material, ImageFormat::R16G16B16A16Sfloat)
        .expect("HDR variant compiled");
    let ldr_pipeline = variant_pipeline(&renderer, material, ImageFormat::B8G8R8A8Srgb)
        .expect("LDR variant compiled");
    assert_ne!(
        handle_id(hdr_pipeline),
        handle_id(ldr_pipeline),
        "different target formats compile different pipelines"
    );

    // Re-rendering the HDR configuration hits the cache: same pipeline, no
    // new variant.
    let (frame, _) = render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R16G16B16A16Sfloat,
        None,
        2,
    );
    assert_eq!(frame, 2);
    assert_eq!(variant_count(&renderer), 2, "cache hit adds no variant");
    assert_eq!(
        variant_pipeline(&renderer, material, ImageFormat::R16G16B16A16Sfloat).map(handle_id),
        Some(handle_id(hdr_pipeline)),
        "cached variant is deterministic"
    );

    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_declared_format_material_gains_variant_for_second_format() {
    let mut renderer = headless_renderer();
    let shaders = shaders();
    renderer
        .init_light_culling(WIDTH, HEIGHT, &shaders.join("lighting/light_cull.wgsl"))
        .unwrap();
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();

    // Declared LDR format: that variant compiles eagerly, but the material
    // is not pinned to it — the first compilation context does not define
    // later valid uses.
    let descriptor =
        pbr_descriptor(shaders.join("model_pbr.wgsl")).with_color_format(ImageFormat::B8G8R8A8Srgb);
    let material = renderer.compile_material(&descriptor).unwrap();
    assert_eq!(
        variant_count(&renderer),
        1,
        "declared format compiles eagerly"
    );
    let declared_pipeline =
        variant_pipeline(&renderer, material, ImageFormat::B8G8R8A8Srgb).expect("declared variant");

    render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R16G16B16A16Sfloat,
        None,
        0,
    );

    assert_eq!(variant_count(&renderer), 2);
    assert_ne!(
        variant_pipeline(&renderer, material, ImageFormat::R16G16B16A16Sfloat).map(handle_id),
        Some(handle_id(declared_pipeline)),
        "the second configuration gets its own variant"
    );

    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_hot_reload_drops_only_matching_materials_variants() {
    let mut renderer = headless_renderer();
    let shaders = shaders();
    renderer
        .init_light_culling(WIDTH, HEIGHT, &shaders.join("lighting/light_cull.wgsl"))
        .unwrap();
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();

    let shader_path = shaders.join("model_pbr.wgsl");
    let material = renderer
        .compile_material(
            &pbr_descriptor(shader_path.clone()).with_color_format(ImageFormat::B8G8R8A8Srgb),
        )
        .unwrap();
    render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R16G16B16A16Sfloat,
        None,
        0,
    );
    assert_eq!(variant_count(&renderer), 2);

    let recompiled = renderer.recompile_materials_for_shader(&shader_path);
    assert_eq!(
        recompiled, 1,
        "the material compiled from the changed shader"
    );
    assert_eq!(
        variant_count(&renderer),
        0,
        "every variant of the affected material is invalidated"
    );

    // The next use recompiles what it needs from disk.
    render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R16G16B16A16Sfloat,
        None,
        1,
    );
    assert_eq!(variant_count(&renderer), 1);

    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_layout_invalidation_drops_all_variants_and_recompiles() {
    let mut renderer = headless_renderer();
    let shaders = shaders();
    renderer
        .init_light_culling(WIDTH, HEIGHT, &shaders.join("lighting/light_cull.wgsl"))
        .unwrap();
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();

    let material = renderer
        .compile_material(
            &pbr_descriptor(shaders.join("model_pbr.wgsl"))
                .with_color_format(ImageFormat::B8G8R8A8Srgb),
        )
        .unwrap();
    render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R16G16B16A16Sfloat,
        None,
        0,
    );
    assert_eq!(variant_count(&renderer), 2);

    // A descriptor-layout change (light culling resize) is an input of
    // every variant key: no variant survives, and the next use recompiles
    // against the new layouts.
    renderer.recreate_scene_render_targets(32, 24);
    assert_eq!(variant_count(&renderer), 0, "all variants invalidated");

    render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R16G16B16A16Sfloat,
        None,
        1,
    );
    assert_eq!(variant_count(&renderer), 1, "recompiles after invalidation");

    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_one_material_renders_into_two_attachment_formats() {
    let mut renderer = headless_renderer();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured_errors = errors.clone();
    renderer
        .context()
        .set_validation_callback(move |message, level| {
            if level == katla_gfx::ValidationLevel::Error {
                captured_errors.lock().unwrap().push(message.to_owned());
            }
        });

    let shaders = shaders();
    renderer
        .init_light_culling(WIDTH, HEIGHT, &shaders.join("lighting/light_cull.wgsl"))
        .unwrap();
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();

    let material = renderer
        .compile_material(
            &pbr_descriptor(shaders.join("model_pbr.wgsl"))
                .with_color_format(ImageFormat::B8G8R8A8Srgb),
        )
        .unwrap();
    let draw_list = triangle_draw_list(&mut renderer, material);

    // The same material draws the same triangle into a B8G8R8A8 target and
    // an R8G8B8A8 target; each pass compiles/binds the variant for its own
    // attachment format.
    let bgra = render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::B8G8R8A8Srgb,
        Some(&draw_list),
        0,
    );
    let rgba = render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R8G8B8A8Srgb,
        Some(&draw_list),
        1,
    );
    assert_eq!(variant_count(&renderer), 2);

    // The triangle covers the center probe in both captures. Readback
    // copies the headless drawable (always BGRA), which normalizes byte
    // order, so red dominates byte 2 in both — what differs is which
    // pipeline variant each declared attachment format compiled and bound.
    let center = ((HEIGHT as usize / 2) * WIDTH as usize + WIDTH as usize / 2) * 4;
    let corner = 0;
    for (name, pixels) in [("bgra", &bgra.1), ("rgba", &rgba.1)] {
        assert_ne!(
            &pixels[center..center + 4],
            &pixels[corner..corner + 4],
            "{name}: triangle must be drawn"
        );
        assert!(
            pixels[center + 2] > pixels[center] && pixels[center] < 32,
            "{name}: red must dominate the readback byte 2, got {:?}",
            &pixels[center..center + 4]
        );
    }
    assert_eq!(
        &bgra.1[center..center + 4],
        &rgba.1[center..center + 4],
        "the same draw shades identically into both attachment formats"
    );

    assert!(
        errors.lock().unwrap().is_empty(),
        "no validation errors: {:?}",
        errors.lock().unwrap()
    );

    renderer.destroy();
}

/// Render one frame of a single-pass graph targeting `format`, returning
/// the frame counter and (when the pass draws) the captured pixels.
fn render_variant_graph(
    renderer: &mut VulkanRenderer,
    material: katla_gfx::MaterialHandle,
    format: ImageFormat,
    draw_list: Option<&DrawList>,
    frame: usize,
) -> (usize, Vec<u8>) {
    let mut graph: FrameGraph<VulkanRenderer> = FrameGraphBuilder::new()
        .add_pass(
            GeometryPass::new("geometry")
                .write_color("backbuffer", format)
                .clear_color([0.0, 0.0, 0.0, 1.0])
                .material(material),
        )
        .build::<VulkanRenderer>()
        .unwrap();
    let pass = graph.pass_id("geometry").unwrap();
    let pixels = render_once(renderer, &mut graph, pass, draw_list, frame);
    graph.cleanup();
    drop(graph);
    (frame, pixels)
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_deferred_material_compiles_pipeline_for_the_declared_format() {
    // Regression guard: a variant of an `Auto` material must never build
    // with an undefined attachment format — validation rejects the draw
    // the moment such a pipeline binds (VUID-08963/08910).
    let mut renderer = VulkanRenderer::init_headless(
        WIDTH,
        HEIGHT,
        ValidationMode::Enabled,
        CString::new("Pipeline variants deferred test").unwrap(),
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

    let _atlas = renderer
        .create_ui_font_atlas(1, 1, &[255, 0, 0, 255])
        .expect("test font atlas creation");
    let white = renderer
        .create_texture(&katla_gfx::TextureDescriptor::rgba8_unorm(1, 1), &[255; 4])
        .expect("test texture creation");
    let white_slot = renderer.get_bindless_slot(white).unwrap();
    let shaders = shaders();

    // UI pipelines are safe to compile under the validation layer (unlike
    // PBR on this machine's system driver). `Auto` defers compilation to
    // the first use.
    let descriptor =
        PipelineDescriptor::ui(shaders.join("ui/ui.wgsl").to_string_lossy().into_owned())
            .with_color_format(ImageFormat::Auto);
    let material = renderer.compile_material(&descriptor).unwrap();
    assert_eq!(
        variant_count(&renderer),
        0,
        "Auto material starts uncompiled"
    );

    let ui = {
        let mut ui = UIDrawList {
            screen_size: [WIDTH as f32, HEIGHT as f32],
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
            clip_rect: [0.0, 0.0, WIDTH as f32, HEIGHT as f32],
        });
        ui.commands = vec![UiDrawCommand::instanced(0, 1, None)];
        ui
    };

    let mut graph: FrameGraph<VulkanRenderer> = FrameGraphBuilder::new()
        .add_pass(
            GeometryPass::new("background")
                .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb)
                .clear_color([0.0, 0.0, 1.0, 1.0]),
        )
        .add_pass(UIPass::new("ui").write("backbuffer").material(material))
        .build::<VulkanRenderer>()
        .unwrap();
    let ui_pass = graph.pass_id("ui").unwrap();

    for frame in 0..2 {
        renderer.wait_for_frame().unwrap();
        renderer
            .render(&mut graph, |frame_context| {
                frame_context.submit_ui(ui_pass, &ui);
            })
            .unwrap();
        renderer.queue_async_readback(frame).unwrap();
        let (_, pixels) = renderer.wait_for_pending_readback().unwrap().unwrap();
        // The green quad covers the left probe; the right probe stays the
        // background's blue.
        let probe = ((8usize) * WIDTH as usize + 8) * 4;
        assert_eq!(
            &pixels[probe..probe + 4],
            &[0, 255, 0, 255],
            "the deferred material's UI pipeline must render"
        );
        let corner = ((40usize) * WIDTH as usize + 24) * 4;
        // BGRA memory order: the blue clear reads back B-first.
        assert_eq!(&pixels[corner..corner + 4], &[255, 0, 0, 255]);
    }
    graph.cleanup();
    drop(graph);

    assert!(
        errors.lock().unwrap().is_empty(),
        "no validation errors: {:?}",
        errors.lock().unwrap()
    );
    assert!(
        variant_pipeline(&renderer, material, ImageFormat::B8G8R8A8Srgb).is_some(),
        "the deferred material compiled a variant for the UI pass format"
    );

    renderer.destroy();
}
