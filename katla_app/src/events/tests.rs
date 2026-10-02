use super::*;
use crate::animation::{AnimatedModel, AnimationClip, AnimationPlayer};
use crate::components::TransformComponent;
use crate::systems::physics::RapierPhysicsSystem;
use katla_agent::events::TriggerOp;
use katla_ecs::{EntityId, System, World};
use katla_math::Vec3;
use katla_physics::{
    ColliderShape, PhysicsActive, PhysicsWorld, RigidBody, SphereShape, TriggerVolume,
};
use katla_script::{PendingPhysicsEvents, PhysicsCollisionEventType};

fn setup() -> (World, EntityId, EntityId) {
    let mut world = World::new();
    world.insert_resource(PhysicsWorld::new());
    world.insert_resource(PhysicsActive(true));
    world.insert_resource(PendingPhysicsEvents::default());
    let visitor = world.spawn((
        TransformComponent::default(),
        ColliderShape::Sphere(SphereShape::new(0.5)),
        RigidBody::kinematic(),
        AnimatedModel {
            animations: ["Idle", "Run"]
                .into_iter()
                .map(|name| {
                    (
                        name.into(),
                        AnimationClip {
                            name: name.into(),
                            duration: 1.0,
                            channels: vec![],
                        },
                    )
                })
                .collect(),
            sequences: Default::default(),
        },
        AnimationPlayer::new("Idle"),
    ));
    let request = serde_json::json!({"action":"create_box", "name":"Start zone", "position":[0,0,0], "half_extents":[2,2,2],
        "rules":[{"event":"enter", "other_entity":visitor.id().to_string(), "once":true,
            "actions":[{"action":"play_animation","target":{"kind":"other"},"clip":"Run"},{"action":"emit","name":"race_started"}]},
            {"event":"exit","actions":[{"action":"emit","name":"left_start"}]}]});
    let result = control::execute(
        &mut world,
        serde_json::from_value::<TriggerOp<String>>(request)
            .unwrap()
            .resolve_ids()
            .unwrap(),
    )
    .unwrap();
    (
        world,
        visitor,
        EntityId::from_raw(result["entity_id"].as_str().unwrap().parse().unwrap()),
    )
}

#[test]
fn test_agent_box_enter_fades_once_exit_and_reentry() {
    let (mut world, visitor, trigger) = setup();
    let mut physics = RapierPhysicsSystem;
    physics.update(&mut world, 0.016);
    let player = world.get_component::<AnimationPlayer>(visitor).unwrap();
    assert!(player.blending);
    assert_eq!(player.target_clip.as_deref(), Some("Run"));
    assert_eq!(player.blend_duration, 0.25);
    assert_eq!(
        world
            .get_component::<TriggerVolume>(trigger)
            .unwrap()
            .overlapping_entities,
        vec![visitor.id()]
    );
    let pending = world.get_resource_mut::<PendingPhysicsEvents>().unwrap();
    assert_eq!(pending.0.len(), 2);
    assert_eq!(
        pending.0[1].event_type,
        PhysicsCollisionEventType::TriggerSignal("race_started".into())
    );
    pending.0.clear();
    physics.update(&mut world, 0.016);
    assert!(
        world
            .get_resource::<PendingPhysicsEvents>()
            .unwrap()
            .0
            .is_empty()
    );
    world
        .get_component_mut::<TransformComponent>(visitor)
        .unwrap()
        .transform
        .position = Vec3::new(10.0, 0.0, 0.0);
    physics.update(&mut world, 0.016);
    assert!(
        world
            .get_component::<TriggerVolume>(trigger)
            .unwrap()
            .overlapping_entities
            .is_empty()
    );
    assert_eq!(
        world.get_resource::<PendingPhysicsEvents>().unwrap().0[1].event_type,
        PhysicsCollisionEventType::TriggerSignal("left_start".into())
    );
    world
        .get_resource_mut::<PendingPhysicsEvents>()
        .unwrap()
        .0
        .clear();
    world
        .get_component_mut::<TransformComponent>(visitor)
        .unwrap()
        .transform
        .position = Vec3::new(0.0, 0.0, 0.0);
    physics.update(&mut world, 0.016);
    assert_eq!(
        world
            .get_resource::<PendingPhysicsEvents>()
            .unwrap()
            .0
            .len(),
        1
    );
    assert!(
        world
            .get_component::<TriggerRules>(trigger)
            .unwrap()
            .last_errors
            .is_empty()
    );
    world.insert_resource(PhysicsActive(false));
    physics.update(&mut world, 0.016);
    assert!(
        world
            .get_component::<TriggerRules>(trigger)
            .unwrap()
            .fired
            .is_empty()
    );
    world.insert_resource(PhysicsActive(true));
    physics.update(&mut world, 0.016);
    // An existing fade fails explicitly, while the next emit action still executes.
    let state = control::execute(
        &mut world,
        TriggerOp::Inspect {
            entity_id: trigger.id(),
        },
    )
    .unwrap();
    assert_eq!(state["last_errors"].as_array().unwrap().len(), 1);
    assert_eq!(state["entity_id"], trigger.id().to_string());
    assert_eq!(state["rules"][0]["other_entity"], visitor.id().to_string());
    assert_eq!(
        state["overlapping_entities"],
        serde_json::json!([visitor.id().to_string()])
    );
    assert_eq!(
        world
            .get_resource::<PendingPhysicsEvents>()
            .unwrap()
            .0
            .len(),
        2
    );
}

