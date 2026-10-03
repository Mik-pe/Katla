//! Native static/skinned primitive identity, surface and scene ownership acceptance.

use super::*;
#[test]
#[ignore = "requires native Vulkan or Metal geometry and material sampling"]
fn test_native_primitive_surfaces_remain_independent_across_scene_roundtrip() {
    assert_native_primitive_surfaces(false);
}

#[test]
#[ignore = "requires native Vulkan or Metal skinning and material sampling"]
fn test_native_skinned_primitive_surfaces_use_selected_skin_across_roundtrip() {
    assert_native_primitive_surfaces(true);
}

fn assert_native_primitive_surfaces(skinned: bool) {
    use crate::components::Children;
    use crate::scene::{EntitySource, SceneManager};
    use katla_gfx::Vertex;
    let fixture = Fixture::new();
    let shader = r#"
struct Object { model: mat4x4f, color: vec4f, params: vec4f, textures: vec4u }
@group(0) @binding(1) var<storage,read> objects: array<Object>;
@group(0) @binding(2) var<storage,read> surfaces: array<SurfaceParameters>;
@group(0) @binding(3) var<uniform> mode: vec4u;
@group(1) @binding(0) var textures: binding_array<texture_2d<f32>,4096>;
@group(5) @binding(0) var role0_sampler: sampler;
@group(5) @binding(1) var role1_sampler: sampler;
@group(5) @binding(2) var role2_sampler: sampler;
@group(5) @binding(3) var role3_sampler: sampler;
@group(5) @binding(4) var role4_sampler: sampler;
struct Output { @builtin(position) position: vec4f, @location(0) @interpolate(flat) object: u32, @location(1) uv0: vec2f, @location(2) uv1: vec2f }
@vertex fn vs_main(@location(0) position: vec3f, @location(3) uv0: vec2f, @location(6) uv1: vec2f, @builtin(instance_index) object: u32) -> Output {
    var result: Output; result.position = objects[object].model * vec4f(position,1); result.position.y = -result.position.y; result.object = object; result.uv0=uv0; result.uv1=uv1; return result;
}
@fragment fn fs_main(input: Output) -> @location(0) vec4f {
    let object = objects[input.object];
    let surface = surfaces[input.object];
    let basis = material_tangent_basis(input.position.xyz,
        material_uv(surface,1u,input.position.xy,input.position.xy),
        vec3f(0,0,1),vec3f(1,0,0),vec3f(0,1,0),surface.normal_occlusion.z > 0.5);
    if (mode.x >= 7u) { return vec4f(basis[0]*0.5+0.5,1); }
    if (mode.x == 2u) { return textureSample(textures[object.textures.x],role0_sampler,material_uv(surface,0u,input.uv0,input.uv1)); }
    if (mode.x == 3u) { return textureSample(textures[object.textures.y],role1_sampler,material_uv(surface,1u,input.uv0,input.uv1)); }
    if (mode.x == 4u) { return textureSample(textures[object.textures.z],role2_sampler,material_uv(surface,2u,input.uv0,input.uv1)); }
    if (mode.x == 5u) { return textureSample(textures[object.textures.w],role3_sampler,material_uv(surface,3u,input.uv0,input.uv1)); }
    if (mode.x == 6u) { return textureSample(textures[u32(object.params.w)],role4_sampler,material_uv(surface,4u,input.uv0,input.uv1)); }
    let texel = textureLoad(textures[object.textures.x], vec2i(0),0);
    if (mode.x == 1u) {
        let surface = surfaces[input.object];
        let normal = surface_tangent_normal(textureLoad(textures[object.textures.y], vec2i(0),0).rgb, surface.normal_occlusion.x);
        let ao = surface_occlusion(textureLoad(textures[object.textures.w], vec2i(0),0).r, surface.normal_occlusion.y, object.params.z);
        let emission = surface_emission(textureLoad(textures[u32(object.params.w)], vec2i(0),0).rgb, surface.emissive.rgb);
        return vec4f(emission.r, normal.y * 0.5 + 0.5, ao, emission.g);
    }
    return vec4f(object.color.rg * texel.rg, object.params.y, object.params.x);
}
"#;
    let shader = format!(
        "{}\n{shader}",
        include_str!("../../../../resources/shaders/common/material_surface.wgsl")
    );
    let skin_shader = shader.replace("@vertex fn vs_main(@location(0) position: vec3f, @location(3) uv0: vec2f, @location(6) uv1: vec2f, @builtin(instance_index) object: u32)",
        "@group(2) @binding(0) var<storage,read> joints: array<mat4x4f>; @vertex fn vs_main(@location(0) position: vec3f, @location(3) uv0: vec2f, @location(6) uv1: vec2f, @location(4) joint_ids: vec4u, @location(5) weights: vec4f, @builtin(instance_index) object: u32)")
        .replace("objects[object].model * vec4f(position,1)","objects[object].model * (joints[joint_ids.x]*weights.x + joints[joint_ids.y]*weights.y + joints[joint_ids.z]*weights.z + joints[joint_ids.w]*weights.w) * vec4f(position,1)");
    for name in ["model_pbr.wgsl", "model_pbr_skinned.wgsl"] {
        std::fs::write(
            fixture.0.join(name),
            if name == "model_pbr_skinned.wgsl" {
                &skin_shader
            } else {
                &shader
            },
        )
        .unwrap();
    }
    let vertices = [
        [-1.0f32, -1.0, 0.0],
        [0.0, -1.0, 0.0],
        [-0.5, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [1.0, -1.0, 0.0],
        [0.5, 1.0, 0.0],
    ];
    std::fs::write(fixture.0.join("mesh.bin"), bytemuck::cast_slice(&vertices)).unwrap();
    for (name, pixel) in [
        ("left.png", [128u8, 255, 255, 255]),
        ("right.png", [255, 128, 255, 255]),
    ] {
        let pixels = [pixel, [0, 0, 255, 255], [255, 0, 0, 255], [0, 255, 0, 255]].concat();
        image::save_buffer(fixture.0.join(name), &pixels, 2, 2, image::ColorType::Rgba8).unwrap();
    }
    std::fs::write(fixture.0.join("mesh.gltf"),r#"{
        "asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0,"name":"Two surfaces"}],
        "buffers":[{"uri":"mesh.bin","byteLength":72}],
        "bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":36}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[-1,-1,0],"max":[0,1,0]},
                     {"bufferView":1,"componentType":5126,"count":3,"type":"VEC3","min":[0,-1,0],"max":[1,1,0]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0},{"attributes":{"POSITION":1},"material":1}]}],
        "images":[{"uri":"left.png"},{"uri":"right.png"}],"textures":[{"source":0},{"source":1}],
        "materials":[{"emissiveFactor":[0.2,0.4,0.1],"normalTexture":{"index":0,"scale":0},"occlusionTexture":{"index":0,"strength":0.25},"pbrMetallicRoughness":{"baseColorFactor":[0.8,0.2,0.1,1],"metallicFactor":0.25,"roughnessFactor":0.75,"baseColorTexture":{"index":0}}},
                     {"emissiveFactor":[0.6,0.1,0.2],"emissiveTexture":{"index":1},"normalTexture":{"index":0,"scale":2},"occlusionTexture":{"index":0,"strength":0.75},"pbrMetallicRoughness":{"baseColorFactor":[0.1,0.9,0.2,1],"metallicFactor":0.8,"roughnessFactor":0.2,"baseColorTexture":{"index":1}}}]
    }"#).unwrap();
    if skinned {
        let mut document: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture.0.join("mesh.gltf")).unwrap())
                .unwrap();
        let mut bytes = std::fs::read(fixture.0.join("mesh.bin")).unwrap();
        bytes.extend_from_slice(&[0u8; 12]);
        bytes.extend_from_slice(bytemuck::cast_slice(&[[1.0f32, 0.0, 0.0, 0.0]; 3]));
        document["buffers"][0]["byteLength"] = serde_json::json!(bytes.len());
        document["bufferViews"].as_array_mut().unwrap().extend([
            serde_json::json!({"buffer":0,"byteOffset":72,"byteLength":12}),
            serde_json::json!({"buffer":0,"byteOffset":84,"byteLength":48}),
        ]);
        document["accessors"].as_array_mut().unwrap().extend([
            serde_json::json!({"bufferView":2,"componentType":5121,"count":3,"type":"VEC4"}),
            serde_json::json!({"bufferView":3,"componentType":5126,"count":3,"type":"VEC4"}),
        ]);
        for primitive in document["meshes"][0]["primitives"].as_array_mut().unwrap() {
            primitive["attributes"]["JOINTS_0"] = serde_json::json!(2);
            primitive["attributes"]["WEIGHTS_0"] = serde_json::json!(3);
        }
        document["nodes"][0]["skin"] = serde_json::json!(1);
        document["nodes"].as_array_mut().unwrap().extend([
            serde_json::json!({"translation":[4,0,0]}),
            serde_json::json!({}),
        ]);
        document["scenes"][0]["nodes"] = serde_json::json!([0, 1, 2]);
        document["skins"] = serde_json::json!([{"joints":[1]},{"joints":[2]}]);
        std::fs::write(fixture.0.join("mesh.bin"), bytes).unwrap();
        std::fs::write(
            fixture.0.join("mesh.gltf"),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(fixture.0.join("compute/animation")).unwrap();
        std::fs::write(
            fixture.0.join("compute/animation/pose_eval.wgsl"),
            include_str!("../../../../resources/shaders/compute/animation/pose_eval.wgsl"),
        )
        .unwrap();
    }
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.0.join("mesh.gltf")).unwrap()).unwrap();
    let mut bytes = std::fs::read(fixture.0.join("mesh.bin")).unwrap();
    for (set, value) in [(0, [0.25f32, 0.25]), (1, [0.75, 0.75])] {
        let offset = bytes.len();
        bytes.extend_from_slice(bytemuck::cast_slice(&[value; 3]));
        let view = document["bufferViews"].as_array().unwrap().len();
        let accessor = document["accessors"].as_array().unwrap().len();
        document["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"buffer":0,"byteOffset":offset,"byteLength":24}));
        document["accessors"].as_array_mut().unwrap().push(
            serde_json::json!({"bufferView":view,"componentType":5126,"count":3,"type":"VEC2"}),
        );
        for primitive in document["meshes"][0]["primitives"].as_array_mut().unwrap() {
            primitive["attributes"][format!("TEXCOORD_{set}")] = serde_json::json!(accessor);
        }
    }
    document["buffers"][0]["byteLength"] = serde_json::json!(bytes.len());
    document["extensionsUsed"] = serde_json::json!(["KHR_texture_transform"]);
    document["samplers"] = serde_json::json!([{"minFilter":9728,"magFilter":9728,"wrapS":10497,"wrapT":10497},{"minFilter":9728,"magFilter":9728,"wrapS":33071,"wrapT":33071}]);
    document["textures"][0]["sampler"] = serde_json::json!(0);
    document["textures"][1]["sampler"] = serde_json::json!(1);
    for material in document["materials"].as_array_mut().unwrap() {
        material["pbrMetallicRoughness"]["baseColorTexture"]["extensions"] =
            serde_json::json!({"KHR_texture_transform":{"texCoord":1,"offset":[0.5,-0.5]}});
        material["pbrMetallicRoughness"]["metallicRoughnessTexture"] =
            serde_json::json!({"index":0});
        material["normalTexture"]["extensions"] = serde_json::json!({"KHR_texture_transform":{"texCoord":1,"scale":[-1,1],"offset":[1,0]}});
        material["occlusionTexture"]["extensions"] = serde_json::json!({"KHR_texture_transform":{"rotation":std::f32::consts::FRAC_PI_2,"offset":[1,0]}});
    }
    std::fs::write(fixture.0.join("mesh.bin"), bytes).unwrap();
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
    #[cfg(not(target_os = "macos"))]
    let errors = {
        let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let backend = app.renderer.as_vulkan().unwrap();
        assert!(backend.context().validation_active());
        let captured = errors.clone();
        backend
            .context()
            .set_validation_callback(move |message, level| {
                if level == katla_gfx::ValidationLevel::Error {
                    captured.lock().unwrap().push(message.to_string());
                }
            });
        errors
    };
    app.resources.shaders = fixture.0.clone();
    let root = app
        .spawn_gltf_model(fixture.0.join("mesh.gltf"), [0.0; 3], None)
        .unwrap();
    assert!(app.world.get_component::<DrawableComponent>(root).is_none());
    let children = app
        .world
        .get_component::<Children>(root)
        .unwrap()
        .children
        .clone();
    assert_eq!(children.len(), 2);
    let mut scene = SceneManager::save_scene(&mut app).unwrap();
    assert_eq!(scene.entities.len(), 3);
    assert!(
        scene
            .entities
            .iter()
            .any(|entity| matches!(entity.source, EntitySource::GltfGroup { .. }))
    );
    let builder = FrameGraphBuilder::new()
        .create_resource(katla_gfx::render_graph::GraphResourceDesc {
            name: "result".into(),
            resource_type: katla_gfx::render_graph::GraphResourceType::ColorAttachment {
                clear_value: Some([0.0; 4]),
            },
            format: ImageFormat::R8G8B8A8Unorm,
            width: 16,
            height: 8,
            tracks_swapchain_size: false,
        })
        .add_pass(
            GeometryPass::new("primitive surfaces")
                .without_depth()
                .write_color("result", ImageFormat::R8G8B8A8Unorm),
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
    let pass = graph.pass_id("primitive surfaces").unwrap();
    let mut animation = skinned.then(|| {
        let mut animation = crate::application::scene_features::AnimationFeatures::new(
            &mut app.renderer,
            &app.resources,
        )
        .unwrap();
        animation.install_graph(&mut graph).unwrap();
        animation.warm_pipeline(&mut app.renderer, &graph).unwrap();
        animation
    });
    let mut cpu_animation = crate::systems::gpu_animation_system::GpuAnimationSystem::new();

    for roundtrip in 0..2 {
        if roundtrip == 1 {
            let original_materials: Vec<_> = app
                .world
                .query_ref::<&DrawableComponent>()
                .map(|(_, drawable)| drawable.material_handle)
                .collect();
            SceneManager::load_scene(&mut app, scene.clone()).unwrap();
            assert_eq!(
                SceneManager::save_scene(&mut app).unwrap().entities,
                scene.entities
            );
            assert_eq!(app.gpu_resource_tracker.mesh_count(), 2);
            assert_eq!(app.gpu_resource_tracker.material_count(), 2);
            assert_eq!(app.gpu_resource_tracker.texture_count(), 3);
            assert!(
                app.world
                    .query_ref::<&DrawableComponent>()
                    .all(|(_, drawable)| !original_materials.contains(&drawable.material_handle))
            );
        }
        for mode in 0u32..11 {
            let mut context = crate::rendering::FrameContext::new();
            let mut pipelines = Vec::new();
            for (_, drawable) in app.world.query_ref::<&DrawableComponent>() {
                assert_eq!(app.renderer.mesh_index_count(drawable.mesh_handle), Some(3));
                pipelines.push(PassPipeline {
                    vertex_layout: if skinned {
                        katla_gfx::VertexPBRSkinned::layout()
                    } else {
                        katla_gfx::VertexPBR::layout()
                    },
                    material: drawable.material_handle,
                });
                let mut sampling = drawable.sampling;
                if mode >= 7 {
                    sampling.normal.uv = match mode {
                        8 => crate::rendering::UvTransform {
                            rotation: std::f32::consts::FRAC_PI_2,
                            ..Default::default()
                        },
                        9 => crate::rendering::UvTransform {
                            scale: [-1., 1.],
                            ..Default::default()
                        },
                        10 => crate::rendering::UvTransform {
                            scale: [0.; 2],
                            ..Default::default()
                        },
                        _ => Default::default(),
                    };
                }
                context
                    .draw(drawable.mesh_handle, drawable.material_handle)
                    .with_skeleton(drawable.skeleton_handle)
                    .with_color(drawable.color.unwrap().to_array())
                    .with_pbr(drawable.metallic, drawable.roughness, drawable.ao)
                    .with_emission(drawable.emission)
                    .with_surface(drawable.surface)
                    .with_sampling(sampling)
                    .with_tangent_uv(if mode == 7 { None } else { drawable.tangent_uv })
                    .submit();
            }
            let submission = context.take_submission();
            let list = submission.draw_list;
            let indices: Vec<_> = list.iter().map(|draw| draw.base_object_slot()).collect();
            let mut phases =
                crate::application::scene_features::material_pipelines::geometry_phases(
                    &indices,
                    &submission.samplers,
                );
            for phase in &mut phases {
                phase.pipelines = pipelines.clone();
            }
            graph
                .set_pass_bindings(
                    pass,
                    PassBindings {
                        constants: vec![
                            ConstantBinding {
                                group: 0,
                                binding: 2,
                                stages: ShaderStages::FRAGMENT,
                                bytes: bytemuck::cast_slice(&submission.surfaces).to_vec(),
                            },
                            ConstantBinding {
                                group: 0,
                                binding: 3,
                                stages: ShaderStages::FRAGMENT,
                                bytes: bytemuck::cast_slice(&[mode, 0, 0, 0]).to_vec(),
                            },
                        ],
                        phases,
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
                panic!("headless frame")
            };
            if let Some(animation) = &mut animation {
                assert!(
                    app.world
                        .query_ref::<&crate::animation::Skin>()
                        .all(|(_, skin)| skin.joints == [2])
                );
                animation
                    .prepare_frame(
                        &mut app.renderer,
                        &mut graph,
                        &frame,
                        &mut app.world,
                        &mut cpu_animation,
                    )
                    .unwrap();
                graph
                    .set_pass_commands(pass, vec![], animation.skinning_accesses().to_vec())
                    .unwrap();
                animation.retire_unused_imports(&mut graph).unwrap();
            }
            app.renderer.execute_draw_calls(&frame, &list).unwrap();
            app.renderer
                .render(&frame, &mut graph, |frame| {
                    frame.submit(pass, std::rc::Rc::new(list));
                })
                .unwrap();
            app.renderer.present(frame).unwrap();
            let source = app
                .renderer
                .graph_texture_source(graph.resource_id("result").unwrap())
                .unwrap();
            let expected = if mode >= 2 {
                let values = match mode {
                    2 => [[55, 255, 255, 255], [0, 0, 255, 255]],
                    3 => [[255, 0, 0, 255]; 2],
                    4 => [[128, 255, 255, 255]; 2],
                    5 => [[0, 0, 255, 255]; 2],
                    6 => [[255, 255, 255, 255], [255, 55, 255, 255]],
                    8 => [[128, 0, 128, 255]; 2],
                    9 => [[0, 128, 128, 255]; 2],
                    _ => [[255, 128, 128, 255]; 2],
                };
                [([4, 4], values[0]), ([12, 4], values[1])]
            } else if mode == 0 {
                [([4, 4], [44u8, 51, 191, 64]), ([12, 4], [26, 50, 51, 204])]
            } else {
                [([4, 4], [51, 128, 223, 102]), ([12, 4], [153, 242, 160, 6])]
            };
            for (origin, expected) in expected {
                let ticket = app
                    .renderer
                    .queue_texture_readback(
                        source,
                        TextureReadbackRegion {
                            origin,
                            size: Size2D::new(1, 1),
                            mip_level: 0,
                            array_layer: 0,
                        },
                    )
                    .unwrap();
                app.renderer.wait_for_device();
                let texel = app.renderer.poll_texture_readback(ticket).unwrap().unwrap();
                for (actual, expected) in texel.bytes.iter().zip(expected) {
                    assert!(
                        actual.abs_diff(expected) <= 1,
                        "roundtrip {roundtrip} at {origin:?}: {:?} expected {expected}",
                        texel.bytes
                    );
                }
            }
        }
    }
    let mut unavailable_uv_asset: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.0.join("mesh.gltf")).unwrap()).unwrap();
    for primitive in unavailable_uv_asset["meshes"][0]["primitives"]
        .as_array_mut()
        .unwrap()
    {
        primitive["attributes"]
            .as_object_mut()
            .unwrap()
            .remove("TEXCOORD_1");
    }
    for material in unavailable_uv_asset["materials"].as_array_mut().unwrap() {
        material["pbrMetallicRoughness"]["baseColorTexture"]["extensions"]
            .as_object_mut()
            .unwrap()
            .remove("KHR_texture_transform");
        material["normalTexture"]["extensions"]
            .as_object_mut()
            .unwrap()
            .remove("KHR_texture_transform");
    }
    let unavailable_path = fixture.0.join("uv0_only.gltf");
    std::fs::write(
        &unavailable_path,
        serde_json::to_vec(&unavailable_uv_asset).unwrap(),
    )
    .unwrap();
    let mut invalid_sampling_scene = scene.clone();
    for entity in &mut invalid_sampling_scene.entities {
        match &mut entity.source {
            EntitySource::GltfPrimitive { path, .. } | EntitySource::GltfGroup { path } => {
                *path = crate::scene::AssetRef::File(unavailable_path.clone())
            }
            _ => {}
        }
        if let Some(drawable) = &mut entity.drawable {
            let mut sampling = crate::rendering::MaterialSampling::default();
            sampling.normal.uv.tex_coord = 1;
            drawable.sampling = Some(sampling);
        }
    }
    let before_invalid_uv = SceneManager::save_scene(&mut app).unwrap();
    assert!(
        SceneManager::load_scene(&mut app, invalid_sampling_scene)
            .unwrap_err()
            .to_string()
            .contains("normal texture requires missing TEXCOORD_1")
    );
    assert_eq!(
        SceneManager::save_scene(&mut app).unwrap().entities,
        before_invalid_uv.entities
    );
    assert_eq!(app.gpu_resource_tracker.mesh_count(), 2);
    assert_eq!(app.gpu_resource_tracker.texture_count(), 3);
    if let EntitySource::GltfPrimitive {
        primitive_index, ..
    } = &mut scene
        .entities
        .iter_mut()
        .find(|entity| matches!(entity.source, EntitySource::GltfPrimitive { .. }))
        .unwrap()
        .source
    {
        *primitive_index = 999;
    }
    let before = SceneManager::save_scene(&mut app).unwrap();
    assert!(SceneManager::load_scene(&mut app, scene).is_err());
    assert_eq!(
        SceneManager::save_scene(&mut app).unwrap().entities,
        before.entities
    );
    assert_eq!(app.gpu_resource_tracker.mesh_count(), 2);
    assert_eq!(app.gpu_resource_tracker.texture_count(), 3);
    let mut original = crate::scene::Scene::new("Whole model input");
    let mut desc = crate::scene::EntityDescriptor::new(
        crate::scene::SceneEntityId(1),
        EntitySource::GltfModel {
            path: crate::scene::AssetRef::File(fixture.0.join("mesh.gltf")),
        },
    );
    desc.drawable = Some(crate::scene::DrawableDescriptor {
        surface: None,
        sampling: None,
        color: None,
        metallic: 0.33,
        roughness: 0.66,
        ao: 0.8,
    });
    desc.collider_shape = Some(crate::scene::ColliderShapeDescriptor::Trimesh);
    original.entities.push(desc);
    original.next_entity_id = 2;
    SceneManager::load_scene(&mut app, original).unwrap();
    let expanded = SceneManager::save_scene(&mut app).unwrap();
    assert_eq!(expanded.entities.len(), 3);
    assert_eq!(expanded.next_entity_id, 4);
    for (_, drawable) in app.world.query_ref::<&DrawableComponent>() {
        assert_eq!(drawable.metallic, 0.33);
        assert_eq!(drawable.roughness, 0.66);
    }
    let mesh = app
        .world
        .query_ref::<&super::super::CollisionMesh>()
        .next()
        .unwrap()
        .1
        .handle;
    let geometry = app.geometry_cache.get(mesh).unwrap();
    assert_eq!(geometry.positions.len(), 6);
    assert_eq!(geometry.triangles, [[0, 1, 2], [3, 4, 5]]);
    assert_eq!(app.gpu_resource_tracker.mesh_count(), 3);
    SceneManager::load_scene(&mut app, expanded.clone()).unwrap();
    assert_eq!(
        SceneManager::save_scene(&mut app).unwrap().entities,
        expanded.entities
    );
    SceneManager::load_scene(&mut app, crate::scene::Scene::new("Empty")).unwrap();
    assert_eq!(app.gpu_resource_tracker.mesh_count(), 0);
    assert_eq!(app.gpu_resource_tracker.material_count(), 0);
    assert_eq!(app.gpu_resource_tracker.texture_count(), 0);
    assert!(app.geometry_cache.get(mesh).is_none());
    graph.cleanup();
    app.renderer.destroy();
    #[cfg(not(target_os = "macos"))]
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}
