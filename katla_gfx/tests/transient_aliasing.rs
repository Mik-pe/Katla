//! Native physical aliasing and resize coverage through exact graph exports.
//!
//! The alias members have disjoint lifetimes. A final ordinary shader reads
//! the later member's last texel into the exported backbuffer. Native storage
//! observations prove both members share the same range in each frame slot;
//! queued exports prove each submitted slot produced the expected pixels.

use katla_gfx::TextureReadbackTicket;
use std::ffi::CString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use katla_gfx::render_graph::{
    FrameGraph, FrameGraphBuilder, GeometryPass, GraphResourceDesc, ImageSubresourceRange,
    RenderGraphBackend,
};
use katla_gfx::render_pass::{AttachmentOps, ClearValue};
use katla_gfx::texture::ImageFormat;
use katla_gfx::{
    CullMode, DepthState, GpuRenderer, GraphTextureSource, ImageBinding, PassBindings, PassDraw,
    PassDrawPhase, PipelineDescriptor, ShaderStages, TextureReadbackRegion, ValidationMode,
    VertexLayout, VulkanRenderer,
};

const BLUE: [u8; 4] = [255, 0, 0, 255];
const RED: [u8; 4] = [0, 0, 255, 255];

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

fn headless_renderer(label: &str) -> (VulkanRenderer, Arc<Mutex<Vec<String>>>) {
    let renderer = VulkanRenderer::init_headless(
        64,
        64,
        ValidationMode::Enabled,
        CString::new(label).unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap();
    assert!(
        renderer.context().validation_active(),
        "native Vulkan tests require active Khronos validation"
    );
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
        tracks_swapchain_size: true,
    }
}

