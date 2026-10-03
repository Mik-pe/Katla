//! Native draw-local image isolation, material fallback and stale-handle safety.

use super::*;
use crate::{
    CullMode, DepthState, ImageFormat, MaterialTextures, PassBindings, PassDraw, PassDrawPhase,
    PipelineDescriptor, TextureReadbackRegion, VertexLayout, VertexPosition,
};

#[test]
fn test_native_draw_texture_overrides_preserve_shared_materials_and_retirement() {
    #[cfg(target_os = "macos")]
    {
        assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
        assert_eq!(
            std::env::var("METAL_DEVICE_WRAPPER_TYPE").as_deref(),
            Ok("1")
        );
    }
    let mut renderer = renderer();
    let errors = capture_validation_errors(&renderer);
    let path = std::env::temp_dir().join(format!(
        "katla-draw-images-{}.wgsl",
        crate::renderer::texture_readback::fresh_readback_id()
    ));
    std::fs::write(
        &path,
        format!(
            r#"
{}
{}
@group(0) @binding(1) var<storage,read> objects:array<ObjectUniforms>;
struct Out {{ @builtin(position) position:vec4f,@location(0) @interpolate(flat) slot:u32 }}
@vertex fn vs_main(@location(0) position:vec3f,@builtin(instance_index) slot:u32)->Out {{
    var out:Out;out.position=vec4f(position,1.);out.slot=slot;return out;
}}
@fragment fn fs_main(in:Out)->@location(0) vec4f {{
    let indices=objects[in.slot].texture_indices;
    let uv=vec2f(.5);
    return vec4f(sample_texture(indices.x,uv).r,sample_texture(indices.y,uv).g,
                 sample_texture(indices.z,uv).b,sample_texture(indices.w,uv).r);
}}
"#,
            include_str!("../../../../resources/shaders/common/frame_uniforms.wgsl"),
            include_str!("../../../../resources/shaders/common/bindless.wgsl")
        ),
    )
    .unwrap();
    let material = renderer
        .compile_material(
            &PipelineDescriptor::simple(path.to_string_lossy())
                .with_vertex_layout(VertexLayout::position())
                .with_color_format(ImageFormat::R8G8B8A8Unorm)
                .with_depth(DepthState::disabled())
                .with_depth_format(None)
                .with_cull(CullMode::None),
        )
        .unwrap();
    let mesh = renderer
        .create_mesh(
            &[
                VertexPosition {
                    position: [-1., -1., 0.],
                },
                VertexPosition {
                    position: [3., -1., 0.],
                },
                VertexPosition {
                    position: [-1., 3., 0.],
                },
            ],
            &[0u32, 1, 2],
            crate::PrimitiveTopology::TriangleList,
        )
        .unwrap();
    let red = renderer.create_texture_solid([255, 0, 0, 255]).unwrap();
    let green = renderer.create_texture_solid([0, 255, 0, 255]).unwrap();
    let blue = renderer.create_texture_solid([0, 0, 255, 255]).unwrap();
    let white = renderer.default_texture();
    let inherited = MaterialTextures {
        albedo: green,
        normal: green,
        metallic_roughness: green,
        occlusion: green,
    };
    renderer.set_material_textures(material, inherited);
    let first = MaterialTextures {
        albedo: red,
        normal: blue,
        metallic_roughness: white,
        occlusion: red,
    };
    let second = MaterialTextures {
        albedo: blue,
        normal: green,
        metallic_roughness: red,
        occlusion: white,
    };
    let stale = MaterialTextures {
        albedo: red,
        normal: red,
        metallic_roughness: red,
        occlusion: red,
    };
    let mut graph = FrameGraphBuilder::new()
        .create_resource(GraphResourceDesc {
            name: "result".into(),
            resource_type: GraphResourceType::ColorAttachment { clear_value: None },
            format: ImageFormat::R8G8B8A8Unorm,
            width: 16,
            height: 16,
            tracks_swapchain_size: false,
        })
        .add_pass(
            GeometryPass::new("probe")
                .without_depth()
                .write_color("result", ImageFormat::R8G8B8A8Unorm),
        )
        .export_resource("result")
        .build::<NativeRenderer>()
        .unwrap();
    let pass = graph.pass_id("probe").unwrap();
    let mut replacement = None;
    for frame_index in 0..FRAME_SLOTS + 2 {
        let mut draws = crate::renderer::DrawList::new();
        let slots = [
            draws.push(crate::renderer::DrawCall::new(mesh, material).with_textures(first)),
            draws.push(
                crate::renderer::DrawCall::instanced(
                    mesh,
                    material,
                    vec![crate::renderer::InstanceData::default(); 2],
                )
                .with_textures(second),
            ),
            draws.push(crate::renderer::DrawCall::new(mesh, material)),
            draws.push(crate::renderer::DrawCall::new(mesh, material).with_textures(stale)),
        ];
        graph
            .set_pass_bindings(
                pass,
                PassBindings {
                    phases: slots
                        .iter()
                        .enumerate()
                        .map(|(index, slot)| PassDrawPhase {
                            pipelines: Vec::new(),
                            constants: Vec::new(),
                            samplers: Vec::new(),
                            draw: PassDraw::ObjectIndices(vec![*slot]),
                            viewport: Some(crate::Rect::new(
                                [(index * 4) as f32, 0.],
                                [(index * 4 + 4) as f32, 16.],
                            )),
                        })
                        .collect(),
                    ..Default::default()
                },
            )
            .unwrap();
        let frame = acquire(&mut renderer);
        GpuRenderer::execute_draw_calls(&mut renderer, &frame, &draws).unwrap();
        renderer
            .render(&frame, &mut graph, |context| {
                context.submit(pass, std::rc::Rc::new(draws));
            })
            .unwrap();
        renderer.present(frame).unwrap();
        let source = renderer
            .graph_texture_source(graph.resource_id("result").unwrap())
            .unwrap();
        let tickets: Vec<_> = (0..4)
            .map(|index| {
                renderer
                    .queue_texture_readback(source, TextureReadbackRegion::pixel(index * 4 + 2, 8))
                    .unwrap()
            })
            .collect();
        if frame_index == 0 {
            // Retire after accepted submission, then force a new handle generation.
            renderer.destroy_texture(red);
            replacement = Some(renderer.create_texture_solid([255, 255, 0, 255]).unwrap());
        }
        renderer.wait_for_device();
        let expected = if frame_index == 0 {
            [
                [255, 0, 255, 255],
                [0, 255, 0, 255],
                [0, 255, 0, 0],
                [255, 0, 0, 255],
            ]
        } else {
            [
                [255, 0, 255, 255],
                [0, 255, 255, 255],
                [0, 255, 0, 0],
                [255, 255, 255, 255],
            ]
        };
        for (index, ticket) in tickets.into_iter().enumerate() {
            let pixel = renderer
                .poll_texture_readback(ticket)
                .unwrap()
                .unwrap()
                .bytes;
            assert_eq!(pixel, expected[index], "frame {frame_index}, draw {index}");
        }
        assert_eq!(
            GpuRenderer::material_textures(&renderer, material),
            Some(inherited)
        );
    }
    graph.cleanup();
    renderer.destroy_mesh(mesh);
    renderer.destroy_material(material);
    for texture in [Some(green), Some(blue), replacement].into_iter().flatten() {
        renderer.destroy_texture(texture);
    }
    std::fs::remove_file(path).unwrap();
    renderer.destroy();
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}
