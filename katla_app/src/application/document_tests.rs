//! Native regressions for user-visible scene lifecycle behavior.

use super::editor::document::DocumentAction;
use super::*;
use crate::components::{NameComponent, PointLight, TransformComponent};
use crate::scene::{EntitySource, Scene, SceneManager};
use crate::ui::editor_ui::declarative::scene_dialog::SceneDialog;
use crate::{ApplicationFrameGraph, empty_frame_graph};
use katla_gfx::GpuRenderer;

fn app() -> Application {
    ApplicationBuilder::new()
        .validation_layer(true)
        .with_frame_graph(|renderer, _| Ok(ApplicationFrameGraph::new(empty_frame_graph(renderer))))
        .build_headless(1, String::new())
        .expect("native application")
}

fn light(app: &mut Application, name: &str) -> katla_ecs::EntityId {
    app.world.spawn((
        TransformComponent::from_position(katla_math::Vec3::new(1.0, 2.0, 3.0)),
        PointLight::default(),
        NameComponent::new(name),
        EntitySource::Light,
    ))
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_failed_scene_load_preserves_existing_world_and_document() {
    let mut app = app();
    let existing = light(&mut app, "Keep me");
    let camera = app.camera.entity;
    app.scene_document.saved.name = "Working Scene".into();
    let mut scene = SceneManager::save_scene(&mut app).unwrap();
    let mut missing = scene.entities[0].clone();
    missing.id = crate::scene::SceneEntityId(scene.next_entity_id);
    scene.next_entity_id += 1;
    missing.name = Some("Missing model".into());
    missing.source = EntitySource::GltfModel {
        path: crate::scene::AssetRef::File("/does-not-exist/model.glb".into()),
    };
    scene.entities.push(missing);
    assert!(SceneManager::load_scene(&mut app, scene).is_err());
    assert!(app.world.get_component::<PointLight>(existing).is_some());
    assert_eq!(app.world.entity_count(), 2);
    assert_eq!(app.camera.entity, camera);
    assert_eq!(app.scene_document.saved.name, "Working Scene");
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_play_restore_preserves_properties_and_unsaved_document() {
    let mut app = app();
    let id = light(&mut app, "Lamp");
    let emitter = crate::components::ParticleEmitterComponent::with_config(
        katla_gfx::particles::EmitterConfig {
            emit_rate: 27.0,
            base_lifetime: 4.0,
            color: [0.1, 0.2, 0.3, 1.0],
            ..Default::default()
        },
    );
    app.world.add_component(id, emitter);
    app.world
        .add_component(id, crate::components::PerspectiveComponent::default());
    let audio_path = std::env::temp_dir().join(format!("katla-audio-{}.wav", std::process::id()));
    std::fs::write(&audio_path, b"RIFF").unwrap();
    app.world.add_component(
        id,
        crate::components::AudioEmitter {
            source_path: audio_path.display().to_string(),
            volume: 0.4,
            ..crate::components::AudioEmitter::new("audio.wav")
        },
    );
    app.world.add_component(
        id,
        crate::components::VelocityComponent::new(
            katla_math::Vec3::new(1.0, 0.0, 0.0),
            katla_math::Vec3::new(0.0, 0.0, 0.0),
        ),
    );
    let mut body = katla_physics::RigidBody::dynamic()
        .with_gravity_scale(0.4)
        .with_ccd(true);
    body.linear_velocity = katla_math::Vec3::new(1.0, 2.0, 3.0);
    app.world.add_component(id, body);
    app.world.add_component(
        id,
        crate::components::ReverbZone::new([2.0, 3.0, 4.0]).with_params(0.2, 0.3, 0.4),
    );
    app.scene_document.path = Some("work.katla".into());
    let before = SceneManager::save_scene(&mut app).unwrap();
    let snapshot = game_state::SceneSnapshot::capture(&mut app).unwrap();
    app.world
        .get_component_mut::<PointLight>(id)
        .unwrap()
        .intensity = 99.0;
    snapshot.restore(&mut app).expect("restore");
    assert_eq!(
        SceneManager::save_scene(&mut app).unwrap().entities,
        before.entities
    );
    assert_eq!(app.scene_document.path, Some("work.katla".into()));
    assert!(app.has_unsaved_scene());
    let restored = app
        .world
        .query_ref::<&NameComponent>()
        .find(|(_, name)| name.name == "Lamp")
        .unwrap()
        .0;
    let particles = app
        .world
        .get_component::<crate::components::ParticleEmitterComponent>(restored)
        .unwrap();
    assert_eq!(
        app.world
            .get_component::<crate::components::PerspectiveComponent>(restored)
            .unwrap()
            .fov,
        60.0
    );
    assert_eq!(particles.config.emit_rate, 27.0);
    assert_eq!(particles.config.base_lifetime, 4.0);
    assert_eq!(particles.config.color, [0.1, 0.2, 0.3, 1.0]);
    assert_eq!(
        app.world
            .get_component::<PointLight>(restored)
            .unwrap()
            .intensity,
        1.0
    );
    std::fs::remove_file(audio_path).unwrap();
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_unsaved_quit_requires_save_or_discard_and_failed_save_stays_open() {
    let mut app = app();
    light(&mut app, "Lamp");
    app.request_document_action(DocumentAction::Quit);
    assert!(!app.quit_requested);
    assert_eq!(
        app.editor.editor_ui.scene_dialog,
        Some(SceneDialog::Unsaved)
    );
    app.save_editor_scene(Some("/proc/katla-unwritable.katla".into()));
    assert!(!app.quit_requested);
    assert!(matches!(
        app.editor.editor_ui.scene_dialog,
        Some(SceneDialog::Error(_))
    ));
    assert!(app.editor.pending_document_action.is_some());
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_duplicate_names_keep_distinct_parent_relationships_after_save_load() {
    let mut app = app();
    let first = light(&mut app, "Lamp");
    let second = light(&mut app, "Lamp");
    light(&mut app, "Lamp (2)");
    let child = light(&mut app, "Child");
    app.world
        .add_component(child, crate::components::Parent::new(second));
    let scene = SceneManager::save_scene(&mut app).unwrap();
    let names: Vec<_> = scene
        .entities
        .iter()
        .filter_map(|entity| entity.name.as_deref())
        .collect();
    assert_eq!(names.iter().filter(|name| **name == "Lamp").count(), 2);
    let second_key = app
        .world
        .get_component::<crate::scene::identity::SceneIdentity>(second)
        .unwrap()
        .id;
    assert_eq!(
        scene
            .entities
            .iter()
            .find(|entity| entity.name.as_deref() == Some("Child"))
            .unwrap()
            .parent,
        Some(second_key)
    );
    SceneManager::load_scene(&mut app, scene.clone()).unwrap();
    let restored = SceneManager::save_scene(&mut app).unwrap();
    assert_eq!(scene.entities, restored.entities);
    assert!(app.world.get_component::<PointLight>(first).is_none());
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_saved_scene_metadata_and_custom_path_survive_repeated_saves() {
    let mut app = app();
    let mut scene = Scene::new("Named Scene");
    scene.author = Some("Artist".into());
    scene.created_at = Some("123".into());
    SceneManager::load_scene(&mut app, scene).unwrap();
    let path = std::env::temp_dir().join(format!("katla-scene-{}.katla", std::process::id()));
    SceneManager::save_to_file(&mut app, &path).unwrap();
    light(&mut app, "Lamp");
    SceneManager::save_to_file(&mut app, &path).unwrap();
    let saved: Scene = ron::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved.name, "Named Scene");
    assert_eq!(saved.author.as_deref(), Some("Artist"));
    assert_eq!(saved.created_at.as_deref(), Some("123"));
    assert_eq!(app.scene_document.path.as_ref(), Some(&path));
    assert!(!app.has_unsaved_scene());
    std::fs::remove_file(path).unwrap();
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_save_as_uses_chosen_path_and_requires_explicit_overwrite() {
    let mut app = app();
    light(&mut app, "Lamp");
    let path = std::env::temp_dir().join(format!("katla-save-as-{}.katla", std::process::id()));
    std::fs::write(&path, "keep this file").unwrap();
    app.editor.editor_ui.scene_dialog = Some(SceneDialog::SaveAs(String::new()));
    app.submit_scene_path(path.display().to_string());
    assert_eq!(
        app.editor.editor_ui.scene_dialog,
        Some(SceneDialog::Overwrite(path.clone()))
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep this file");
    assert!(app.scene_document.path.is_none());
    app.editor
        .editor_ui
        .pending_actions
        .push(crate::ui::EditorAction::OverwriteSceneFile);
    editor::process_editor_actions(&mut app);
    assert_eq!(app.scene_document.path, Some(path.clone()));
    assert!(!app.has_unsaved_scene());
    let saved: Scene = ron::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved.entities.len(), 1);
    std::fs::remove_file(path).unwrap();
}

#[derive(katla_ecs::Component, serde::Serialize, serde::Deserialize)]
struct SceneHealth {
    current: u32,
}
#[derive(katla_ecs::Component)]
struct SceneTarget {
    #[inspect(skip)]
    entity: katla_ecs::EntityId,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct SceneTargetData {
    entity: crate::scene::SceneEntityId,
}

fn register_scene_components(app: &mut Application) {
    app.scene_components
        .register::<SceneHealth>("game.health", 1)
        .unwrap();
    app.scene_components
        .register_codec::<SceneTarget, SceneTargetData>(
            "game.target",
            1,
            |value, context| {
                Ok(SceneTargetData {
                    entity: context.id(value.entity)?,
                })
            },
            |data, context| {
                Ok(SceneTarget {
                    entity: context.entity(data.entity)?,
                })
            },
        )
        .unwrap();
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_custom_component_references_and_unknown_data_survive_world_replacement() {
    let mut app = app();
    register_scene_components(&mut app);
    let a = light(&mut app, "Same");
    let b = light(&mut app, "Same");
    app.world.add_component(a, SceneHealth { current: 42 });
    app.world.add_component(a, SceneTarget { entity: b });
    let mut scene = SceneManager::save_scene(&mut app).unwrap();
    let a_key = scene.entities[0].id;
    let b_key = scene.entities[1].id;
    scene.entities[0].name = Some("Renamed".into());
    let future = crate::scene::CustomComponentDescriptor {
        version: 7,
        data: "FutureMode(speed: 88)".into(),
    };
    scene.entities[0]
        .components
        .insert("dlc.mode".into(), future.clone());
    SceneManager::load_scene(&mut app, scene).unwrap();
    let mapped: std::collections::HashMap<_, _> = app
        .world
        .query_ref::<&crate::scene::identity::SceneIdentity>()
        .map(|(entity, key)| (key.id, entity))
        .collect();
    assert_ne!(mapped[&a_key], a);
    assert_ne!(mapped[&b_key], b);
    assert_eq!(
        app.world
            .get_component::<SceneHealth>(mapped[&a_key])
            .unwrap()
            .current,
        42
    );
    assert_eq!(
        app.world
            .get_component::<SceneTarget>(mapped[&a_key])
            .unwrap()
            .entity,
        mapped[&b_key]
    );
    let saved = SceneManager::save_scene(&mut app).unwrap();
    assert_eq!(saved.entities[0].components["dlc.mode"], future);
    app.world.remove_component::<SceneHealth>(mapped[&a_key]);
    let saved = SceneManager::save_scene(&mut app).unwrap();
    assert!(!saved.entities[0].components.contains_key("game.health"));
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_component_decode_failure_rolls_back_staged_gpu_mesh_and_document() {
    let mut app = app();
    register_scene_components(&mut app);
    let old = light(&mut app, "Keep");
    let before = SceneManager::save_scene(&mut app).unwrap();
    let meshes = app.gpu_resource_tracker.mesh_count();
    let mut scene = Scene::new("Fail staged decode");
    scene.next_entity_id = 2;
    let mut e = crate::scene::EntityDescriptor::new(
        crate::scene::SceneEntityId(1),
        EntitySource::Cube { size: [1.0; 3] },
    );
    e.components.insert(
        "game.target".into(),
        crate::scene::CustomComponentDescriptor {
            version: 1,
            data: "(entity:999)".into(),
        },
    );
    scene.entities.push(e);
    assert!(matches!(
        SceneManager::load_scene(&mut app, scene),
        Err(crate::scene::SceneError::Component { .. })
    ));
    assert!(app.world.get_component::<PointLight>(old).is_some());
    assert_eq!(app.world.entity_count(), 2);
    assert_eq!(app.gpu_resource_tracker.mesh_count(), meshes);
    assert_eq!(SceneManager::save_scene(&mut app).unwrap(), before);
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_particle_light_and_joint_components_restore_independently_of_source() {
    use crate::scene::{
        ColliderShapeDescriptor, EntityDescriptor, JointDescriptor, ParticleEmitterDescriptor,
        RigidBodyDescriptor, SceneEntityId,
    };
    let mut app = app();
    let mut scene = Scene::new("Independent components");
    scene.next_entity_id = 4;
    let mut a = EntityDescriptor::new(SceneEntityId(1), EntitySource::Cube { size: [1.0; 3] });
    let mut p = ParticleEmitterDescriptor {
        emit_rate: 29.0,
        color_end: [0.8, 0.7, 0.6, 0.2],
        scale_end: 0.3,
        kill_on_destroy: true,
        timed_emission: Some(3.25),
        burst_queue: vec![3, 9],
        ..Default::default()
    };
    p.active = false;
    a.particle_emitter = Some(p.clone());
    a.point_light = Some(crate::scene::PointLightDescriptor {
        color: [0.2, 0.5, 0.8],
        intensity: 31.0,
        range: 9.0,
    });
    a.rigid_body = Some(RigidBodyDescriptor::new(katla_physics::BodyType::Dynamic));
    a.collider_shape = Some(ColliderShapeDescriptor::Box([0.5; 3]));
    let mut b = EntityDescriptor::new(SceneEntityId(2), EntitySource::Empty);
    b.rigid_body = a.rigid_body.clone();
    b.collider_shape = Some(ColliderShapeDescriptor::Sphere(0.5));
    let mut c = EntityDescriptor::new(SceneEntityId(3), EntitySource::Empty);
    c.joint = Some(JointDescriptor {
        kind: katla_physics::JointType::Hinge,
        a: a.id,
        b: b.id,
        anchor_a: [0.1, 0.2, 0.3],
        anchor_b: [0.4, 0.5, 0.6],
        limits: Some([-0.2, 0.9]),
    });
    scene.entities = vec![c, b, a];
    SceneManager::load_scene(&mut app, scene).unwrap();
    let ids: std::collections::HashMap<_, _> = app
        .world
        .query_ref::<&crate::scene::identity::SceneIdentity>()
        .map(|(entity, key)| (key.id, entity))
        .collect();
    assert_eq!(
        app.world
            .get_component::<PointLight>(ids[&SceneEntityId(1)])
            .unwrap()
            .intensity,
        31.0
    );
    let emitter = app
        .world
        .get_component::<crate::components::ParticleEmitterComponent>(ids[&SceneEntityId(1)])
        .unwrap();
    assert_eq!(emitter.config.emit_rate, 29.0);
    assert_eq!(emitter.config.color_end.0, p.color_end);
    assert_eq!(emitter.config.scale_end, 0.3);
    assert!(emitter.kill_on_destroy);
    assert_eq!(emitter.timed_emission, Some(3.25));
    assert_eq!(emitter.burst_queue, vec![3, 9]);
    assert!(!emitter.active);
    let joint = app
        .world
        .get_component::<katla_physics::Joint>(ids[&SceneEntityId(3)])
        .unwrap();
    assert_eq!(joint.entity_a, ids[&SceneEntityId(1)].id());
    assert_eq!(joint.entity_b, ids[&SceneEntityId(2)].id());
    assert_eq!(joint.anchor_b, [0.4, 0.5, 0.6]);
    assert!(joint.joint_handle.is_none());
    let saved = SceneManager::save_scene(&mut app).unwrap();
    assert_eq!(saved.entities[0].particle_emitter.as_ref(), Some(&p));
    assert_eq!(
        saved.entities[2].joint.as_ref().unwrap().limits,
        Some([-0.2, 0.9])
    );
    let mut physics = crate::systems::RapierPhysicsSystem;
    katla_ecs::System::update(&mut physics, &mut app.world, 1.0 / 60.0);
    assert!(
        app.world
            .get_component::<katla_physics::Joint>(ids[&SceneEntityId(3)])
            .unwrap()
            .joint_handle
            .is_some()
    );
    let handle = app
        .world
        .get_component::<katla_physics::Joint>(ids[&SceneEntityId(3)])
        .unwrap()
        .joint_handle;
    katla_ecs::System::update(&mut physics, &mut app.world, 1.0 / 60.0);
    assert_eq!(
        app.world
            .get_component::<katla_physics::Joint>(ids[&SceneEntityId(3)])
            .unwrap()
            .joint_handle,
        handle
    );
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_identity_counter_survives_deletion_and_rename_without_reusing_keys() {
    let mut app = app();
    let first = light(&mut app, "Old");
    let scene = SceneManager::save_scene(&mut app).unwrap();
    let key = scene.entities[0].id;
    app.world
        .get_component_mut::<NameComponent>(first)
        .unwrap()
        .name = "New".into();
    assert_eq!(
        SceneManager::save_scene(&mut app).unwrap().entities[0].id,
        key
    );
    app.world.destroy_entity(first);
    light(&mut app, "Old");
    let scene = SceneManager::save_scene(&mut app).unwrap();
    assert!(scene.entities[0].id.0 > key.0);
    SceneManager::load_scene(&mut app, scene.clone()).unwrap();
    light(&mut app, "Another");
    let captured = SceneManager::save_scene(&mut app).unwrap();
    assert_eq!(captured.entities[1].id.0, scene.next_entity_id);
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_scene_relative_model_and_save_as_preserve_asset_origin() {
    use crate::scene::{AssetRef, ColliderShapeDescriptor, EntityDescriptor, SceneEntityId};
    let mut app = app();
    let directory = std::env::temp_dir().join(format!("katla-origin-{}", std::process::id()));
    let source_dir = directory.join("source");
    let target_dir = directory.join("target");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    let model = source_dir.join("triangle.stl");
    std::fs::write(&model,b"solid test\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid test\n").unwrap();
    let path = source_dir.join("level.katla");
    let mut scene = Scene::new("Portable");
    scene.next_entity_id = 2;
    let mut e = EntityDescriptor::new(
        SceneEntityId(1),
        EntitySource::StlModel {
            path: AssetRef::Scene("triangle.stl".into()),
        },
    );
    e.collider_shape = Some(ColliderShapeDescriptor::Trimesh);
    scene.entities.push(e);
    std::fs::write(&path, SceneManager::to_ron(&scene).unwrap()).unwrap();
    SceneManager::load_from_file(&mut app, &path).unwrap();
    let before = SceneManager::save_scene(&mut app).unwrap();
    assert!(matches!(
        before.entities[0].source,
        EntitySource::StlModel {
            path: AssetRef::Scene(_)
        }
    ));
    let snapshot = game_state::SceneSnapshot::capture(&mut app).unwrap();
    snapshot.restore(&mut app).unwrap();
    let chosen = target_dir.join("copy.katla");
    SceneManager::save_to_file(&mut app, &chosen).unwrap();
    assert!(!app.has_unsaved_scene());
    let saved = SceneManager::parse(&std::fs::read_to_string(&chosen).unwrap()).unwrap();
    assert_eq!(
        saved.entities[0].source,
        EntitySource::StlModel {
            path: AssetRef::File(model)
        }
    );
    assert_eq!(
        SceneManager::save_scene(&mut app).unwrap().entities,
        saved.entities
    );
    SceneManager::load_from_file(&mut app, &chosen).unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(not(target_os = "macos"))]
#[test]
#[ignore = "requires native Vulkan"]
fn test_failed_model_shader_releases_untracked_mesh_and_geometry() {
    let mut app = app();
    let path = app.resources.model_path("DamagedHelmet.glb");
    let meshes = app
        .renderer
        .as_vulkan()
        .unwrap()
        .asset_registry
        .mesh_count();
    let materials = app
        .renderer
        .as_vulkan()
        .unwrap()
        .asset_registry
        .material_count();
    let entities = app.world.entity_count();
    app.resources.shaders = std::env::temp_dir().join("katla-missing-shader-directory");
    assert!(app.spawn_gltf_model(path, [0.0; 3], None).is_err());
    assert_eq!(
        app.renderer
            .as_vulkan()
            .unwrap()
            .asset_registry
            .mesh_count(),
        meshes
    );
    assert_eq!(
        app.renderer
            .as_vulkan()
            .unwrap()
            .asset_registry
            .material_count(),
        materials
    );
    assert_eq!(app.gpu_resource_tracker.mesh_count(), 0);
    assert_eq!(app.world.entity_count(), entities);
    app.renderer.wait_for_device();
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_visible_entities_without_transforms_fail_instead_of_being_silently_lost() {
    let mut app = app();
    let entity = app.world.spawn((SceneHealth { current: 42 },));
    assert!(SceneManager::save_scene(&mut app).is_err());
    app.world
        .add_component(entity, crate::components::EditorHidden);
    assert!(
        SceneManager::save_scene(&mut app)
            .unwrap()
            .entities
            .is_empty()
    );
}

#[test]
#[ignore = "requires native Vulkan or Metal"]
fn test_document_and_play_restore_retain_captured_resource_root() {
    use crate::scene::{AssetRef, EntityDescriptor, SceneEntityId};
    let mut app = app();
    let root = std::env::temp_dir().join(format!("katla-root-snapshot-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("triangle.stl"),b"solid test\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid test\n").unwrap();
    app.resources.root = root.clone();
    let mut scene = Scene::new("Root snapshot");
    scene.next_entity_id = 2;
    scene.entities.push(EntityDescriptor::new(
        SceneEntityId(1),
        EntitySource::StlModel {
            path: AssetRef::Resource("triangle.stl".into()),
        },
    ));
    SceneManager::load_scene(&mut app, scene).unwrap();
    let snapshot = game_state::SceneSnapshot::capture(&mut app).unwrap();
    app.resources.root = std::env::temp_dir().join("unrelated-resource-root");
    snapshot.restore(&mut app).unwrap();
    let captured = SceneManager::save_scene(&mut app).unwrap();
    assert_eq!(
        captured.entities[0].source,
        EntitySource::StlModel {
            path: AssetRef::Resource("triangle.stl".into())
        }
    );
    let drawable = app
        .world
        .query_ref::<&crate::components::DrawableComponent>()
        .next()
        .unwrap()
        .1;
    assert_eq!(app.renderer.mesh_index_count(drawable.mesh_handle), Some(3));
    std::fs::remove_dir_all(root).unwrap();
}