#[test]
fn test_trigger_filter_deletion_and_stale_generation() {
    let (mut world, visitor, trigger) = setup();
    let stranger = world.spawn((
        TransformComponent::default(),
        ColliderShape::Sphere(SphereShape::new(0.25)),
        RigidBody::kinematic(),
    ));
    let mut physics = RapierPhysicsSystem;
    physics.update(&mut world, 0.016);
    assert_eq!(
        world
            .get_component::<TriggerVolume>(trigger)
            .unwrap()
            .overlapping_entities
            .len(),
        2
    );
    assert_eq!(
        world
            .get_resource::<PendingPhysicsEvents>()
            .unwrap()
            .0
            .iter()
            .filter(|event| matches!(
                event.event_type,
                PhysicsCollisionEventType::TriggerSignal(_)
            ))
            .count(),
        1
    );
    world.destroy_entity(visitor);
    physics.update(&mut world, 0.016);
    assert_eq!(
        world
            .get_component::<TriggerVolume>(trigger)
            .unwrap()
            .overlapping_entities,
        vec![stranger.id()]
    );
    let replacement = world.create_entity();
    assert_ne!(replacement, visitor);
    let before = world
        .get_component::<TriggerRules>(trigger)
        .unwrap()
        .rules
        .clone();
    assert!(
        control::execute(
            &mut world,
            TriggerOp::SetRules {
                entity_id: trigger.id(),
                rules: vec![TriggerRule {
                    event: TriggerPhase::Enter,
                    other_entity: Some(visitor.id()),
                    once: false,
                    actions: vec![EventAction::Emit { name: "bad".into() }]
                }]
            }
        )
        .is_err()
    );
    assert_eq!(
        world.get_component::<TriggerRules>(trigger).unwrap().rules,
        before
    );
}

#[test]
fn test_rule_reference_mapping_and_scene_validation() {
    let rule: TriggerRule<String> = serde_json::from_value(serde_json::json!({"event":"enter","other_entity":"Player",
        "actions":[{"action":"play_animation","target":{"kind":"entity","entity":"Door"},"clip":"Open"}]})).unwrap();
    let ids = std::collections::HashMap::from([("Player", 5u64), ("Door", 9)]);
    let runtime = rule
        .map_entities(|name| ids.get(name.as_str()).copied().ok_or("missing"))
        .unwrap();
    let names =
        std::collections::HashMap::from([(5u64, "Player".to_string()), (9, "Door".to_string())]);
    assert_eq!(
        runtime
            .map_entities(|id| names.get(id).cloned().ok_or("missing"))
            .unwrap(),
        rule
    );
    let serialized = ron::to_string(&rule).unwrap();
    assert_eq!(
        ron::from_str::<TriggerRule<String>>(&serialized).unwrap(),
        rule
    );
    assert!(rule.map_entities::<u64, _>(|_| Err("missing")).is_err());
}

