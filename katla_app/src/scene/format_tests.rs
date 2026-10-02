//! Document contracts independent of a renderer.
use super::*;

fn entity(id: u64) -> EntityDescriptor {
    EntityDescriptor::new(SceneEntityId(id), EntitySource::Empty)
}

#[test]
fn test_v1_fixture_migrates_into_strict_current_schema() {
    let scene = SceneManager::parse(include_str!("fixtures/v1.katla")).unwrap();
    assert_eq!(scene.version, SCENE_VERSION);
    assert!(!scene.entities.is_empty());
    assert_eq!(scene.next_entity_id, scene.entities.len() as u64 + 1);
    let output = SceneManager::to_ron(&scene).unwrap();
    assert_eq!(SceneManager::parse(&output).unwrap(), scene);
    assert!(!output.contains("mesh_handle"));
    assert!(!output.contains("rigid_body_properties"));
    assert!(!output.contains("resources/models"));
}

#[test]
fn test_published_v2_default_scene_migrates_into_current_schema() {
    let scene = SceneManager::parse(include_str!("fixtures/v2-default.katla")).unwrap();
    assert_eq!(scene.version, SCENE_VERSION);
    assert_eq!(scene.entities.len(), 41);
    assert_eq!(
        SceneManager::parse(&SceneManager::to_ron(&scene).unwrap()).unwrap(),
        scene
    );
}

#[test]
fn test_v2_trigger_names_migrate_to_keys_and_reject_missing_or_ambiguous_targets() {
    use katla_agent::events::{EventAction, EventTarget, TriggerPhase, TriggerRule};
    let rules = vec![TriggerRule {
        event: TriggerPhase::Enter,
        other_entity: Some("Visitor".to_string()),
        once: true,
        actions: vec![EventAction::PlayAnimation {
            target: EventTarget::Entity {
                entity: "Visitor".to_string(),
            },
            clip: "Run".into(),
            fade_seconds: 0.25,
            looping: true,
            speed: 1.0,
        }],
    }];
    let rules = ron::to_string(&rules).unwrap();
    let text = format!(
        r#"(version: 2, name: "Legacy triggers", entities: [
        (name: Some("Entrance"), transform: (), source: Trigger, rigid_body: Some(Kinematic),
         collider_shape: Some(Box((1.0,1.0,1.0))), trigger_volume: Some(()), trigger_rules: {rules}),
        (name: Some("Visitor"), transform: (), source: Light),
        (name: Some("Unrelated"), transform: (), source: Light),
    ])"#
    );
    let scene = SceneManager::parse(&text).unwrap();
    assert_eq!(scene.entities[0].source, EntitySource::Trigger);
    let rule = &scene.entities[0].trigger_rules[0];
    assert_eq!(rule.other_entity, Some(scene.entities[1].id));
    assert!(matches!(&rule.actions[0], EventAction::PlayAnimation {
        target: EventTarget::Entity { entity }, ..
    } if *entity == scene.entities[1].id));
    assert_eq!(
        SceneManager::parse(&SceneManager::to_ron(&scene).unwrap()).unwrap(),
        scene
    );
    for invalid in [
        text.replace("Some(\"Unrelated\")", "Some(\"Visitor\")"),
        text.replace("Some(\"Visitor\"), transform", "Some(\"Gone\"), transform"),
    ] {
        assert!(matches!(
            SceneManager::parse(&invalid),
            Err(SceneError::Migration(_))
        ));
    }
}

#[test]
fn test_legacy_parent_names_become_keys_and_ambiguity_is_rejected() {
    let text = r#"(version: 1, name: "Old", entities: [
        (name: Some("Parent"), transform: (position: (0.0,0.0,0.0), rotation: (0.0,0.0,0.0,1.0), scale: (1.0,1.0,1.0)), source: Light),
        (name: Some("Child"), parent: Some("Parent"), transform: (position: (0.0,0.0,0.0), rotation: (0.0,0.0,0.0,1.0), scale: (1.0,1.0,1.0)), source: ParticleEmitter)
    ])"#;
    let scene = SceneManager::parse(text).unwrap();
    assert_eq!(scene.entities[1].parent, Some(scene.entities[0].id));
    assert!(scene.entities[0].point_light.is_some());
    assert!(scene.entities[1].particle_emitter.is_some());
    let duplicate = text.replace("Some(\"Child\")", "Some(\"Parent\")");
    assert!(matches!(
        SceneManager::parse(&duplicate),
        Err(SceneError::Migration(_))
    ));
    let missing = text.replace("parent: Some(\"Parent\")", "parent: Some(\"Missing\")");
    assert!(SceneManager::parse(&missing).is_err());
}

