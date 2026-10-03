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

#[path = "support/ui_bindings.rs"]
mod ui_bindings;

#[path = "support/readback.rs"]
mod readback;

#[path = "support/camera_shader_data.rs"]
mod camera_shader_data;
use camera_shader_data::CameraShaderData;

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use std::rc::Rc;

use katla_gfx::handle::PipelineHandle;

use katla_gfx::render_graph::PassId;

use katla_gfx::render_graph::{
    FrameGraph, FrameGraphBuilder, GeometryPass, GraphResourceDesc, GraphResourceType, UIPass,
};

use katla_gfx::renderer::pipeline_variant::PipelineVariantKey;

use katla_gfx::renderer::{DrawCall, DrawList};

use katla_gfx::texture::ImageFormat;

use katla_gfx::vertex::{VertexPBR, VertexUIInstance};

use katla_gfx::{
    CullMode, DepthState, GpuRenderer, PipelineDescriptor, UIDrawList, UiDrawCommand,
    ValidationMode, VulkanRenderer,
};

/// Acquire one frame from the headless renderer (always ready offscreen).
fn acquire_frame_token(
    renderer: &mut VulkanRenderer,
) -> katla_gfx::renderer::frame_scope::FrameToken {
    use katla_gfx::renderer::frame_scope::FrameAcquisition;
    match renderer.acquire_frame().unwrap() {
        FrameAcquisition::Ready(token) => token,
        other => panic!("headless renderer must acquire a frame, got {other:?}"),
    }
}

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

