//! Native material import and texture sampling regressions.

use crate::application::ApplicationBuilder;
use crate::components::DrawableComponent;
use crate::{ApplicationFrameGraph, empty_frame_graph};
use katla_gfx::render_graph::{FrameGraphBuilder, GeometryPass};
use katla_gfx::{
    ConstantBinding, CullMode, DepthState, GpuRenderer, ImageFormat, PassBindings, PassDraw,
    PassDrawPhase, PassPipeline, PipelineDescriptor, ShaderStages, Size2D, TextureReadbackRegion,
    VertexLayout,
};

const SHADER: &str = r#"
@group(0) @binding(0) var<uniform> params: vec4f;
@group(1) @binding(0) var textures: binding_array<texture_2d<f32>, 4096>;
@vertex fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4f {
    let corners = array<vec2f, 3>(vec2f(-1.0,-1.0), vec2f(3.0,-1.0), vec2f(-1.0,3.0));
    return vec4f(corners[vertex], 0.0, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4f {
    let sample = textureLoad(textures[u32(params.x)], vec2i(0), 0);
    if (params.w > 0.0) {
        return vec4f(sample.b * params.y, sample.g * params.z, sample.r, 1.0);
    }
    return sample;
}
"#;

struct Fixture(std::path::PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "katla-material-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("probe.wgsl"), SHADER).unwrap();
        let positions: [f32; 9] = [-1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0];
        std::fs::write(path.join("mesh.bin"), bytemuck::cast_slice(&positions)).unwrap();
        std::fs::write(
            path.join("mesh.gltf"),
            r#"{
            "asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],
            "buffers":[{"uri":"mesh.bin","byteLength":36}],
            "bufferViews":[{"buffer":0,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3",
                          "min":[-1,-1,0],"max":[1,1,0]}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":1}]}],
            "materials":[{}, {"pbrMetallicRoughness":{
                "baseColorFactor":[0.2,0.4,0.6,0.8],"metallicFactor":0.75,"roughnessFactor":0.25}}]
        }"#,
        )
        .unwrap();
        Self(path)
    }
}

#[path = "alpha_tests.rs"]
mod alpha_tests;

#[path = "lighting_tests.rs"]
mod lighting_tests;

#[cfg(feature = "editor")]
#[path = "texture_authoring_tests.rs"]
mod texture_authoring_tests;

#[path = "image_tests.rs"]
mod image_tests;

#[path = "import_benchmark.rs"]
mod import_benchmark;

#[path = "primitive_tests.rs"]
mod primitive_tests;

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires native Vulkan or Metal texture sampling"]
fn test_native_material_defaults_import_factors_and_texture_color_spaces() {
    let fixture = Fixture::new();
    // The Intel PBR compiler crashes with the Vulkan validation layer enabled.
    // Metal API validation is enabled by the test process environment.
    let mut app = ApplicationBuilder::new()
        .validation_layer(false)
        .with_frame_graph(|renderer, _| Ok(ApplicationFrameGraph::new(empty_frame_graph(renderer))))
        .build_headless(1, String::new())
        .unwrap();
    let entity = app
        .spawn_gltf_model(fixture.0.join("mesh.gltf"), [0.0; 3], None)
        .unwrap();
    let drawable = app
        .world
        .get_component::<DrawableComponent>(entity)
        .unwrap();
    let color = drawable.color.unwrap();
    assert_eq!(
        [color.r, color.g, color.b, color.a],
        [0.2, 0.4, 0.6, 0.8],
        "glTF factors are already linear"
    );
    assert_eq!(drawable.metallic, 0.75);
    assert_eq!(drawable.roughness, 0.25);
    let metallic = drawable.metallic;
    let roughness = drawable.roughness;
    let defaults =
        crate::application::scene_features::create_material_textures(&mut app.renderer).unwrap();
    let image = gltf::image::Data {
        pixels: vec![128, 128, 128, 255],
        format: gltf::image::Format::R8G8B8A8,
        width: 1,
        height: 1,
    };
    let mut model = crate::util::GLTFModel::new(fixture.0.join("mesh.gltf")).unwrap();
    model.primitives[0].material.emission_texture =
        Some(crate::util::gltf_material::GltfTextureInfo {
            image_index: 0,
            sampling: Default::default(),
        });
    model.primitives[0].material.normal_texture =
        Some(crate::util::gltf_material::GltfTextureInfo {
            image_index: 0,
            sampling: Default::default(),
        });
    model.images.push(image);
    let upload = app.upload_gltf_textures(
        &fixture.0.join("mesh.gltf"),
        &model.images,
        &model.primitives[0].material,
    );
    assert_eq!(upload.handles.len(), 2);
    let color_texture = upload.emission;
    let data_texture = upload.textures.normal;
    let descriptor = PipelineDescriptor::simple(fixture.0.join("probe.wgsl").to_string_lossy())
        .with_vertex_layout(VertexLayout::empty())
        .with_depth(DepthState::disabled())
        .with_depth_format(None)
        .with_cull(CullMode::None)
        .with_color_format(ImageFormat::R8G8B8A8Unorm);
    let material = app.renderer.compile_material(&descriptor).unwrap();
    let builder = FrameGraphBuilder::new()
        .create_resource(katla_gfx::render_graph::GraphResourceDesc {
            name: "result".into(),
            resource_type: katla_gfx::render_graph::GraphResourceType::ColorAttachment {
                clear_value: None,
            },
            format: ImageFormat::R8G8B8A8Unorm,
            width: 8,
            height: 8,
            tracks_swapchain_size: false,
        })
        .add_pass(
            GeometryPass::new("probe")
                .without_depth()
                .write_color("result", ImageFormat::R8G8B8A8Unorm)
                .material(material),
        )
        .export_resource("result");
    #[cfg(not(target_os = "macos"))]
    let mut graph = katla_gfx::render_graph::any_frame_graph::AnyFrameGraph::from_vulkan(
        builder.build::<katla_gfx::VulkanRenderer>().unwrap(),
    );
    #[cfg(target_os = "macos")]
    let mut graph = katla_gfx::render_graph::any_frame_graph::AnyFrameGraph::from_metal(
        builder.build::<katla_gfx::MetalRenderer>().unwrap(),
    );
    let pass = graph.pass_id("probe").unwrap();
    for (texture, factors, expected) in [
        (
            defaults.metallic_roughness,
            [metallic, roughness, 1.0],
            [191u8, 64, 255, 255],
        ),
        (defaults.normal, [0.0; 3], [128, 128, 255, 255]),
        (color_texture, [0.0; 3], [55, 55, 55, 255]),
        (data_texture, [0.0; 3], [128, 128, 128, 255]),
    ] {
        let params = [
            app.renderer.get_bindless_slot(texture).unwrap() as f32,
            factors[0],
            factors[1],
            factors[2],
        ];
        graph
            .set_pass_bindings(
                pass,
                PassBindings {
                    constants: vec![ConstantBinding {
                        group: 0,
                        binding: 0,
                        stages: ShaderStages::FRAGMENT,
                        bytes: bytemuck::cast_slice(&params).to_vec(),
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
                    ..Default::default()
                },
            )
            .unwrap();
        #[cfg(target_os = "macos")]
        {
            let offscreen = app.renderer.create_offscreen_texture(
                crate::application::headless::HEADLESS_WIDTH,
                crate::application::headless::HEADLESS_HEIGHT,
            );
            app.renderer.set_headless_drawable(offscreen);
        }
        let token = match app.renderer.acquire_frame().unwrap() {
            katla_gfx::FrameAcquisition::Ready(token) => token,
            other => panic!("headless frame unavailable: {other:?}"),
        };
        app.renderer.render(&token, &mut graph, |_| {}).unwrap();
        app.renderer.present(token).unwrap().surface.unwrap();
        let source = app
            .renderer
            .graph_texture_source(graph.resource_id("result").unwrap())
            .unwrap();
        let ticket = app
            .renderer
            .queue_texture_readback(
                source,
                TextureReadbackRegion {
                    origin: [0; 2],
                    size: Size2D::new(1, 1),
                    mip_level: 0,
                    array_layer: 0,
                },
            )
            .unwrap();
        app.renderer.wait_for_device();
        let result = app.renderer.poll_texture_readback(ticket).unwrap().unwrap();
        assert_eq!(result.format, ImageFormat::R8G8B8A8Unorm);
        assert_eq!(result.bytes.len(), 4, "one RGBA8 texel must be returned");
        for (actual, expected) in result.bytes.iter().zip(expected) {
            assert!(
                actual.abs_diff(expected) <= 1,
                "texture {texture:?}: {:?} != {expected:?}",
                result.bytes
            );
        }
    }
    // Malformed optional textures retain NONE in a GraphOnly application and
    // must not acquire or track the renderer's generic white fallback.
    model.images[0] = gltf::image::Data {
        pixels: vec![0],
        format: gltf::image::Format::R8G8B8,
        width: 1,
        height: 1,
    };
    let upload = app.upload_gltf_textures(
        &fixture.0.join("malformed.gltf"),
        &model.images,
        &model.primitives[0].material,
    );
    assert!(upload.textures.normal.is_none());
    assert!(upload.handles.is_empty());
    graph.cleanup();
    app.renderer.destroy();
}
