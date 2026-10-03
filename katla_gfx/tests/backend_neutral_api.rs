//! A custom graph built from the portable device and pass APIs.
//!
//! Vulkan is selected only by the constructor and graph type parameter. The
//! shader, bindings, frame ownership and readback use backend-neutral types.

use std::ffi::CString;
use std::path::PathBuf;

use katla_gfx::render_graph::{FrameGraph, FrameGraphBuilder, GeometryPass};
use katla_gfx::renderer::frame_scope::{FrameAcquisition, FrameToken};
use katla_gfx::{
    ConstantBinding, CullMode, DepthState, GpuRenderer, ImageFormat, PassBindings, PassDraw,
    PassDrawPhase, PassPipeline, PipelineDescriptor, ShaderStages, Size2D, TextureReadbackRegion,
    ValidationMode, VertexLayout, VulkanRenderer,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;
const SHADER: &str = r#"
@group(0) @binding(0) var<uniform> tint: vec4f;

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4f {
    let corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    return vec4f(corners[vertex], 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4f {
    return tint;
}
"#;

struct ShaderFile(PathBuf);

impl ShaderFile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "katla-backend-neutral-api-{}.wgsl",
            std::process::id()
        ));
        std::fs::write(&path, SHADER).unwrap();
        Self(path)
    }
}

