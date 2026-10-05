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
        let mut packet = PassBindings {
            constants: vec![ConstantBinding {
                group: 2,
                binding: 0,
                stages: ShaderStages::FRAGMENT,
                bytes: right.into_iter().flat_map(f32::to_ne_bytes).collect(),
            }],
            ..Default::default()
        };
        for draw in 0..300 {
            let is_left = draw < 150;
            packet.phases.push(PassDrawPhase {
                pipelines: vec![PassPipeline {
                    material,
                    vertex_layout: VertexLayout::empty(),
                }],
                constants: if is_left {
                    vec![ConstantBinding {
                        group: 2,
                        binding: 0,
                        stages: ShaderStages::FRAGMENT,
                        bytes: left.into_iter().flat_map(f32::to_ne_bytes).collect(),
                    }]
                } else {
                    Vec::new()
                },
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
            renderer.frame_resources[frame.slot()]
                .descriptors
                .allocated_sets(),
            0
        );
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        if index == 2 {
            renderer.abort(frame).unwrap();
            frame = acquire(&mut renderer);
            assert_eq!(
                renderer.frame_resources[frame.slot()]
                    .descriptors
                    .allocated_sets(),
                0
            );
            renderer.render(&frame, &mut graph, |_| {}).unwrap();
        }
        let arena = &renderer.frame_resources[frame.slot()].descriptors;
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

#[test]
#[ignore = "requires a Vulkan device"]
fn test_native_graphics_descriptor_snapshots_preserve_distinct_ui_passes() {
    use crate::render_graph::UIPass;
    use crate::vertex::{VertexUI, VertexUIInstance};
    use crate::{UIDrawList, UiDrawCommand};

    let mut renderer = VulkanRenderer::init_headless(
        16,
        16,
        ValidationMode::Enabled,
        c"UI snapshots".into(),
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
    let white = renderer.create_texture_solid([255; 4]).unwrap();
    let white_slot = renderer.get_bindless_slot(white).unwrap();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders/ui/ui.wgsl");
    let material = renderer
        .compile_material(&crate::PipelineDescriptor::ui(path.to_string_lossy()))
        .unwrap();
    let instanced = |screen_size, position, size, color| UIDrawList {
        screen_size,
        scale_factor: 1.0,
        instances: vec![VertexUIInstance {
            position,
            size,
            color,
            uv_min: [0.; 2],
            uv_max: [1.; 2],
            texture_index: white_slot,
            clip_rect: [0., 0., screen_size[0], screen_size[1]],
        }],
        commands: vec![UiDrawCommand::instanced(0, 1, None)],
        ..Default::default()
    };
    let vertices = |x, color| UIDrawList {
        screen_size: [16., 16.],
        scale_factor: 1.0,
        vertices: [[x, 10.], [x, 14.], [x + 4., 14.], [x + 4., 10.]]
            .into_iter()
            .map(|position| VertexUI::new(position, [0.; 2], color, white_slot))
            .collect(),
        indices: vec![0, 1, 2, 0, 2, 3],
        commands: vec![UiDrawCommand {
            offset: 0,
            count: 6,
            clip_rect: None,
            is_instanced: false,
        }],
        ..Default::default()
    };
    let mut mixed = vertices(10., [255, 255, 0, 255]);
    mixed.instances = instanced([16., 16.], [6., 7.], [4., 2.], [255, 0, 255, 255]).instances;
    mixed
        .commands
        .insert(0, UiDrawCommand::instanced(0, 1, None));
    mixed.commands.push(UiDrawCommand::instanced(0, 1, None));
    let draws = [
        instanced([16., 16.], [2., 2.], [4., 4.], [255, 0, 0, 255]),
        instanced([32., 16.], [20., 2.], [8., 4.], [0, 255, 0, 255]),
        vertices(2., [0, 0, 255, 255]),
        mixed,
    ];
    let mut builder = FrameGraphBuilder::new().add_pass(
        GeometryPass::new("clear")
            .without_depth()
            .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb),
    );
    for index in 0..draws.len() {
        builder = builder.add_pass(
            UIPass::new(format!("ui-{index}"))
                .write("backbuffer")
                .material(material),
        );
    }
    let mut graph = builder
        .export_resource("backbuffer")
        .build::<VulkanRenderer>()
        .unwrap();
    let passes: Vec<_> = (0..draws.len())
        .map(|index| graph.pass_id(&format!("ui-{index}")).unwrap())
        .collect();
    for pass in &passes {
        graph
            .set_pass_bindings(
                *pass,
                PassBindings {
                    samplers: vec![crate::SamplerBinding {
                        group: 0,
                        binding: 1,
                        stages: ShaderStages::FRAGMENT,
                        sampling: crate::SamplingMode::Linear,
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
    }
    for index in 0..4 {
        if index == 3 {
            renderer.resize(16, 16).unwrap();
        }
        let frame = acquire(&mut renderer);
        renderer
            .render(&frame, &mut graph, |frame| {
                for (pass, draw) in passes.iter().zip(&draws) {
                    frame.submit_ui(*pass, draw);
                }
            })
            .unwrap();
        assert_eq!(renderer.frame_resources[frame.slot()].uploaded_ranges(), 17);
        assert_eq!(
            renderer.frame_resources[frame.slot()]
                .descriptors
                .allocated_sets(),
            4
        );
        renderer.present(frame).unwrap().surface.unwrap();
        let source = renderer
            .graph_texture_source(graph.resource_id("backbuffer").unwrap())
            .unwrap();
        let pixels: Vec<_> = [
            (4, 4, [0, 0, 255, 255]),
            (12, 4, [0, 255, 0, 255]),
            (4, 12, [255, 0, 0, 255]),
            (12, 12, [0, 255, 255, 255]),
            (8, 8, [255, 0, 255, 255]),
        ]
        .into_iter()
        .map(|(x, y, expected)| {
            (
                renderer
                    .queue_texture_readback(source, crate::TextureReadbackRegion::pixel(x, y))
                    .unwrap(),
                expected,
            )
        })
        .collect();
        renderer.wait_for_device();
        for (ticket, expected) in pixels {
            assert_eq!(
                renderer
                    .poll_texture_readback(ticket)
                    .unwrap()
                    .unwrap()
                    .bytes,
                expected
            );
        }
    }
    graph.cleanup();
    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