fn headless_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        WIDTH,
        HEIGHT,
        ValidationMode::Enabled,
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
        .with_depth_format(None)
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
            &[0u32, 1, 2],
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
    format: ImageFormat,
) -> Vec<u8> {
    let uniforms = CameraShaderData {
        view_matrix: identity(),
        proj_matrix: identity(),
        inv_view_proj_matrix: identity(),
        ..Default::default()
    };
    let frame_token = acquire_frame_token(&mut *renderer);
    graph.set_pass_bindings(pass, uniforms.bindings()).unwrap();
    if let Some(draw_list) = draw_list {
        renderer
            .execute_draw_calls(&frame_token, draw_list)
            .unwrap();
    }
    renderer
        .render(&frame_token, graph, |frame_context| {
            if let Some(draw_list) = draw_list {
                frame_context.submit(pass, Rc::new(draw_list.clone()));
            }
        })
        .unwrap();
    assert_eq!(
        renderer
            .present(frame_token)
            .unwrap()
            .surface
            .expect("surface presentation"),
        katla_gfx::SurfaceStatus::Presented
    );
    let (_, pixels) = readback::read_pixels(renderer, graph.resource_id("target").unwrap());
    assert_eq!(
        pixels.len(),
        (WIDTH * HEIGHT * format.bytes_per_pixel()) as usize
    );
    pixels
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_deferred_material_compiles_variant_per_target_format() {
    let mut renderer = headless_renderer();
    let shaders = shaders();

    // `Auto` color format: nothing compiles until a pass uses the material.
    let material = renderer
        .compile_material(&pbr_descriptor(
            shaders.join("../../katla_gfx/tests/support/mesh.wgsl"),
        ))
        .unwrap();
    assert_eq!(
        variant_count(&renderer),
        0,
        "Auto material starts uncompiled"
    );

    render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R16G16B16A16Sfloat,
        None,
        0,
    );
    render_variant_graph(&mut renderer, material, ImageFormat::B8G8R8A8Srgb, None, 1);

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

    // Declared LDR format: that variant compiles eagerly, but the material
    // is not pinned to it — the first compilation context does not define
    // later valid uses.
    let descriptor = pbr_descriptor(shaders.join("../../katla_gfx/tests/support/mesh.wgsl"))
        .with_color_format(ImageFormat::B8G8R8A8Srgb);
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
fn test_hot_reload_replaces_all_live_material_variants() {
    let mut renderer = headless_renderer();
    let shaders = shaders();

    let shader_path = shaders.join("../../katla_gfx/tests/support/mesh.wgsl");
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
        2,
        "every live variant is replaced before publication"
    );

    render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R16G16B16A16Sfloat,
        None,
        1,
    );
    assert_eq!(variant_count(&renderer), 2);

    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_surface_resize_preserves_attachment_variants() {
    let mut renderer = headless_renderer();
    let shaders = shaders();

    let material = renderer
        .compile_material(
            &pbr_descriptor(shaders.join("../../katla_gfx/tests/support/mesh.wgsl"))
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

    renderer.resize(32, 24).unwrap();
    renderer.resize(WIDTH, HEIGHT).unwrap();
    assert_eq!(
        variant_count(&renderer),
        2,
        "surface dimensions do not change pipeline identity"
    );

    render_variant_graph(
        &mut renderer,
        material,
        ImageFormat::R16G16B16A16Sfloat,
        None,
        1,
    );
    assert_eq!(
        variant_count(&renderer),
        2,
        "rendering reuses compatible variants after resize"
    );

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

    let material = renderer
        .compile_material(
            &pbr_descriptor(shaders.join("../../katla_gfx/tests/support/mesh.wgsl"))
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

    let center = ((HEIGHT as usize / 2) * WIDTH as usize + WIDTH as usize / 2) * 4;
    let corner = 0;
    let rgba_pixel = |pixels: &[u8], bgra: bool| -> [u8; 4] {
        let mut pixel: [u8; 4] = pixels[center..center + 4].try_into().unwrap();
        if bgra {
            pixel.swap(0, 2);
        }
        pixel
    };
    for (name, pixels, bgra_order) in [("bgra", &bgra.1, true), ("rgba", &rgba.1, false)] {
        assert_ne!(
            &pixels[center..center + 4],
            &pixels[corner..corner + 4],
            "{name}: triangle must be drawn"
        );
        let pixel = rgba_pixel(pixels, bgra_order);
        assert!(
            pixel[0] > pixel[2] && pixel[2] < 32,
            "{name}: real target red must dominate, got {pixel:?}"
        );
    }
    assert_eq!(
        rgba_pixel(&bgra.1, true),
        rgba_pixel(&rgba.1, false),
        "the same draw shades identically into both native attachment formats"
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
        .create_resource(GraphResourceDesc {
            name: "target".into(),
            resource_type: GraphResourceType::ColorAttachment { clear_value: None },
            format,
            width: WIDTH,
            height: HEIGHT,
            tracks_swapchain_size: true,
        })
        .export_resource("target")
        .add_pass(
            GeometryPass::new("geometry")
                .without_depth()
                .write_color("target", format)
                .clear_color([0.0, 0.0, 0.0, 1.0])
                .material(material),
        )
        .build::<VulkanRenderer>()
        .unwrap();
    let pass = graph.pass_id("geometry").unwrap();
    let pixels = render_once(renderer, &mut graph, pass, draw_list, format);
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
        .create_texture(
            &katla_gfx::TextureDescriptor::rgba8_unorm(1, 1),
            &[255, 0, 0, 255],
        )
        .expect("test texture creation");
    let white = renderer
        .create_texture(&katla_gfx::TextureDescriptor::rgba8_unorm(1, 1), &[255; 4])
        .expect("test texture creation");
    let white_slot = renderer.get_bindless_slot(white).unwrap();
    let shaders = shaders();

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
                .without_depth()
                .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb)
                .clear_color([0.0, 0.0, 1.0, 1.0]),
        )
        .add_pass(UIPass::new("ui").write("backbuffer").material(material))
        .build::<VulkanRenderer>()
        .unwrap();
    let ui_pass = graph.pass_id("ui").unwrap();
    graph
        .set_pass_bindings(ui_pass, ui_bindings::bindings())
        .unwrap();

    for _ in 0..2 {
        let frame_token = acquire_frame_token(&mut renderer);
        renderer
            .render(&frame_token, &mut graph, |frame_context| {
                frame_context.submit_ui(ui_pass, &ui);
            })
            .unwrap();
        assert_eq!(
            renderer
                .present(frame_token)
                .unwrap()
                .surface
                .expect("surface presentation"),
            katla_gfx::SurfaceStatus::Presented
        );
        let (_, pixels) =
            readback::read_pixels(&mut renderer, graph.resource_id("backbuffer").unwrap());
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