#[test]
fn test_sparse_document_defaults_and_opaque_game_data_round_trip() {
    let text = r#"(version: 3, name: "Minimal", next_entity_id: 8, entities: [
        (id: 7, name: "Thing", transform: (position: (1.0,2.0,3.0)), components: {
            "game.health": (version: 3, data: "(current:42)")
        }),
    ])"#;
    // IMPLICIT_SOME is explicit in the document header, as in emitted scenes.
    let scene = SceneManager::parse(&format!("#![enable(implicit_some)]\n{text}")).unwrap();
    assert_eq!(scene.entities[0].source, EntitySource::Empty);
    assert_eq!(scene.entities[0].transform.scale, [1.0; 3]);
    assert_eq!(
        scene.entities[0].components["game.health"].data,
        "(current:42)"
    );
    assert_eq!(
        SceneManager::parse(&SceneManager::to_ron(&scene).unwrap()).unwrap(),
        scene
    );
}

#[test]
fn test_canonical_output_is_independent_of_entity_order() {
    let mut scene = Scene::new("Stable");
    scene.next_entity_id = 20;
    scene.entities = vec![entity(9), entity(2)];
    let first = SceneManager::to_ron(&scene).unwrap();
    scene.entities.reverse();
    assert_eq!(first, SceneManager::to_ron(&scene).unwrap());
    assert!(!first.contains("parent: None"));
    assert!(!first.contains("components:"));
}

#[test]
fn test_validation_reports_multiple_precise_entity_fields() {
    let mut scene = Scene::new("Bad");
    scene.next_entity_id = 2;
    let mut e = entity(1);
    e.parent = Some(SceneEntityId(99));
    e.transform.scale[2] = 0.0;
    e.transform.position[0] = f32::NAN;
    e.transform.rotation = [0.0; 4];
    scene.entities = vec![e.clone(), e];
    let Err(SceneError::Validation(issues)) = scene.validate() else {
        panic!("validation diagnostics");
    };
    for field in [
        "id",
        "parent",
        "transform.scale",
        "transform.position",
        "transform.rotation",
    ] {
        assert!(
            issues
                .iter()
                .any(|issue| issue.entity == Some(SceneEntityId(1)) && issue.field == field),
            "{field}: {issues:?}"
        );
    }
}

#[test]
fn test_extreme_tessellation_and_heightfield_values_fail_without_overflow() {
    for source in [
        EntitySource::Sphere {
            radius: 1.0,
            segments: u32::MAX,
            rings: u32::MAX,
        },
        EntitySource::Torus {
            radius: 1.0,
            tube_radius: 0.2,
            segments: u32::MAX,
            tube_segments: u32::MAX,
        },
    ] {
        let mut scene = Scene::new("Budget");
        scene.next_entity_id = 2;
        scene
            .entities
            .push(EntityDescriptor::new(SceneEntityId(1), source));
        assert!(scene.validate().is_err());
    }
    let mut scene = Scene::new("Heightfield");
    scene.next_entity_id = 2;
    let mut e = entity(1);
    e.collider_shape = Some(ColliderShapeDescriptor::Heightfield {
        rows: u32::MAX,
        cols: u32::MAX,
        heights: Vec::new(),
    });
    scene.entities.push(e);
    assert!(scene.validate().is_err());
}

#[test]
fn test_deep_hierarchy_is_validated_iteratively_and_cycles_are_rejected() {
    let mut scene = Scene::new("Deep");
    scene.next_entity_id = 20_001;
    scene.entities = (1..=20_000)
        .map(|id| {
            let mut e = entity(id);
            e.parent = (id > 1).then(|| SceneEntityId(id - 1));
            e
        })
        .collect();
    scene.validate().unwrap();
    scene.entities[0].parent = Some(SceneEntityId(20_000));
    assert!(scene.validate().is_err());
}