#[test]
fn test_invalid_authoring_does_not_spawn_or_replace() {
    let (mut world, _, trigger) = setup();
    let count = world.entity_ids().count();
    let request = TriggerOp::CreateBox {
        name: "invalid".into(),
        position: [0.0; 3],
        half_extents: [0.0; 3],
        rules: vec![],
    };
    assert!(control::execute(&mut world, request).is_err());
    assert_eq!(world.entity_ids().count(), count);
    let invalid = TriggerRule {
        event: TriggerPhase::Enter,
        other_entity: None,
        once: false,
        actions: vec![EventAction::PlayAnimation {
            target: EventTarget::Other,
            clip: "Run".into(),
            fade_seconds: f32::NAN,
            looping: true,
            speed: 1.0,
        }],
    };
    assert!(
        control::execute(
            &mut world,
            TriggerOp::SetRules {
                entity_id: trigger.id(),
                rules: vec![invalid]
            }
        )
        .is_err()
    );
    assert_eq!(
        world
            .get_component::<TriggerRules>(trigger)
            .unwrap()
            .rules
            .len(),
        2
    );
}

#[test]
fn test_scene_trigger_keys_survive_duplicate_names_and_reject_invalid_rules() {
    use crate::scene::{SceneEntityId, SceneManager, TriggerVolumeDescriptor, build_default_scene};
    let mut scene = build_default_scene();
    let target = scene.entities[1].id;
    let trigger = &mut scene.entities[0];
    trigger.trigger_volume = Some(TriggerVolumeDescriptor);
    trigger.rigid_body = Some(crate::scene::RigidBodyDescriptor::new(
        katla_physics::BodyType::Kinematic,
    ));
    trigger.collider_shape = Some(crate::scene::ColliderShapeDescriptor::Sphere(1.0));
    trigger.trigger_rules = vec![TriggerRule {
        event: TriggerPhase::Enter,
        other_entity: Some(target),
        once: false,
        actions: vec![EventAction::Emit {
            name: "signal".into(),
        }],
    }];
    scene.validate().unwrap();
    let loaded = SceneManager::parse(&SceneManager::to_ron(&scene).unwrap()).unwrap();
    assert_eq!(
        loaded.entities[0].trigger_rules,
        scene.entities[0].trigger_rules
    );
    scene.entities[0].trigger_rules[0].other_entity = Some(SceneEntityId(u64::MAX));
    assert!(scene.validate().is_err());
    scene.entities[0].trigger_rules[0].other_entity = Some(target);
    scene.entities[2].name = scene.entities[1].name.clone();
    scene.validate().unwrap();
    scene.entities[0].trigger_rules[0].actions.clear();
    assert!(scene.validate().is_err());
    scene.entities[0].trigger_rules.clear();
    scene.entities[0].trigger_rules = vec![TriggerRule {
        event: TriggerPhase::Enter,
        other_entity: None,
        once: false,
        actions: vec![EventAction::Emit {
            name: "signal".into(),
        }],
    }];
    scene.entities[0].trigger_volume = None;
    assert!(scene.validate().is_err());
}

