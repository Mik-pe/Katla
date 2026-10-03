//! Native image authoring, scoped history, scene reconstruction and weak ownership.

use super::*;
use crate::application::{Application, editor::material};
use crate::components::{NameComponent, TransformComponent};
use crate::scene::{AssetRef, SceneManager};
use katla_agent::{material::MaterialOp, material_sampling::TextureRole};
use serde_json::json;

#[test]
#[ignore = "requires native Vulkan or Metal material image readback"]
fn test_native_image_assignment_history_persistence_and_last_owner_retirement() {
    let fixture = Fixture::new();
    let source = r#"
struct Object {model:mat4x4f,color:vec4f,params:vec4f,textures:vec4u}
@group(0) @binding(1) var<storage,read> objects:array<Object>;
@group(0) @binding(3) var<uniform> mode:vec4u;
@group(1) @binding(0) var images:binding_array<texture_2d<f32>,4096>;
struct Out {@builtin(position) position:vec4f,@location(0) @interpolate(flat) slot:u32}
@vertex fn vs_main(@location(0) position:vec3f,@builtin(instance_index) slot:u32)->Out {
 var out:Out;out.position=vec4f(position,1);out.slot=slot;return out;
}
@fragment fn fs_main(in:Out)->@location(0) vec4f {
 let object=objects[in.slot];
 var index=u32(object.params.w);if mode.x<4u {index=object.textures[mode.x];}
 return textureLoad(images[index],vec2i(0),0);
}
"#;
    std::fs::write(fixture.0.join("model_pbr.wgsl"), source).unwrap();
    let mut bytes = std::fs::read(fixture.0.join("mesh.bin")).unwrap();
    bytes.extend_from_slice(bytemuck::cast_slice(&[[0f32; 2]; 3]));
    std::fs::write(fixture.0.join("mesh.bin"), bytes).unwrap();
    image::save_buffer(
        fixture.0.join("map.png"),
        &[128u8, 64, 255, 128].repeat(4),
        2,
        2,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.0.join("mesh.gltf")).unwrap()).unwrap();
    document["buffers"][0]["byteLength"] = json!(60);
    document["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":36,"byteLength":24}));
    document["accessors"]
        .as_array_mut()
        .unwrap()
        .push(json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}));
    document["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = json!(1);
    document["images"] = json!([{"uri":"map.png"}]);
    std::fs::write(
        fixture.0.join("mesh.gltf"),
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
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
    let first = app
        .spawn_gltf_model(fixture.0.join("mesh.gltf"), [0.; 3], None)
        .unwrap();
    let second = app
        .spawn_gltf_model(fixture.0.join("mesh.gltf"), [0.; 3], None)
        .unwrap();
    app.world.add_component(first, NameComponent::new("First"));
    app.world
        .add_component(second, NameComponent::new("Second"));
    let old_material = app
        .world
        .get_component::<DrawableComponent>(first)
        .unwrap()
        .material_handle;
    let original = app.renderer.material_textures(old_material).unwrap();
    let mut graph = graph(&mut app, &fixture);
    let assign = |app: &mut Application, ids: Vec<katla_ecs::EntityId>, role, source| {
        material::execute(
            app,
            MaterialOp::SetTexture {
                entity_ids: ids.into_iter().map(|id| id.id().to_string()).collect(),
                role,
                source,
            },
            true,
        )
    };
    let file = json!({"kind":"file","asset":AssetRef::File(fixture.0.join("map.png"))});
    assign(
        &mut app,
        vec![first, second],
        TextureRole::Albedo,
        file.clone(),
    )
    .unwrap();
    let handle = app
        .world
        .get_component::<DrawableComponent>(first)
        .unwrap()
        .texture_bindings
        .0[0]
        .as_ref()
        .unwrap()
        .handle(katla_gfx::TextureHandle::NONE);
    assert_eq!(
        handle,
        app.world
            .get_component::<DrawableComponent>(second)
            .unwrap()
            .texture_bindings
            .0[0]
            .as_ref()
            .unwrap()
            .handle(katla_gfx::TextureHandle::NONE)
    );
    assert_pixel(
        render(&mut app, &mut graph, first, 0),
        [0.21586, 0.05127, 1., 128. / 255.],
    );
    image::save_buffer(
        fixture.0.join("map.png"),
        &[0u8, 255, 0, 255].repeat(4),
        2,
        2,
        image::ColorType::Rgba8,
    )
    .unwrap();
    assign(&mut app, vec![first], TextureRole::Albedo, file.clone()).unwrap();
    assert_pixel(render(&mut app, &mut graph, first, 0), [0., 1., 0., 1.]);
    assert_pixel(
        render(&mut app, &mut graph, second, 0),
        [0.21586, 0.05127, 1., 128. / 255.],
    );
    assert!(app.editor.perform_agent_undo(&mut app.world));
    assert_pixel(
        render(&mut app, &mut graph, first, 0),
        [0.21586, 0.05127, 1., 128. / 255.],
    );
    assert!(app.editor.perform_agent_redo(&mut app.world));
    let revised = app
        .world
        .get_component::<DrawableComponent>(first)
        .unwrap()
        .texture_bindings
        .0[0]
        .as_ref()
        .unwrap()
        .handle(katla_gfx::TextureHandle::NONE);
    assert!(app.editor.perform_agent_undo(&mut app.world));
    image::save_buffer(
        fixture.0.join("map.png"),
        &[128u8, 64, 255, 128].repeat(4),
        2,
        2,
        image::ColorType::Rgba8,
    )
    .unwrap();
    assign(&mut app, vec![first], TextureRole::Normal, file.clone()).unwrap();
    assert!(app.renderer.get_bindless_slot(revised).is_none());
    assert_pixel(
        render(&mut app, &mut graph, first, 1),
        [128. / 255., 64. / 255., 1., 128. / 255.],
    );
    assign(&mut app,vec![first],TextureRole::Emission,json!({"kind":"gltf_image","asset":AssetRef::File(fixture.0.join("mesh.gltf")),"image_index":0})).unwrap();
    assert_pixel(
        render(&mut app, &mut graph, first, 4),
        [0.21586, 0.05127, 1., 128. / 255.],
    );
    let invalid = app
        .world
        .spawn((TransformComponent::from_position(katla_math::Vec3::new(
            0., 0., 0.,
        )),));
    assert!(
        assign(
            &mut app,
            vec![first, invalid],
            TextureRole::Albedo,
            json!({"kind":"neutral"})
        )
        .is_err()
    );
    app.world.destroy_entity(invalid);
    assert!(
        assign(
            &mut app,
            vec![first],
            TextureRole::Albedo,
            json!({"kind":"file","asset":AssetRef::File(fixture.0.join("missing.png"))})
        )
        .is_err()
    );
    assert!(assign(&mut app,vec![first],TextureRole::Albedo,json!({"kind":"gltf_image","asset":AssetRef::File(fixture.0.join("mesh.gltf")),"image_index":99})).is_err());
    assert!(app.editor.perform_agent_undo(&mut app.world));
    assert_pixel(render(&mut app, &mut graph, first, 4), [1.; 4]);
    assert!(app.editor.perform_agent_redo(&mut app.world));
    assert_pixel(
        render(&mut app, &mut graph, first, 4),
        [0.21586, 0.05127, 1., 128. / 255.],
    );
    assert_eq!(app.renderer.material_textures(old_material), Some(original));
    assert_eq!(
        app.world
            .get_component::<DrawableComponent>(first)
            .unwrap()
            .roughness,
        0.25
    );
    let saved = SceneManager::save_scene(&mut app).unwrap();
    let encoded = SceneManager::to_ron(&saved).unwrap();
    let saved = SceneManager::parse(&encoded).unwrap();
    SceneManager::save_to_file(&mut app, &fixture.0.join("saved.katla")).unwrap();
    let inspected = material::execute(
        &mut app,
        MaterialOp::Inspect {
            entity_id: first.id().to_string(),
        },
        true,
    )
    .unwrap();
    assert_eq!(
        inspected["provenance"]["authored_textures"]["albedo"]["source"]["kind"],
        "file"
    );
    let saved_file =
        SceneManager::parse(&std::fs::read_to_string(fixture.0.join("saved.katla")).unwrap())
            .unwrap();
    assert!(
        saved_file
            .entities
            .iter()
            .filter_map(|entity| entity.drawable.as_ref())
            .all(|drawable| matches!(
                drawable.textures.as_ref().unwrap().albedo,
                crate::material_images::TextureSource::File {
                    asset: AssetRef::Scene(_)
                }
            ))
    );
    SceneManager::load_scene(&mut app, saved.clone()).unwrap();
    let first = app
        .world
        .query_ref::<&NameComponent>()
        .find(|(_, name)| name.name == "First")
        .unwrap()
        .0;
    assert_pixel(
        render(&mut app, &mut graph, first, 0),
        [0.21586, 0.05127, 1., 128. / 255.],
    );
    assert_pixel(
        render(&mut app, &mut graph, first, 1),
        [128. / 255., 64. / 255., 1., 128. / 255.],
    );
    let mut bad = saved.clone();
    bad.entities
        .iter_mut()
        .find(|entity| entity.name.as_deref() == Some("First"))
        .unwrap()
        .drawable
        .as_mut()
        .unwrap()
        .textures
        .as_mut()
        .unwrap()
        .emission = crate::material_images::TextureSource::GltfImage {
        asset: AssetRef::File(fixture.0.join("mesh.gltf")),
        image_index: 99,
    };
    assert!(SceneManager::load_scene(&mut app, bad).is_err());
    assert_pixel(
        render(&mut app, &mut graph, first, 0),
        [0.21586, 0.05127, 1., 128. / 255.],
    );
    // Export and move a complete surface, then apply a revision with scoped full-state history.
    let original_root = app.resources.root.clone();
    app.resources.root = fixture.0.join("resources");
    std::fs::create_dir_all(&app.resources.root).unwrap();
    use katla_agent::material_asset::MaterialAssetOp;
    let asset_op = |app: &mut Application, op| {
        crate::application::editor::material_asset::execute(app, op, true)
    };
    let captured = asset_op(
        &mut app,
        MaterialAssetOp::Capture {
            path: "surface.katmat".into(),
            entity_id: first.id().to_string(),
        },
    )
    .unwrap();
    assert_eq!(
        captured["document"]["textures"]["albedo"]["asset"],
        json!({"Scene":"map.png"})
    );
    assert_eq!(
        captured["document"]["textures"]["emission"]["asset"],
        json!({"Scene":"mesh.gltf"})
    );
    assert!(
        captured["document"]["textures"]
            .as_object()
            .unwrap()
            .values()
            .all(|source| source["kind"] != "inherit")
    );
    let moved = fixture.0.join("moved");
    std::fs::create_dir_all(moved.join("resources")).unwrap();
    for name in ["surface.katmat", "map.png", "mesh.gltf", "mesh.bin"] {
        std::fs::copy(fixture.0.join(name), moved.join(name)).unwrap();
    }
    app.resources.root = moved.join("resources");
    let second = app
        .world
        .query_ref::<&NameComponent>()
        .find(|(_, name)| name.name == "Second")
        .unwrap()
        .0;
    let before = |app: &Application, id| {
        let d = app.world.get_component::<DrawableComponent>(id).unwrap();
        (
            material::values(d),
            d.sampling,
            d.texture_bindings.assignments(),
            d.material_handle,
            d.mesh_handle,
        )
    };
    let old_first = before(&app, first);
    let old_second = before(&app, second);
    let mut document = asset_op(
        &mut app,
        MaterialAssetOp::Read {
            path: "surface.katmat".into(),
        },
    )
    .unwrap();
    document["values"]["roughness"] = json!(0.6);
    document["values"]["metallic"] = json!(0.8);
    document["values"]["emissive_factor"] = json!([3., 2., 1.]);
    document["sampling"]["albedo"]["uv"]["scale"] = json!([3., 2.]);
    image::save_buffer(
        moved.join("map.png"),
        &[0u8, 255, 0, 255].repeat(4),
        2,
        2,
        image::ColorType::Rgba8,
    )
    .unwrap();
    asset_op(
        &mut app,
        MaterialAssetOp::Write {
            path: "surface.katmat".into(),
            document: document.clone(),
        },
    )
    .unwrap();
    assert_eq!(before(&app, first), old_first);
    assert_eq!(before(&app, second), old_second);
    let published = std::fs::read(moved.join("surface.katmat")).unwrap();
    let mut invalid = document.clone();
    invalid["textures"]["emission"]["image_index"] = json!(99);
    assert!(
        asset_op(
            &mut app,
            MaterialAssetOp::Write {
                path: "surface.katmat".into(),
                document: invalid
            }
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(moved.join("surface.katmat")).unwrap(),
        published
    );
    let no_mesh = app.world.spawn((NameComponent::new("No mesh"),));
    assert!(
        asset_op(
            &mut app,
            MaterialAssetOp::Apply {
                path: "surface.katmat".into(),
                entity_ids: vec![first.id().to_string(), no_mesh.id().to_string()]
            }
        )
        .is_err()
    );
    assert_eq!(before(&app, first), old_first);
    asset_op(
        &mut app,
        MaterialAssetOp::Apply {
            path: "surface.katmat".into(),
            entity_ids: vec![first.id().to_string(), second.id().to_string()],
        },
    )
    .unwrap();
    for id in [first, second] {
        assert_pixel(render(&mut app, &mut graph, id, 0), [0., 1., 0., 1.]);
        assert_pixel(render(&mut app, &mut graph, id, 1), [0., 1., 0., 1.]);
        assert_pixel(render(&mut app, &mut graph, id, 4), [0., 1., 0., 1.]);
        let current = before(&app, id);
        assert_eq!(current.0.roughness, 0.6);
        assert_eq!(current.0.emissive_factor, [3., 2., 1.]);
        assert_eq!(current.1.albedo.uv.scale, [3., 2.]);
        let original = if id == first { &old_first } else { &old_second };
        assert_eq!((current.3, current.4), (original.3, original.4));
    }
    let mut undo = app.editor.agent_undo_stack.pop().unwrap();
    undo.undo_all(&mut app.world).unwrap();
    assert_eq!(before(&app, first), old_first);
    assert_eq!(before(&app, second), old_second);
    assert_pixel(
        render(&mut app, &mut graph, first, 0),
        [0.21586, 0.05127, 1., 128. / 255.],
    );
    assert!(
        asset_op(
            &mut app,
            MaterialAssetOp::Read {
                path: "../surface.katmat".into()
            }
        )
        .is_err()
    );
    undo.redo_all(&mut app.world).unwrap();
    assert_pixel(render(&mut app, &mut graph, second, 0), [0., 1., 0., 1.]);
    undo.undo_all(&mut app.world).unwrap();
    app.editor.agent_redo_stack.push(undo);
    app.resources.root = original_root;
    let mut empty = saved;
    empty.entities.clear();
    SceneManager::load_scene(&mut app, empty).unwrap();
    app.drain_material_images();
    assert!(app.renderer.get_bindless_slot(handle).is_none());
    assert_eq!(app.gpu_resource_tracker.mesh_count(), 0);
    graph.cleanup();
    app.renderer.destroy_material(app.default_material_handle);
    app.renderer.destroy();
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}

fn graph(app: &mut Application, fixture: &Fixture) -> crate::FrameGraph {
    let material = app
        .renderer
        .compile_material(
            &PipelineDescriptor::simple(fixture.0.join("model_pbr.wgsl").to_string_lossy())
                .with_vertex_layout(VertexLayout::pbr())
                .with_depth(DepthState::disabled())
                .with_depth_format(None)
                .with_cull(CullMode::None)
                .with_color_format(ImageFormat::R16G16B16A16Sfloat),
        )
        .unwrap();
    app.default_material_handle = material;
    let builder = FrameGraphBuilder::new()
        .create_resource(katla_gfx::render_graph::GraphResourceDesc {
            name: "result".into(),
            resource_type: katla_gfx::render_graph::GraphResourceType::ColorAttachment {
                clear_value: None,
            },
            format: ImageFormat::R16G16B16A16Sfloat,
            width: 8,
            height: 8,
            tracks_swapchain_size: false,
        })
        .add_pass(
            GeometryPass::new("probe")
                .without_depth()
                .material(material)
                .write_color("result", ImageFormat::R16G16B16A16Sfloat),
        )
        .export_resource("result");
    #[cfg(not(target_os = "macos"))]
    {
        katla_gfx::render_graph::any_frame_graph::AnyFrameGraph::from_vulkan(
            builder.build::<katla_gfx::VulkanRenderer>().unwrap(),
        )
    }
    #[cfg(target_os = "macos")]
    {
        katla_gfx::render_graph::any_frame_graph::AnyFrameGraph::from_metal(
            builder.build::<katla_gfx::MetalRenderer>().unwrap(),
        )
    }
}

fn render(
    app: &mut Application,
    graph: &mut crate::FrameGraph,
    target: katla_ecs::EntityId,
    role: u32,
) -> [f32; 4] {
    let mut context = crate::FrameContext::new();
    let frustum = katla_math::Frustum::from_proj_and_view(
        &katla_math::Mat4::identity(),
        &katla_math::Mat4::identity(),
    );
    app.collect_draws_with_context(&mut context, &frustum);
    let slot = *app
        .editor
        .entity_to_instance_indices
        .get(&target)
        .unwrap()
        .first()
        .unwrap();
    let draws = context.take_submission().draw_list;
    let pass = graph.pass_id("probe").unwrap();
    graph
        .set_pass_bindings(
            pass,
            PassBindings {
                constants: vec![ConstantBinding {
                    group: 0,
                    binding: 3,
                    stages: ShaderStages::FRAGMENT,
                    bytes: bytemuck::cast_slice(&[role, 0u32, 0, 0]).to_vec(),
                }],
                phases: vec![PassDrawPhase {
                    draw: PassDraw::ObjectIndices(vec![slot]),
                    pipelines: vec![PassPipeline {
                        material: app.default_material_handle,
                        vertex_layout: VertexLayout::pbr(),
                    }],
                    constants: Vec::new(),
                    samplers: Vec::new(),
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
    app.renderer.execute_draw_calls(&frame, &draws).unwrap();
    app.renderer
        .render(&frame, graph, |frame| {
            frame.submit(pass, std::rc::Rc::new(draws));
        })
        .unwrap();
    app.renderer.present(frame).unwrap();
    let source = app
        .renderer
        .graph_texture_source(graph.resource_id("result").unwrap())
        .unwrap();
    let ticket = app
        .renderer
        .queue_texture_readback(source, TextureReadbackRegion::pixel(4, 4))
        .unwrap();
    app.renderer.wait_for_device();
    let bytes = app
        .renderer
        .poll_texture_readback(ticket)
        .unwrap()
        .unwrap()
        .bytes;
    std::array::from_fn(|i| {
        half::f16::from_bits(u16::from_ne_bytes([bytes[2 * i], bytes[2 * i + 1]])).to_f32()
    })
}
fn assert_pixel(actual: [f32; 4], expected: [f32; 4]) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < 0.001),
        "{actual:?} != {expected:?}"
    );
}
