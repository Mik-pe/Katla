//! Native regressions for user-visible scene lifecycle behavior.

use super::editor::document::DocumentAction;
use super::*;
use crate::components::{NameComponent, PointLight, TransformComponent};
use crate::scene::{EntitySource, Scene, SceneManager};
use crate::ui::editor_ui::declarative::scene_dialog::SceneDialog;
use crate::{ApplicationFrameGraph, empty_frame_graph};

fn app() -> Application {
    ApplicationBuilder::new()
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
    let mut scene = SceneManager::save_scene(&app);
    let mut missing = scene.entities[0].clone();
    missing.name = Some("Missing model".into());
    missing.source = EntitySource::GltfModel {
        path: "/does-not-exist/model.glb".into(),
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
    app.world.add_component(
        id,
        crate::components::AudioEmitter {
            source_path: "audio.wav".into(),
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
    let before = SceneManager::save_scene(&app);
    let snapshot = game_state::SceneSnapshot::capture(&app);
    app.world
        .get_component_mut::<PointLight>(id)
        .unwrap()
        .intensity = 99.0;
    snapshot.restore(&mut app).expect("restore");
    assert_eq!(SceneManager::save_scene(&app).entities, before.entities);
    assert_eq!(app.scene_document.path, Some("work.katla".into()));
    assert!(app.has_unsaved_scene());
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
    let scene = SceneManager::save_scene(&app);
    let names: Vec<_> = scene
        .entities
        .iter()
        .filter_map(|entity| entity.name.as_deref())
        .collect();
    assert_eq!(names, ["Child", "Lamp", "Lamp (2)", "Lamp (3)"]);
    SceneManager::load_scene(&mut app, scene.clone()).unwrap();
    let restored = SceneManager::save_scene(&app);
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