impl Drop for ShaderFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn acquire(renderer: &mut VulkanRenderer) -> FrameToken {
    match renderer.acquire_frame().unwrap() {
        FrameAcquisition::Ready(token) => token,
        other => panic!("headless device must acquire a frame, got {other:?}"),
    }
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_representative_frame_compiles_and_renders_portably() {
    let mut renderer = VulkanRenderer::init_headless(
        WIDTH,
        HEIGHT,
        ValidationMode::Enabled,
        CString::new("Backend-neutral API test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap();
    let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = errors.clone();
    renderer
        .context()
        .set_validation_callback(move |message, level| {
            if level == katla_gfx::ValidationLevel::Error {
                captured.lock().unwrap().push(message.to_owned());
            }
        });
    let shader = ShaderFile::new();
    let descriptor = PipelineDescriptor::simple(shader.0.to_string_lossy())
        .with_vertex_layout(VertexLayout::empty())
        .with_color_format(ImageFormat::B8G8R8A8Srgb)
        .with_depth(DepthState::disabled())
        .with_depth_format(None)
        .with_cull(CullMode::None);
    let material = renderer.compile_material(&descriptor).unwrap();
    let mut graph: FrameGraph<VulkanRenderer> = FrameGraphBuilder::new()
        .add_pass(
            GeometryPass::new("custom_triangle")
                .without_depth()
                .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb)
                .material(material),
        )
        .export_resource("backbuffer")
        .build::<VulkanRenderer>()
        .unwrap();
    let pass = graph.pass_id("custom_triangle").unwrap();
    for (color, expected) in [
        ([1.0f32, 0.0, 0.0, 1.0], [0, 0, 255, 255]),
        ([0.0, 1.0, 0.0, 1.0], [0, 255, 0, 255]),
        ([0.0, 0.0, 1.0, 1.0], [255, 0, 0, 255]),
        ([1.0, 1.0, 1.0, 1.0], [255, 255, 255, 255]),
    ] {
        let packet = PassBindings {
            constants: vec![ConstantBinding {
                group: 0,
                binding: 0,
                stages: ShaderStages::FRAGMENT,
                bytes: color.into_iter().flat_map(f32::to_ne_bytes).collect(),
            }],
            phases: vec![PassDrawPhase {
                samplers: Vec::new(),
                pipelines: vec![PassPipeline {
                    vertex_layout: VertexLayout::empty(),
                    material,
                }],
                constants: vec![],
                draw: PassDraw::Vertices {
                    count: 3,
                    instances: 1,
                },
                viewport: None,
            }],
            ..PassBindings::default()
        };
        graph.set_pass_bindings(pass, packet.clone()).unwrap();
        let mut invalid = packet;
        invalid.phases[0].viewport = Some(katla_gfx::Rect::new([0.0, 0.0], [0.0, HEIGHT as f32]));
        assert!(graph.set_pass_bindings(pass, invalid).is_err());
        let frame = acquire(&mut renderer);
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        assert_eq!(
            renderer
                .present(frame)
                .unwrap()
                .surface
                .expect("surface presentation"),
            katla_gfx::SurfaceStatus::Presented
        );

        let resource = graph.resource_id("backbuffer").unwrap();
        let source = renderer
            .graph_texture_source(resource)
            .expect("committed graph export");
        let ticket = renderer
            .queue_texture_readback(
                source,
                TextureReadbackRegion {
                    origin: [0, 0],
                    size: Size2D::new(WIDTH, HEIGHT),
                    mip_level: 0,
                    array_layer: 0,
                },
            )
            .unwrap();
        renderer.wait_for_device();
        let result = renderer
            .poll_texture_readback(ticket)
            .unwrap()
            .expect("readback completes");
        assert_eq!(result.format, ImageFormat::B8G8R8A8Srgb);
        assert_eq!(result.size, Size2D::new(WIDTH, HEIGHT));
        assert_eq!(result.bytes.len(), (WIDTH * HEIGHT * 4) as usize);
        assert!(
            result
                .bytes
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == expected)
        );
    }
    graph.cleanup();
    renderer.destroy();
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_committed_texture_sources_and_tickets_reject_other_devices() {
    let create_device = || {
        VulkanRenderer::init_headless(
            WIDTH,
            HEIGHT,
            ValidationMode::Disabled,
            CString::new("Readback source ownership").unwrap(),
            CString::new("Katla").unwrap(),
        )
        .unwrap()
    };
    let create_graph = || {
        FrameGraphBuilder::new()
            .add_pass(
                GeometryPass::new("clear")
                    .without_depth()
                    .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb),
            )
            .export_resource("backbuffer")
            .build::<VulkanRenderer>()
            .unwrap()
    };
    let mut first = create_device();
    let mut second = create_device();
    let mut first_graph = create_graph();
    let mut second_graph = create_graph();
    let first_frame = acquire(&mut first);
    let second_frame = acquire(&mut second);
    assert_eq!(first_frame.slot(), second_frame.slot());
    assert_ne!(first_frame, second_frame);
    assert!(
        second
            .execute_draw_calls(&first_frame, &katla_gfx::DrawList::new())
            .is_err()
    );
    assert!(
        second
            .render(&first_frame, &mut second_graph, |_| {})
            .is_err()
    );
    assert!(second.present(first_frame).is_err());
    first
        .render(&first_frame, &mut first_graph, |_| {})
        .unwrap();
    assert_eq!(
        first
            .present(first_frame)
            .unwrap()
            .surface
            .expect("surface presentation"),
        katla_gfx::SurfaceStatus::Presented
    );
    let copied_second_frame = second_frame;
    second
        .render(&copied_second_frame, &mut second_graph, |_| {})
        .unwrap();
    assert_eq!(
        second
            .present(second_frame)
            .unwrap()
            .surface
            .expect("surface presentation"),
        katla_gfx::SurfaceStatus::Presented
    );
    let first_source = first
        .graph_texture_source(first_graph.resource_id("backbuffer").unwrap())
        .unwrap();
    let second_source = second
        .graph_texture_source(second_graph.resource_id("backbuffer").unwrap())
        .unwrap();
    assert_eq!(first_source.resource, second_source.resource);
    assert_eq!(first_source.frame_slot, second_source.frame_slot);
    assert_eq!(first_source.generation, second_source.generation);
    assert_eq!(first_source.submission, second_source.submission);
    assert_ne!(first_source.id, second_source.id);
    let region = TextureReadbackRegion::pixel(0, 0);
    assert!(second.queue_texture_readback(first_source, region).is_err());
    let ticket = first.queue_texture_readback(first_source, region).unwrap();
    assert!(second.poll_texture_readback(ticket).is_err());
    first.wait_for_device();
    let result = first.poll_texture_readback(ticket).unwrap().unwrap();
    assert_eq!(result.bytes, [0, 0, 0, 255]);
    first_graph.cleanup();
    second_graph.cleanup();
    first.destroy();
    second.destroy();
}