#[test]
#[ignore = "requires native Vulkan or Metal application construction"]
fn test_native_scene_trigger_reference_roundtrip() {
    use crate::application::ApplicationBuilder;
    use crate::components::{NameComponent, PointLight};
    use crate::scene::{EntitySource, SceneManager};
    use crate::{ApplicationFrameGraph, empty_frame_graph};
    let mut app = ApplicationBuilder::new()
        .validation_layer(true)
        .with_frame_graph(|renderer, _| Ok(ApplicationFrameGraph::new(empty_frame_graph(renderer))))
        .build_headless(1, String::new())
        .unwrap();
    let visitor = app.world.spawn((
        TransformComponent::default(),
        PointLight::default(),
        NameComponent::new("Visitor"),
        EntitySource::Light,
    ));
    let op: TriggerOp<String> = serde_json::from_value(serde_json::json!({"action":"create_box", "name":"Box", "position":[0,0,0], "half_extents":[2,2,2],
        "rules":[{"event":"enter","other_entity":visitor.id().to_string(),"once":true,"actions":[{"action":"emit","name":"hello"}]}]})).unwrap();
    let result = control::execute(&mut app.world, op.resolve_ids().unwrap()).unwrap();
    let trigger = EntityId::from_raw(result["entity_id"].as_str().unwrap().parse().unwrap());
    let rules = app
        .world
        .get_component_mut::<TriggerRules>(trigger)
        .unwrap();
    rules.rules.push(TriggerRule {
        event: TriggerPhase::Exit,
        other_entity: None,
        once: false,
        actions: vec![EventAction::PlayAnimation {
            target: EventTarget::Entity {
                entity: visitor.id(),
            },
            clip: "Run".into(),
            fade_seconds: 0.25,
            looping: true,
            speed: 1.0,
        }],
    });
    rules.fired.push(0);
    let scene = SceneManager::save_scene(&mut app).unwrap();
    let descriptor = scene
        .entities
        .iter()
        .find(|entity| entity.name.as_deref() == Some("Box"))
        .unwrap();
    let visitor_key = scene
        .entities
        .iter()
        .find(|entity| entity.name.as_deref() == Some("Visitor"))
        .unwrap()
        .id;
    assert_eq!(descriptor.trigger_rules[0].other_entity, Some(visitor_key));
    assert!(
        matches!(&descriptor.trigger_rules[1].actions[0], EventAction::PlayAnimation {
        target: EventTarget::Entity { entity }, ..
    } if *entity == visitor_key)
    );
    let mut invalid = scene.clone();
    invalid
        .entities
        .iter_mut()
        .find(|entity| entity.name.as_deref() == Some("Box"))
        .unwrap()
        .trigger_rules[0]
        .other_entity = Some(crate::scene::SceneEntityId(u64::MAX));
    let count = app.world.entity_count();
    assert!(SceneManager::load_scene(&mut app, invalid).is_err());
    assert_eq!(app.world.entity_count(), count);
    assert!(app.world.get_component::<TriggerRules>(trigger).is_some());
    let mut renamed = scene;
    renamed
        .entities
        .iter_mut()
        .find(|entity| entity.id == visitor_key)
        .unwrap()
        .name = Some("Renamed visitor".into());
    SceneManager::load_scene(&mut app, renamed).unwrap();
    let visitor_after = app
        .world
        .query_ref::<&NameComponent>()
        .find(|(_, name)| name.name == "Renamed visitor")
        .unwrap()
        .0;
    let trigger_after = app
        .world
        .query_ref::<&NameComponent>()
        .find(|(_, name)| name.name == "Box")
        .unwrap()
        .0;
    assert_ne!(visitor_after, visitor);
    assert_ne!(trigger_after, trigger);
    let rules = app
        .world
        .get_component::<TriggerRules>(trigger_after)
        .unwrap();
    assert_eq!(rules.rules[0].other_entity, Some(visitor_after.id()));
    assert!(
        matches!(&rules.rules[1].actions[0], EventAction::PlayAnimation {
        target: EventTarget::Entity { entity }, ..
    } if *entity == visitor_after.id())
    );
    assert!(rules.fired.is_empty());
    app.world
        .add_component(visitor_after, ColliderShape::Sphere(SphereShape::new(0.5)));
    app.world
        .add_component(visitor_after, RigidBody::kinematic());
    app.world.insert_resource(PhysicsActive(true));
    RapierPhysicsSystem.update(&mut app.world, 0.016);
    assert_eq!(
        app.world.get_resource::<PendingPhysicsEvents>().unwrap().0[1].event_type,
        PhysicsCollisionEventType::TriggerSignal("hello".into())
    );
    // Saving cannot silently redirect a destroyed ID to an unrelated replacement.
    app.world.destroy_entity(visitor_after);
    assert!(SceneManager::save_scene(&mut app).is_err());
    let path =
        std::env::temp_dir().join(format!("katla-stale-trigger-{}.katla", std::process::id()));
    std::fs::write(&path, "existing file").unwrap();
    assert!(SceneManager::save_to_file(&mut app, &path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing file");
    std::fs::remove_file(path).unwrap();
}
