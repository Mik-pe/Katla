//! Native independent UV attributes without changing joint or weight locations.

use super::*;
use crate::{
    CullMode, DepthState, ImageFormat, PipelineDescriptor, VertexLayout, VertexPBR,
    VertexPBRSkinned,
};

#[test]
fn test_native_static_and_skinned_secondary_uv_locations() {
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
    let vertices = [[-1f32, -1., 0.], [3., -1., 0.], [-1., 3., 0.]].map(|position| {
        let mut vertex = VertexPBR::new(position, [0., 0., 1.], [1., 0., 0., 1.], [0.25, 0.5]);
        vertex.tex_coord1 = [0.125, 0.5];
        vertex
    });
    let skinned =
        vertices.map(|vertex| VertexPBRSkinned::from_pbr(vertex, [4, 0, 0, 0], [0.5, 0.5, 0., 0.]));
    let meshes = [
        renderer
            .create_mesh(
                &vertices,
                &[0u32, 1, 2],
                crate::PrimitiveTopology::TriangleList,
            )
            .unwrap(),
        renderer
            .create_mesh(
                &skinned,
                &[0u32, 1, 2],
                crate::PrimitiveTopology::TriangleList,
            )
            .unwrap(),
    ];
    let path = std::env::temp_dir().join(format!(
        "katla-uv-attributes-{}.wgsl",
        crate::renderer::texture_readback::fresh_readback_id()
    ));
    for (index, (mesh, layout)) in meshes
        .into_iter()
        .zip([VertexLayout::pbr(), VertexLayout::pbr_skinned()])
        .enumerate()
    {
        let joint_fields = if index == 1 {
            "@location(4) joints:vec4u,@location(5) weights:vec4f,"
        } else {
            ""
        };
        let value = if index == 1 {
            "vec4f(in.uv0.x,in.uv0.y+f32(in.joints.x)*0.0625,in.uv1.x,in.uv1.y*in.weights.x)"
        } else {
            "vec4f(in.uv0,in.uv1)"
        };
        std::fs::write(&path,format!(r#"
struct Input {{ @location(0) position:vec3f,@location(3) uv0:vec2f,@location(6) uv1:vec2f,{joint_fields} }}
struct Output {{ @builtin(position) position:vec4f,@location(0) coordinates:vec4f }}
@vertex fn vs_main(in:Input)->Output {{ var out:Output;out.position=vec4f(in.position,1);out.coordinates={value};return out; }}
@fragment fn fs_main(in:Output)->@location(0) vec4f {{return in.coordinates;}}
"#)).unwrap();
        let material = renderer
            .compile_material(
                &PipelineDescriptor::simple(path.to_string_lossy())
                    .with_vertex_layout(layout)
                    .with_color_format(ImageFormat::R8G8B8A8Unorm)
                    .with_depth(DepthState::disabled())
                    .with_depth_format(None)
                    .with_cull(CullMode::None),
            )
            .unwrap();
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
                    .material(material)
                    .write_color("result", ImageFormat::R8G8B8A8Unorm),
            )
            .export_resource("result")
            .build::<NativeRenderer>()
            .unwrap();
        let pass = graph.pass_id("probe").unwrap();
        let mut draws = crate::renderer::DrawList::new();
        draws.push(crate::renderer::DrawCall::new(mesh, material));
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
        let ticket = renderer
            .queue_texture_readback(source, crate::TextureReadbackRegion::pixel(8, 8))
            .unwrap();
        renderer.wait_for_device();
        let pixel = renderer
            .poll_texture_readback(ticket)
            .unwrap()
            .unwrap()
            .bytes;
        let expected = if index == 1 {
            [64u8, 191, 32, 64]
        } else {
            [64, 128, 32, 128]
        };
        assert!(
            pixel
                .iter()
                .zip(expected)
                .all(|(actual, expected)| actual.abs_diff(expected) <= 1),
            "{pixel:?} != {expected:?}"
        );
        graph.cleanup();
        renderer.destroy_mesh(mesh);
        renderer.destroy_material(material);
    }
    std::fs::remove_file(path).unwrap();
    renderer.destroy();
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