#[test]
fn test_asset_roots_resolve_independently_and_relative_paths_are_strict() {
    let base = std::env::temp_dir().join("katla-scene-roots");
    let root = base.join("resources");
    let scene_file = base.join("level/main.katla");
    let assets = SceneAssetContext::new(&root, Some(&scene_file)).unwrap();
    assert_eq!(
        assets
            .resolve(&AssetRef::Resource("models/model.glb".into()))
            .unwrap(),
        root.join("models/model.glb")
    );
    let relative = AssetRef::Scene("model.glb".into());
    assert_eq!(
        assets.resolve(&relative).unwrap(),
        base.join("level/model.glb")
    );
    assert_eq!(
        assets.identify(&base.join("level/model.glb")).unwrap(),
        relative
    );
    assert!(
        SceneAssetContext::new(&root, None)
            .unwrap()
            .resolve(&relative)
            .is_err()
    );
    for bad in [
        "", "../x", "x/../y", "x/./y", "/x", "x\\y", "C:/x", "x//y", "x\0y",
    ] {
        assert!(
            assets.resolve(&AssetRef::Resource(bad.into())).is_err(),
            "{bad:?}"
        );
    }
    assert!(
        assets
            .resolve(&AssetRef::File("relative.glb".into()))
            .is_err()
    );
}

#[test]
fn test_joint_requires_existing_distinct_body_endpoints() {
    let mut scene = Scene::new("Joint");
    scene.next_entity_id = 4;
    let mut a = entity(1);
    a.rigid_body = Some(RigidBodyDescriptor::new(katla_physics::BodyType::Dynamic));
    a.collider_shape = Some(ColliderShapeDescriptor::Sphere(1.0));
    let mut b = a.clone();
    b.id = SceneEntityId(2);
    let mut joint = entity(3);
    joint.joint = Some(JointDescriptor {
        kind: katla_physics::JointType::Fixed,
        a: a.id,
        b: b.id,
        anchor_a: [0.0; 3],
        anchor_b: [0.0; 3],
        limits: None,
    });
    scene.entities = vec![a, b, joint];
    scene.validate().unwrap();
    scene.entities[1].rigid_body = None;
    assert!(scene.validate().is_err());
}

#[test]
fn test_v1_default_fixture_preserves_particles_models_and_physics() {
    let scene = SceneManager::parse(include_str!("fixtures/v1-default.katla")).unwrap();
    assert_eq!(scene.entities.len(), 41);
    assert_eq!(
        scene
            .entities
            .iter()
            .filter(|entity| entity.particle_emitter.is_some())
            .count(),
        3
    );
    assert!(scene.entities.iter().any(|entity| {
        entity
            .rigid_body
            .as_ref()
            .is_some_and(|body| body.kind == katla_physics::BodyType::Dynamic)
    }));
    let fire = scene
        .entities
        .iter()
        .find(|entity| entity.name.as_deref() == Some("FireEmitter"))
        .unwrap()
        .particle_emitter
        .as_ref()
        .unwrap();
    assert_eq!(fire.emit_rate, 400.0);
    assert_eq!(fire.base_lifetime, 2.5);
    assert_eq!(
        SceneManager::parse(&SceneManager::to_ron(&scene).unwrap()).unwrap(),
        scene
    );
}

#[test]
fn test_documented_minimal_scene_is_valid() {
    let readme = include_str!("README.md");
    let text = readme
        .split("```ron\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    let scene = SceneManager::parse(text).unwrap();
    assert_eq!(scene.entities.len(), 2);
    assert_eq!(scene.entities[1].parent, Some(scene.entities[0].id));
}

#[test]
fn test_shipped_scene_files_are_current_and_valid() {
    for name in [
        "default.katla",
        "playground.katla",
        "shared-room.katla",
        "teen-room-blockout.katla",
    ] {
        let file = default_scene_path().with_file_name(name);
        let text = std::fs::read_to_string(&file).unwrap();
        let raw: Scene = ron::from_str(&text).unwrap();
        assert_eq!(raw.version, SCENE_VERSION, "{}", file.display());
        SceneManager::parse(&text).unwrap();
    }
}

#[test]
fn test_camera_validation_matches_engine_degree_units() {
    let camera = crate::components::PerspectiveComponent::default();
    let mut e = entity(1);
    e.perspective = Some(PerspectiveDescriptor {
        fov: camera.fov,
        near: camera.near,
        aspect_ratio: camera.aspect_ratio,
    });
    let mut scene = Scene::new("Camera");
    scene.next_entity_id = 2;
    scene.entities.push(e);
    scene.validate().unwrap();
    assert_eq!(camera.fov, 60.0);
    scene.entities[0].perspective.as_mut().unwrap().fov = 180.0;
    assert!(scene.validate().is_err());
}
