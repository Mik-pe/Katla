//! Deferred native-resource retirement contract tests for issue #84.
//!
//! Destroyed and replaced resources (textures, materials, pipelines,
//! skeletons, grown buffers) must invalidate their logical handles
//! immediately while their native GPU objects stay alive until the
//! submissions that can still reference them have completed. Bindless slots
//! stay occupied for that whole window: a new texture can never resolve
//! through a slot an in-flight submission still reads.
//!
//! All tests need a Vulkan device (`#[ignore]`, run like the other GPU
//! contract suites:
//! `TMPDIR=$HOME/tmp cargo test -p katla_gfx --test resource_retirement -- --ignored`).

use std::ffi::CString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use katla_gfx::render_graph::{FrameGraph, FrameGraphBuilder, GeometryPass};
use katla_gfx::texture::ImageFormat;
use katla_gfx::{
    FrameUniforms, GpuRenderer, MaterialHandle, MeshHandle, PipelineDescriptor, PrimitiveTopology,
    ValidationMode, VulkanRenderer,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;
/// Frames rendered to age retirements past FRAMES_IN_FLIGHT.
const DRAIN_FRAMES: usize = 6;

fn headless_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        WIDTH,
        HEIGHT,
        // ValidationMode::Disabled: compiling the PBR pipeline under the
        // system validation layer segfaults the Intel driver on this machine
        // (same caveat as the other GPU contract tests). The validation
        // callback below still captures messages when layers are present.
        ValidationMode::Disabled,
        CString::new("Resource retirement test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

fn shaders() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders")
}

fn identity() -> [f32; 16] {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

fn uniforms() -> FrameUniforms {
    FrameUniforms {
        view_matrix: identity(),
        proj_matrix: identity(),
        inv_view_proj_matrix: identity(),
        ..Default::default()
    }
}

fn compile_ui_material(renderer: &mut VulkanRenderer) -> MaterialHandle {
    renderer
        .compile_material(&PipelineDescriptor::ui(
            shaders().join("ui/ui.wgsl").to_string_lossy().into_owned(),
        ))
        .unwrap()
}

/// Build the minimal rendering graph and prepare the renderer for it.
fn ready_graph(
    renderer: &mut VulkanRenderer,
    material: MaterialHandle,
) -> FrameGraph<VulkanRenderer> {
    renderer
        .init_light_culling(WIDTH, HEIGHT, &shaders().join("lighting/light_cull.wgsl"))
        .unwrap();
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();
    FrameGraphBuilder::new()
        .add_pass(
            GeometryPass::new("geometry")
                .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb)
                .clear_color([0.0, 0.0, 0.0, 1.0])
                .material(material),
        )
        .build::<VulkanRenderer>()
        .unwrap()
}

/// Render `count` frames with no submissions so retirement ages advance.
fn render_idle_frames(
    renderer: &mut VulkanRenderer,
    graph: &mut FrameGraph<VulkanRenderer>,
    uniforms: &FrameUniforms,
    count: usize,
) {
    for _ in 0..count {
        renderer.wait_for_frame().unwrap();
        renderer.set_frame_uniforms(uniforms.clone());
        renderer.render(graph, |_| {}).expect("empty frame render");
    }
}

fn triangle_mesh(renderer: &mut VulkanRenderer) -> MeshHandle {
    let vertices = [
        katla_gfx::VertexPBR::from_position([-0.5, -0.5, 0.0]),
        katla_gfx::VertexPBR::from_position([0.5, -0.5, 0.0]),
        katla_gfx::VertexPBR::from_position([0.0, 0.5, 0.0]),
    ];
    renderer
        .create_mesh(&vertices, &[0u32, 1, 2], PrimitiveTopology::TriangleList)
        .unwrap()
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_destroy_texture_retires_image_and_withholds_bindless_slot() {
    let mut renderer = headless_renderer();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured = errors.clone();
    renderer
        .context()
        .set_validation_callback(move |message, level| {
            if level == katla_gfx::ValidationLevel::Error {
                captured.lock().unwrap().push(message.to_owned());
            }
        });

    let material = compile_ui_material(&mut renderer);
    let mut graph = ready_graph(&mut renderer, material);
    let frame = uniforms();

    // A registered texture occupies a bindless slot.
    let first = renderer.create_texture_solid([255, 0, 0, 255]).unwrap();
    let first_slot = renderer.get_bindless_slot(first).expect("registered slot");
    let (_, available_before, _) = renderer.get_bindless_stats();

    // Destroy mid-flight: the handle dies now, the native image and the
    // slot stay alive for in-flight submissions.
    renderer.destroy_texture(first);
    assert!(
        renderer.get_bindless_slot(first).is_none(),
        "handle must invalidate immediately"
    );
    let snapshot = renderer.pending_retirements();
    assert_eq!(snapshot.textures, 1, "{snapshot:?}");
    assert_eq!(snapshot.bindless_slots, 1, "{snapshot:?}");
    let (_, available_after_destroy, _) = renderer.get_bindless_stats();
    assert_eq!(
        available_after_destroy, available_before,
        "the destroyed texture's slot must stay withheld"
    );

    // A new texture can never take the withheld slot.
    let second = renderer.create_texture_solid([0, 0, 255, 255]).unwrap();
    let second_slot = renderer.get_bindless_slot(second).expect("registered slot");
    assert_ne!(
        second_slot, first_slot,
        "a pending-retirement slot must not be reused"
    );
    let (_, available_with_second, _) = renderer.get_bindless_stats();

    // Frames advance the retirement age; the image frees and the slot is
    // released only then.
    render_idle_frames(&mut renderer, &mut graph, &frame, DRAIN_FRAMES);
    assert_eq!(
        renderer.pending_retirements().total(),
        0,
        "{:?}",
        renderer.pending_retirements()
    );
    let (_, available_after_drain, _) = renderer.get_bindless_stats();
    assert_eq!(
        available_after_drain,
        available_with_second + 1,
        "exactly the withheld slot must return to the free list"
    );

    // After expiry the slot is allocatable again.
    let third = renderer.create_texture_solid([0, 255, 0, 255]).unwrap();
    let third_slot = renderer.get_bindless_slot(third).expect("registered slot");
    assert_eq!(third_slot, first_slot, "freed slot must be reusable");

    graph.cleanup();
    drop(graph);
    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_destroy_material_and_skeleton_retire_native_objects() {
    let mut renderer = headless_renderer();
    // The graph keeps its own material alive; the victim material is
    // destroyed mid-flight while frames still render through the graph.
    let graph_material = compile_ui_material(&mut renderer);
    let mut graph = ready_graph(&mut renderer, graph_material);
    let frame = uniforms();

    let victim = compile_ui_material(&mut renderer);
    renderer.destroy_material(victim);
    assert!(
        renderer.asset_registry.get_material(victim).is_none(),
        "handle must invalidate immediately"
    );
    let snapshot = renderer.pending_retirements();
    assert!(
        snapshot.pipelines >= 1,
        "destroyed material pipelines must retire: {snapshot:?}"
    );

    let skeleton = renderer.create_skeleton(4).unwrap();
    assert!(renderer.get_skeleton_descriptor(skeleton).is_some());
    renderer.destroy_skeleton(skeleton);
    assert!(renderer.get_skeleton_descriptor(skeleton).is_none());
    let snapshot = renderer.pending_retirements();
    assert_eq!(snapshot.skeleton_buffers, 1, "{snapshot:?}");

    render_idle_frames(&mut renderer, &mut graph, &frame, DRAIN_FRAMES);
    assert_eq!(renderer.pending_retirements().total(), 0);

    graph.cleanup();
    drop(graph);
    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_repeated_create_destroy_keeps_retirements_bounded_and_valid() {
    let mut renderer = headless_renderer();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured = errors.clone();
    renderer
        .context()
        .set_validation_callback(move |message, level| {
            if level == katla_gfx::ValidationLevel::Error {
                captured.lock().unwrap().push(message.to_owned());
            }
        });

    let material = compile_ui_material(&mut renderer);
    let mut graph = ready_graph(&mut renderer, material);
    let frame = uniforms();

    // Interleave creation and destruction of every resource class across
    // rendered frames; pending retirements must stay bounded and drain
    // completely once frames stop destroying.
    for iteration in 0..12 {
        let mesh = triangle_mesh(&mut renderer);
        let texture = renderer.create_texture_solid([255, 0, 0, 255]).unwrap();
        let skeleton = renderer.create_skeleton(4).unwrap();
        let iter_material = compile_ui_material(&mut renderer);

        renderer.wait_for_frame().unwrap();
        renderer.set_frame_uniforms(frame.clone());
        renderer.render(&mut graph, |_| {}).unwrap();

        // Destroy this iteration's resources; the next iteration's
        // replacements must not collide with any in-flight native object.
        renderer.destroy_mesh(mesh);
        renderer.destroy_texture(texture);
        renderer.destroy_skeleton(skeleton);
        renderer.destroy_material(iter_material);

        let snapshot = renderer.pending_retirements();
        assert!(
            snapshot.total() < 64,
            "retirements must stay bounded across iterations: {snapshot:?}"
        );
        let _ = iteration;
    }

    render_idle_frames(&mut renderer, &mut graph, &frame, DRAIN_FRAMES);
    let snapshot = renderer.pending_retirements();
    assert_eq!(
        snapshot.total(),
        0,
        "everything must retire once submissions completed: {snapshot:?}"
    );

    graph.cleanup();
    drop(graph);
    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