/// `fill_a` clears mid_a to target-order BGRA red, then `fill_b` clears
/// mid_b to blue. The live intervals are disjoint, so the compiled plan
/// aliases the two into one physical slot per frame slot.
fn build_aliased_graph(renderer: &mut VulkanRenderer) -> FrameGraph<VulkanRenderer> {
    let mut descriptor = PipelineDescriptor::simple(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/support/alias_copy.wgsl")
            .to_string_lossy(),
    )
    .with_depth(DepthState::disabled())
    .with_depth_format(None)
    .with_cull(CullMode::None);
    descriptor.vertex = VertexLayout::empty();
    let material = renderer.compile_material(&descriptor).unwrap();
    let mut graph = FrameGraphBuilder::new()
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
        .add_pass(
            GeometryPass::new("read_last_b")
                .without_depth()
                .read("mid_b")
                .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb)
                .material(material),
        )
        .build::<VulkanRenderer>()
        .unwrap();
    let pass = graph.pass_id("read_last_b").unwrap();
    graph
        .set_pass_bindings(
            pass,
            PassBindings {
                images: vec![ImageBinding {
                    group: 2,
                    binding: 0,
                    resource: graph.resource_id("mid_b").unwrap(),
                    range: ImageSubresourceRange::WHOLE_COLOR,
                    stages: ShaderStages::FRAGMENT,
                }],
                phases: vec![PassDrawPhase {
                    draw: PassDraw::Vertices {
                        count: 3,
                        instances: 1,
                    },
                    pipelines: Vec::new(),
                    constants: Vec::new(),
                    viewport: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();
    graph
}

/// Queue the exact committed export before another presentation advances its source.
fn queue_pixel(
    renderer: &mut VulkanRenderer,
    source: GraphTextureSource,
    extent: katla_gfx::Size2D,
) -> TextureReadbackTicket {
    GpuRenderer::queue_texture_readback(
        renderer,
        source,
        TextureReadbackRegion::pixel(extent.width - 1, extent.height - 1),
    )
    .unwrap()
}

fn readback_pixel(renderer: &mut VulkanRenderer, ticket: TextureReadbackTicket) -> [u8; 4] {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(data) = GpuRenderer::poll_texture_readback(renderer, ticket).unwrap() {
            assert_eq!(data.format, ImageFormat::B8G8R8A8Srgb);
            assert_eq!(data.size, katla_gfx::Size2D::new(1, 1));
            return data.bytes.try_into().unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "queued texture copy did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_aliased_transients_share_storage_and_render_independently() {
    let (mut renderer, errors) = headless_renderer("Transient aliasing test");
    let mut graph = build_aliased_graph(&mut renderer);

    let diagnostics = graph.diagnostics().unwrap();
    assert_eq!(
        diagnostics.summary.physical_transient_allocations, 1,
        "the two disjoint transients must compile into one physical slot"
    );

    for cycle in 0..8 {
        if cycle > 0 {
            renderer.wait_for_device();
            let extent = 64 + cycle * 8;
            graph
                .recreate_transient_textures(&mut renderer, extent, extent)
                .unwrap();
        }
        let mut queued_slots = Vec::new();
        let mut sources = Vec::new();
        // Both Vulkan frame slots submit before either result is waited/read.
        for _ in 0..2 {
            let frame_token = acquire_frame_token(&mut renderer);
            let slot = frame_token.slot();
            assert!(!queued_slots.contains(&slot));
            queued_slots.push(slot);
            renderer.render(&frame_token, &mut graph, |_| {}).unwrap();
            assert_eq!(
                renderer
                    .present(frame_token)
                    .unwrap()
                    .surface
                    .expect("surface presentation"),
                katla_gfx::SurfaceStatus::Presented
            );
            let source = GpuRenderer::graph_texture_source(
                &renderer,
                graph.resource_id("backbuffer").unwrap(),
            )
            .unwrap();
            assert_eq!(source.frame_slot, slot);
            sources.push(queue_pixel(
                &mut renderer,
                source,
                katla_gfx::Size2D::new(64, 64),
            ));
        }
        assert_ne!(
            graph
                .transient_texture("mid_b", queued_slots[0])
                .unwrap()
                .image,
            graph
                .transient_texture("mid_b", queued_slots[1])
                .unwrap()
                .image,
            "in-flight frame slots must own distinct images"
        );
        let mut storage = Vec::new();
        for (frame_slot, source) in queued_slots.into_iter().zip(sources) {
            let texture = graph.transient_texture("mid_b", frame_slot).unwrap();
            let extent = 64 + cycle * 8;
            assert_eq!(
                (texture.extent.width, texture.extent.height),
                (extent, extent)
            );
            let a = <VulkanRenderer as RenderGraphBackend>::transient_allocation_info(
                graph.transient_texture("mid_a", frame_slot).unwrap(),
            )
            .unwrap();
            let b =
                <VulkanRenderer as RenderGraphBackend>::transient_allocation_info(texture).unwrap();
            assert_eq!(a.strategy, "vulkan_memory_alias");
            assert_eq!(b.strategy, "vulkan_memory_alias");
            assert_eq!(
                (a.identity, a.offset, a.bytes),
                (b.identity, b.offset, b.bytes),
                "both images must bind the same native range"
            );
            assert!(a.bytes >= u64::from(extent) * u64::from(extent) * 4);
            assert!(
                !storage.contains(&(b.identity, b.offset)),
                "in-flight slots need independent storage"
            );
            storage.push((b.identity, b.offset));
            assert_eq!(
                readback_pixel(&mut renderer, source),
                BLUE,
                "resize cycle {cycle}, slot {frame_slot}: actual later-alias last texel"
            );
        }
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
    let mut graph = build_aliased_graph(&mut renderer);
    // Textures initialize lazily at first render; the switch is observed
    // because it is set before that happens.
    graph.set_transient_aliasing(false).unwrap();
    graph.export_resource("mid_a").unwrap();
    graph.export_resource("mid_b").unwrap();

    let frame_token = acquire_frame_token(&mut renderer);
    let frame_slot = frame_token.slot();
    renderer.render(&frame_token, &mut graph, |_| {}).unwrap();
    assert_eq!(
        renderer
            .present(frame_token)
            .unwrap()
            .surface
            .expect("surface presentation"),
        katla_gfx::SurfaceStatus::Presented
    );

    let a = <VulkanRenderer as RenderGraphBackend>::transient_allocation_info(
        graph.transient_texture("mid_a", frame_slot).unwrap(),
    )
    .unwrap();
    let b = <VulkanRenderer as RenderGraphBackend>::transient_allocation_info(
        graph.transient_texture("mid_b", frame_slot).unwrap(),
    )
    .unwrap();
    assert_eq!(a.strategy, "vulkan_standalone");
    assert_eq!(b.strategy, "vulkan_standalone");
    assert_ne!(
        (a.identity, a.offset),
        (b.identity, b.offset),
        "disabled aliases must own distinct native ranges"
    );
    for (name, expected) in [("mid_a", RED), ("mid_b", BLUE)] {
        let source =
            GpuRenderer::graph_texture_source(&renderer, graph.resource_id(name).unwrap()).unwrap();
        let ticket = queue_pixel(&mut renderer, source, katla_gfx::Size2D::new(64, 64));
        assert_eq!(
            readback_pixel(&mut renderer, ticket),
            expected,
            "standalone {name} must retain its own last texel"
        );
    }

    graph.cleanup();
    renderer.destroy();
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
