//! Reproducible full-asset import timing with native texel acceptance.

use super::*;
use std::time::Instant;

#[test]
#[ignore = "measures native full-asset decoding, material registration and image uploads"]
fn test_native_full_material_asset_import_measurements() {
    let fixture = Fixture::new();
    let path = std::env::var_os("KATLA_MATERIAL_BENCHMARK_ASSET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../resources/models/DamagedHelmet.glb")
        });
    let path = std::fs::canonicalize(path).unwrap();
    let start = Instant::now();
    let decoded = crate::util::GLTFModel::new(&path).unwrap();
    let decode_us = start.elapsed().as_micros();
    assert_eq!(
        decoded.primitives.len(),
        1,
        "this paired benchmark uses one fully textured glTF primitive"
    );
    let image_index = decoded.primitives[0].material.base_color_texture.unwrap();
    let image = &decoded.images[image_index];
    let channels = match image.format {
        gltf::image::Format::R8G8B8 => 3,
        gltf::image::Format::R8G8B8A8 => 4,
        format => panic!("benchmark source must contain RGB8/RGBA8 albedo, got {format:?}"),
    };
    let linear = |value: u8| {
        let s = f32::from(value) / 255.;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let expected = [
        linear(image.pixels[0]),
        linear(image.pixels[1]),
        linear(image.pixels[2]),
        if channels == 4 {
            f32::from(image.pixels[3]) / 255.
        } else {
            1.
        },
    ];
    let image_bytes: usize = decoded.images.iter().map(|image| image.pixels.len()).sum();
    let image_extents: Vec<_> = decoded
        .images
        .iter()
        .map(|image| [image.width, image.height])
        .collect();
    let vertex_count = match &decoded.primitives[0].vertices {
        crate::util::GltfVertices::Static(v) => v.len(),
        crate::util::GltfVertices::Skinned(_) => panic!("static benchmark asset required"),
    };
    let index_count = decoded.primitives[0].indices.len();
    drop(decoded);
    // Full PBR compilation uses the documented Intel-driver workaround.
    let mut app = ApplicationBuilder::new()
        .validation_layer(false)
        .with_frame_graph(|renderer, _| Ok(ApplicationFrameGraph::new(empty_frame_graph(renderer))))
        .build_headless(1, String::new())
        .unwrap();
    app.renderer.wait_for_device();
    let start = Instant::now();
    let first = app.spawn_gltf_model(&path, [0.; 3], None).unwrap();
    let first_cpu_us = start.elapsed().as_micros();
    app.renderer.wait_for_device();
    let first_ready_us = start.elapsed().as_micros();
    let first_material = app
        .world
        .get_component::<DrawableComponent>(first)
        .unwrap()
        .material_handle;
    let first_texture = app
        .renderer
        .material_textures(first_material)
        .unwrap()
        .albedo;
    let start = Instant::now();
    let mut last = first;
    for _ in 0..7 {
        last = app.spawn_gltf_model(&path, [0.; 3], None).unwrap();
    }
    let warm_cpu_us = start.elapsed().as_micros();
    app.renderer.wait_for_device();
    let warm_ready_us = start.elapsed().as_micros();
    let last_material = app
        .world
        .get_component::<DrawableComponent>(last)
        .unwrap()
        .material_handle;
    let last_texture = app
        .renderer
        .material_textures(last_material)
        .unwrap()
        .albedo;
    let unique_texture_handles = app.gpu_resource_tracker.texture_count();
    assert_ne!(first_material, last_material);
    assert_eq!(app.gpu_resource_tracker.material_count(), 8);
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
    let mut native_pixels = Vec::new();
    for texture in [first_texture, last_texture] {
        let params = [
            app.renderer.get_texture_bindless_index(texture) as f32,
            0.,
            0.,
            0.,
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
        let katla_gfx::FrameAcquisition::Ready(frame) = app.renderer.acquire_frame().unwrap()
        else {
            panic!("native frame")
        };
        app.renderer.render(&frame, &mut graph, |_| {}).unwrap();
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
        let values: Vec<_> = result
            .bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|value| half_value(u16::from_ne_bytes(*value)))
            .collect();
        assert!(
            values
                .iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 0.001),
            "{values:?} expected {expected:?}"
        );
        native_pixels.push(result.bytes);
    }
    assert_eq!(native_pixels[0], native_pixels[1]);
    println!(
        "KATLA_MATERIAL_IMPORT {}",
        serde_json::json!({
            "asset":path,"asset_bytes":std::fs::metadata(&path).unwrap().len(),"image_bytes":image_bytes,"image_extents":image_extents,
            "vertex_count":vertex_count,"index_count":index_count,"instances":8,"decode_us":decode_us,
            "first_cpu_us":first_cpu_us,"first_ready_us":first_ready_us,"warm_cpu_us":warm_cpu_us,"warm_ready_us":warm_ready_us,
            "unique_texture_handles":unique_texture_handles,"native_rgba16f_texel":native_pixels[0],"validation_enabled":false,
        })
    );
    crate::scene::SceneManager::load_scene(&mut app, crate::scene::Scene::new("Empty")).unwrap();
    assert_eq!(app.gpu_resource_tracker.texture_count(), 0);
    graph.cleanup();
    app.renderer.destroy();
}

fn half_value(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1. } else { 1. };
    let exponent = (bits >> 10) & 31;
    let fraction = f32::from(bits & 1023) / 1024.;
    if exponent == 0 {
        sign * fraction * 2f32.powi(-14)
    } else {
        sign * (1. + fraction) * 2f32.powi(i32::from(exponent) - 15)
    }
}
