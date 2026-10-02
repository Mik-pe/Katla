//! Native output and retirement evidence for reusable graphics descriptors.

use super::{ValidationMode, VulkanRenderer};
use crate::render_graph::{FrameGraphBuilder, GeometryPass};
use crate::renderer::frame_scope::{FrameAcquisition, FrameToken};
use crate::{
    ConstantBinding, GpuRenderer, ImageFormat, PassBindings, PassDraw, PassDrawPhase, PassPipeline,
    ShaderStages, VertexLayout,
};

fn acquire(renderer: &mut VulkanRenderer) -> FrameToken {
    match renderer.acquire_frame().unwrap() {
        FrameAcquisition::Ready(frame) => frame,
        other => panic!("headless acquisition: {other:?}"),
    }
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_native_graphics_descriptor_pools_reuse_slots_after_submit_abort_and_resize() {
    let mut renderer = VulkanRenderer::init_headless(
        8,
        8,
        ValidationMode::Enabled,
        c"descriptor reuse".into(),
        c"Katla".into(),
    )
    .unwrap();
    assert!(renderer.context.validation_active());
    let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = errors.clone();
    renderer
        .context
        .set_validation_callback(move |message, level| {
            if level == crate::ValidationLevel::Error {
                captured.lock().unwrap().push(message.to_owned());
            }
        });
    let path = std::env::temp_dir().join(format!(
        "katla-descriptor-reuse-{}.wgsl",
        super::texture_readback::fresh_readback_id()
    ));
    std::fs::write(
        &path,
        r#"
@group(2) @binding(0) var<uniform> tint:vec4f;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));
    return vec4f(p[i],0.,1.);
}
@fragment fn fs_main()->@location(0) vec4f { return tint; }
"#,
    )
    .unwrap();
    let descriptor = crate::PipelineDescriptor::simple(path.to_string_lossy())
        .with_vertex_layout(VertexLayout::empty())
        .with_color_format(ImageFormat::B8G8R8A8Srgb)
        .with_depth(crate::DepthState::disabled())
        .with_depth_format(None)
        .with_cull(crate::CullMode::None);
    let material = renderer.compile_material(&descriptor).unwrap();
    std::fs::remove_file(path).unwrap();
    let mut graph = FrameGraphBuilder::new()
        .add_pass(
            GeometryPass::new("draw")
                .without_depth()
                .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb),
        )
        .export_resource("backbuffer")
        .build::<VulkanRenderer>()
        .unwrap();
    let pass = graph.pass_id("draw").unwrap();
    let resource = graph.resource_id("backbuffer").unwrap();
    let mut tickets = Vec::new();
    let mut pool_handles = vec![None; super::FRAMES_IN_FLIGHT];
    for index in 0..6 {
        if index == 4 {
            renderer.resize(8, 8).unwrap();
        }
        let left = if index % 2 == 0 {
            [1.0f32, 0., 0., 1.]
        } else {
            [0., 1., 0., 1.]
        };
        let right = [0.0f32, 0., 1., 1.];
        let mut packet = PassBindings::default();
        for draw in 0..300 {
            let is_left = draw < 150;
            let color = if is_left { left } else { right };
            packet.phases.push(PassDrawPhase {
                pipelines: vec![PassPipeline {
                    material,
                    vertex_layout: VertexLayout::empty(),
                }],
                constants: vec![ConstantBinding {
                    group: 2,
                    binding: 0,
                    stages: ShaderStages::FRAGMENT,
                    bytes: color.into_iter().flat_map(f32::to_ne_bytes).collect(),
                }],
                draw: PassDraw::Vertices {
                    count: 3,
                    instances: 1,
                },
                viewport: Some(crate::Rect::new(
                    [if is_left { 0. } else { 4. }, 0.],
                    [if is_left { 4. } else { 8. }, 8.],
                )),
            });
        }
        graph.set_pass_bindings(pass, packet).unwrap();
        let mut frame = acquire(&mut renderer);
        assert_eq!(
            renderer.graphics_descriptors[frame.slot()].allocated_sets(),
            0
        );
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        if index == 2 {
            renderer.abort(frame).unwrap();
            frame = acquire(&mut renderer);
            assert_eq!(
                renderer.graphics_descriptors[frame.slot()].allocated_sets(),
                0
            );
            renderer.render(&frame, &mut graph, |_| {}).unwrap();
        }
        let arena = &renderer.graphics_descriptors[frame.slot()];
        assert_eq!(arena.allocated_sets(), 300);
        assert_eq!(arena.pool_count(), 3);
        let handles = arena.pool_handles();
        if let Some(previous) = &pool_handles[frame.slot()] {
            assert_eq!(&handles, previous);
        } else {
            pool_handles[frame.slot()] = Some(handles);
        }
        renderer.present(frame).unwrap().surface.unwrap();
        let source = renderer.graph_texture_source(resource).unwrap();
        for (x, expected) in [
            (
                1,
                if index % 2 == 0 {
                    [0, 0, 255, 255]
                } else {
                    [0, 255, 0, 255]
                },
            ),
            (6, [255, 0, 0, 255]),
        ] {
            tickets.push((
                renderer
                    .queue_texture_readback(source, crate::TextureReadbackRegion::pixel(x, 4))
                    .unwrap(),
                expected,
            ));
        }
    }
    renderer.wait_for_device();
    for (ticket, expected) in tickets {
        assert_eq!(
            renderer
                .poll_texture_readback(ticket)
                .unwrap()
                .unwrap()
                .bytes,
            expected
        );
    }
    graph.cleanup();
    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
