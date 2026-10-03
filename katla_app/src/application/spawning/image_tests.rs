//! Native image fidelity, filtered mip chains and weak import sharing acceptance.

use super::*;

#[test]
#[ignore = "requires native Vulkan or Metal texture filtering and upload retirement"]
fn test_native_import_image_precision_mips_and_shared_lifetimes() {
    let fixture = Fixture::new();
    let source = r#"
@group(0) @binding(0) var<uniform> params: vec4f;
@group(1) @binding(0) var textures: binding_array<texture_2d<f32>,4096>;
@group(1) @binding(1) var image_sampler: sampler;
@vertex fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4f {
 let corners=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));return vec4f(corners[vertex],0,1);
}
@fragment fn fs_main() -> @location(0) vec4f {
 if (params.w > 0.5) {return textureSampleLevel(textures[u32(params.x)],image_sampler,vec2f(0.5),params.y);}
 return textureLoad(textures[u32(params.x)],vec2i(i32(params.z),0),i32(params.y));
}
"#;
    std::fs::write(fixture.0.join("probe.wgsl"), source).unwrap();
    let import_shader = r#"
@vertex fn vs_main(@location(0) position: vec3f)->@builtin(position) vec4f {return vec4f(position,1);}
@fragment fn fs_main()->@location(0) vec4f {return vec4f(1);}
"#;
    std::fs::write(fixture.0.join("model_pbr.wgsl"), import_shader).unwrap();
    let mut app = ApplicationBuilder::new()
        .validation_layer(true)
        .with_frame_graph(|renderer, _| Ok(ApplicationFrameGraph::new(empty_frame_graph(renderer))))
        .build_headless(1, String::new())
        .unwrap();
    let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    #[cfg(not(target_os = "macos"))]
    {
        let crate::Renderer::Vulkan(renderer) = &app.renderer;
        assert!(renderer.context().validation_active());
        let captured = errors.clone();
        renderer
            .context()
            .set_validation_callback(move |message, level| {
                if level == katla_gfx::ValidationLevel::Error {
                    captured.lock().unwrap().push(message.into());
                }
            });
    }
    #[cfg(target_os = "macos")]
    {
        assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
        assert_eq!(
            std::env::var("METAL_DEVICE_WRAPPER_TYPE").as_deref(),
            Ok("1")
        );
    }
    app.resources.shaders = fixture.0.clone();
    let pixels = [
        255u8, 0, 0, 255, 128, 128, 128, 128, 0, 0, 255, 0, 0, 0, 0, 0,
    ];
    image::save_buffer(
        fixture.0.join("image.png"),
        &pixels,
        2,
        2,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.0.join("mesh.gltf")).unwrap()).unwrap();
    let mut buffer = std::fs::read(fixture.0.join("mesh.bin")).unwrap();
    buffer.extend_from_slice(bytemuck::cast_slice(&[[0f32, 0.], [1., 0.], [0.5, 1.]]));
    std::fs::write(fixture.0.join("mesh.bin"), buffer).unwrap();
    document["buffers"][0]["byteLength"] = serde_json::json!(60);
    document["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"buffer":0,"byteOffset":36,"byteLength":24}));
    document["accessors"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}));
    document["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = serde_json::json!(1);
    document["images"] = serde_json::json!([{"uri":"image.png"}]);
    document["textures"] = serde_json::json!([{"source":0}]);
    document["materials"][1] = serde_json::json!({"pbrMetallicRoughness":{"baseColorTexture":{"index":0},"metallicRoughnessTexture":{"index":0}},"normalTexture":{"index":0},"occlusionTexture":{"index":0},"emissiveTexture":{"index":0}});
    std::fs::write(
        fixture.0.join("mesh.gltf"),
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
    let asset = fixture.0.join("mesh.gltf");
    let first = app.spawn_gltf_model(&asset, [0.; 3], None).unwrap();
    let second = app.spawn_gltf_model(&asset, [0.; 3], None).unwrap();
    assert_eq!(app.gpu_resource_tracker.texture_count(), 2);
    let first_material = app
        .world
        .get_component::<DrawableComponent>(first)
        .unwrap()
        .material_handle;
    let second_material = app
        .world
        .get_component::<DrawableComponent>(second)
        .unwrap()
        .material_handle;
    assert_ne!(first_material, second_material);
    let textures = app.renderer.material_textures(first_material).unwrap();
    assert_eq!(
        textures,
        app.renderer.material_textures(second_material).unwrap()
    );
    assert_ne!(
        textures.albedo, textures.normal,
        "color and data transfer functions need separate images"
    );
    assert_eq!(textures.normal, textures.metallic_roughness);
    assert_eq!(textures.normal, textures.occlusion);
    assert_eq!(
        textures.albedo,
        app.world
            .get_component::<DrawableComponent>(first)
            .unwrap()
            .emission
    );
    let first_handles = app
        .world
        .get_component::<super::super::ModelTextures>(first)
        .unwrap()
        .handles
        .clone();
    assert_eq!(
        first_handles.len(),
        2,
        "one reference per drawable and unique image upload"
    );
    let original = textures;
    let material = app
        .renderer
        .compile_material(
            &PipelineDescriptor::simple(fixture.0.join("probe.wgsl").to_string_lossy())
                .with_vertex_layout(VertexLayout::empty())
                .with_depth(DepthState::disabled())
                .with_depth_format(None)
                .with_cull(CullMode::None)
                .with_color_format(ImageFormat::R16G16B16A16Sfloat),
        )
        .unwrap();
    let builder = FrameGraphBuilder::new()
        .create_resource(katla_gfx::render_graph::GraphResourceDesc {
            name: "result".into(),
            resource_type: katla_gfx::render_graph::GraphResourceType::ColorAttachment {
                clear_value: None,
            },
            format: ImageFormat::R16G16B16A16Sfloat,
            width: 4,
            height: 4,
            tracks_swapchain_size: false,
        })
        .add_pass(
            GeometryPass::new("probe")
                .without_depth()
                .write_color("result", ImageFormat::R16G16B16A16Sfloat),
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
    let color = 0.21586f32;
    for (texture, level, x, filtered, expected, tolerance) in [
        (
            textures.albedo,
            0.,
            1.,
            false,
            [color, color, color, 128. / 255.],
            0.001,
        ),
        (textures.normal, 0., 1., false, [128. / 255.; 4], 0.001),
        (
            textures.albedo,
            1.,
            0.,
            true,
            [
                (1. + color) / 4.,
                color / 4.,
                (1. + color) / 4.,
                (1. + 128. / 255.) / 4.,
            ],
            0.004,
        ),
        (
            textures.normal,
            1.,
            0.,
            true,
            [
                (1. + 128. / 255.) / 4.,
                128. / 255. / 4.,
                (1. + 128. / 255.) / 4.,
                (1. + 128. / 255.) / 4.,
            ],
            0.003,
        ),
    ] {
        let pixel = probe(&mut app, &mut graph, material, texture, level, x, filtered);
        assert_float_pixel(pixel, expected, tolerance);
    }
    for (format, pixels, srgb, expected, tolerance) in [
        (
            gltf::image::Format::R16G16B16A16,
            [64u16, 129, 65535, 32768]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect::<Vec<_>>(),
            false,
            [64. / 65535., 129. / 65535., 1., 32768. / 65535.],
            0.0003,
        ),
        (
            gltf::image::Format::R16G16B16A16,
            [32768u16, 16384, 65535, 32768]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect(),
            true,
            [0.21405, 0.05088, 1., 0.5],
            0.0003,
        ),
        (
            gltf::image::Format::R32G32B32A32FLOAT,
            [8f32, 0.5, -2., 0.25]
                .into_iter()
                .flat_map(f32::to_ne_bytes)
                .collect(),
            true,
            [8., 0.5, -2., 0.25],
            0.,
        ),
    ] {
        let image = gltf::image::Data {
            pixels,
            format,
            width: 1,
            height: 1,
        };
        let texture = app.upload_gltf_image(&image, srgb).unwrap();
        assert_float_pixel(
            probe(&mut app, &mut graph, material, texture, 0., 0., false),
            expected,
            tolerance,
        );
        app.renderer.destroy_texture(texture);
    }
    let hdr = gltf::image::Data {
        pixels: [
            8f32, 2., 4., 1., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.,
        ]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect(),
        format: gltf::image::Format::R32G32B32A32FLOAT,
        width: 2,
        height: 2,
    };
    let texture = app.upload_gltf_image(&hdr, true).unwrap();
    assert_float_pixel(
        probe(&mut app, &mut graph, material, texture, 1., 0., true),
        [2., 0.5, 1., 0.25],
        0.,
    );
    let (descriptor, pixels) = crate::util::gltf_image::texture_upload(
        &gltf::image::Data {
            pixels: vec![0; 16],
            format: gltf::image::Format::R8G8B8A8,
            width: 2,
            height: 2,
        },
        false,
    )
    .unwrap();
    let updated = app.renderer.create_texture(&descriptor, &pixels).unwrap();
    assert_float_pixel(
        probe(&mut app, &mut graph, material, updated, 1., 0., true),
        [0.; 4],
        0.,
    );
    app.renderer.update_texture(updated, &[255; 16]).unwrap();
    assert_float_pixel(
        probe(&mut app, &mut graph, material, updated, 1., 0., true),
        [1.; 4],
        0.,
    );
    app.renderer.destroy_texture(updated);
    app.renderer.destroy_texture(texture);
    #[cfg(feature = "editor")]
    {
        crate::application::editor::record_entity_gpu_handles(&mut app, first);
        app.world.destroy_entity(first);
        crate::application::editor::process_gpu_cleanup_for_destroyed_entities(&mut app);
        assert_eq!(app.gpu_resource_tracker.texture_count(), 2);
        assert_float_pixel(
            probe(
                &mut app,
                &mut graph,
                material,
                original.albedo,
                0.,
                1.,
                false,
            ),
            [color, color, color, 128. / 255.],
            0.001,
        );
    }
    let scene = crate::scene::SceneManager::save_scene(&mut app).unwrap();
    crate::scene::SceneManager::load_scene(&mut app, scene).unwrap();
    assert_eq!(app.gpu_resource_tracker.texture_count(), 2);
    let live = app
        .world
        .query_ref::<&DrawableComponent>()
        .next()
        .unwrap()
        .1;
    assert_eq!(
        app.renderer
            .material_textures(live.material_handle)
            .unwrap(),
        original,
        "scene reconstruction shares still-live immutable uploads"
    );
    crate::scene::SceneManager::load_scene(&mut app, crate::scene::Scene::new("Empty")).unwrap();
    for handle in first_handles {
        assert!(app.renderer.get_bindless_slot(handle).is_none());
    }
    let next = app.spawn_gltf_model(&asset, [0.; 3], None).unwrap();
    let next_material = app
        .world
        .get_component::<DrawableComponent>(next)
        .unwrap()
        .material_handle;
    let replacement = app.renderer.material_textures(next_material).unwrap();
    assert_ne!(
        replacement.albedo, original.albedo,
        "weak cache must reject dead generational handles"
    );
    assert_ne!(replacement.normal, original.normal);
    assert_float_pixel(
        probe(
            &mut app,
            &mut graph,
            material,
            replacement.albedo,
            1.,
            0.,
            true,
        ),
        [
            (1. + color) / 4.,
            color / 4.,
            (1. + color) / 4.,
            (1. + 128. / 255.) / 4.,
        ],
        0.004,
    );
    graph.cleanup();
    app.renderer.destroy();
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "native validation errors: {errors:?}");
}

fn probe(
    app: &mut crate::application::Application,
    graph: &mut crate::FrameGraph,
    material: katla_gfx::MaterialHandle,
    texture: katla_gfx::TextureHandle,
    level: f32,
    x: f32,
    filtered: bool,
) -> [f32; 4] {
    let params = [
        app.renderer.get_texture_bindless_index(texture) as f32,
        level,
        x,
        f32::from(filtered),
    ];
    graph
        .set_pass_bindings(
            graph.pass_id("probe").unwrap(),
            PassBindings {
                constants: vec![ConstantBinding {
                    group: 0,
                    binding: 0,
                    stages: ShaderStages::FRAGMENT,
                    bytes: bytemuck::cast_slice(&params).to_vec(),
                }],
                phases: vec![PassDrawPhase {
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
    let katla_gfx::FrameAcquisition::Ready(frame) = app.renderer.acquire_frame().unwrap() else {
        panic!("native frame")
    };
    app.renderer.render(&frame, graph, |_| {}).unwrap();
    app.renderer.present(frame).unwrap();
    let source = app
        .renderer
        .graph_texture_source(graph.resource_id("result").unwrap())
        .unwrap();
    let ticket = app
        .renderer
        .queue_texture_readback(source, TextureReadbackRegion::pixel(0, 0))
        .unwrap();
    app.renderer.wait_for_device();
    let result = app.renderer.poll_texture_readback(ticket).unwrap().unwrap();
    assert_eq!(result.format, ImageFormat::R16G16B16A16Sfloat);
    let values: Vec<_> = result
        .bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|value| half::f16::from_bits(u16::from_ne_bytes(*value)).to_f32())
        .collect();
    values.try_into().unwrap()
}

fn assert_float_pixel(actual: [f32; 4], expected: [f32; 4], tolerance: f32) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() <= tolerance),
        "{actual:?} expected {expected:?} ± {tolerance}"
    );
}
