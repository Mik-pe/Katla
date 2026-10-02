//! Semantic playback commands shared by the editor agent and MCP bridge.

use katla_agent::animation::AnimationOp;
use katla_ecs::{EntityId, World};
use serde_json::{Value, json};

use super::{AnimatedModel, AnimationPlayer};

pub(crate) fn execute(world: &mut World, op: AnimationOp) -> Result<Value, String> {
    let entity_id = match &op {
        AnimationOp::Inspect { entity_id } | AnimationOp::Play { entity_id, .. } => *entity_id,
    };
    let entity = EntityId::from_raw(entity_id);
    let model = world
        .get_component::<AnimatedModel>(entity)
        .ok_or_else(|| format!("Entity {entity_id} has no animated model or is stale"))?;
    if let AnimationOp::Play {
        clip,
        fade_seconds,
        looping,
        speed,
        ..
    } = op
    {
        if !fade_seconds.is_finite() || fade_seconds < 0.0 {
            return Err("fade_seconds must be finite and nonnegative".into());
        }
        if !speed.is_finite() || speed < 0.0 {
            return Err(
                "speed must be finite and nonnegative; reverse playback is unsupported".into(),
            );
        }
        let duration = model
            .animations
            .get(&clip)
            .ok_or_else(|| {
                format!("Unknown clip '{clip}'; inspect this entity's animations first")
            })?
            .duration;
        if !duration.is_finite() || duration < 0.0 {
            return Err(format!("Clip '{clip}' has an invalid duration"));
        }
        if let Some(player) = world.get_component::<AnimationPlayer>(entity) {
            if fade_seconds > 0.0 && player.blending {
                return Err("A fade is already active; inspect progress and retry after completion, or use fade_seconds=0 for an immediate switch".into());
            }
            if fade_seconds > 0.0
                && let Some(source) = &player.current_clip
            {
                let source_duration = model
                    .animations
                    .get(source)
                    .ok_or_else(|| {
                        format!(
                            "Active clip '{source}' is missing; use fade_seconds=0 to replace it"
                        )
                    })?
                    .duration;
                if !source_duration.is_finite() || source_duration < 0.0 {
                    return Err(format!("Active clip '{source}' has an invalid duration"));
                }
            }
        }
        if world.get_component::<AnimationPlayer>(entity).is_none() {
            world.add_component(entity, AnimationPlayer::stopped());
        }
        let player = world
            .get_component_mut::<AnimationPlayer>(entity)
            .ok_or_else(|| "Animation player disappeared".to_string())?;
        if fade_seconds == 0.0 || player.current_clip.is_none() {
            player.set_clip(clip, duration);
            player.loop_animation = looping;
        } else {
            player
                .crossfade_to(clip, duration, fade_seconds)
                .map_err(String::from)?;
            player.target_loop_animation = looping;
        }
        player.speed = speed;
        player.play();
    }
    inspect(world, entity)
}

fn inspect(world: &World, entity: EntityId) -> Result<Value, String> {
    let model = world
        .get_component::<AnimatedModel>(entity)
        .ok_or_else(|| "Animated model disappeared".to_string())?;
    let mut clips: Vec<_> = model.animations.iter().collect();
    clips.sort_unstable_by_key(|(name, _)| *name);
    let clips: Vec<_> = clips
        .into_iter()
        .map(|(name, clip)| {
            json!({
                "name": name, "duration_seconds": clip.duration,
            })
        })
        .collect();
    let playback = world
        .get_component::<AnimationPlayer>(entity)
        .map(|player| {
            json!({
                "clip": player.current_clip,
                "time_seconds": player.time,
                "playing": player.playing,
                "looping": player.loop_animation,
                "speed": player.speed,
                "transition": if player.blending { Some(json!({
                    "target_clip": player.target_clip,
                    "target_time_seconds": player.target_time,
                    "target_looping": player.target_loop_animation,
                    "duration_seconds": player.blend_duration,
                    "elapsed_seconds": player.blend_time,
                    "progress": 1.0 - player.blend_weight,
                })) } else { None },
            })
        });
    Ok(json!({ "entity_id": entity.id(), "clips": clips, "playback": playback }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{
        AnimationClip, AnimationEvent, AnimationUpdateSystem,
        gpu_clip_loader::build_skeleton_params,
    };
    use katla_ecs::SystemExecutionOrder;
    use std::collections::HashMap;

    fn world() -> (World, EntityId) {
        let model = AnimatedModel {
            animations: [("Walk", 0.1), ("Run", 0.2)]
                .into_iter()
                .map(|(name, duration)| {
                    (
                        name.into(),
                        AnimationClip {
                            name: name.into(),
                            duration,
                            channels: vec![],
                        },
                    )
                })
                .collect(),
            sequences: HashMap::new(),
        };
        let mut world = World::new();
        let entity = world.spawn((model, AnimationPlayer::new("Walk")));
        world.register_typed_system(AnimationUpdateSystem, SystemExecutionOrder::NORMAL);
        (world, entity)
    }

    fn request(entity: EntityId, clip: &str, fade_seconds: f32, looping: bool) -> AnimationOp {
        AnimationOp::Play {
            entity_id: entity.id(),
            clip: clip.into(),
            fade_seconds,
            looping,
            speed: 1.0,
        }
    }

    #[test]
    fn test_agent_request_drives_fade_and_real_gpu_parameters() {
        for parallel in [false, true] {
            let (mut world, entity) = world();
            let names = HashMap::from([("Walk".into(), 0), ("Run".into(), 1)]);
            let state = execute(&mut world, request(entity, "Run", 1.0, false)).unwrap();
            assert_eq!(state["clips"][0]["name"], "Run");
            assert_eq!(state["playback"]["transition"]["progress"], 0.0);
            for (dt, source_weight) in [(0.25, 0.75), (0.25, 0.5), (0.5, 1.0)] {
                if parallel {
                    world.update_parallel(dt);
                } else {
                    world.update(dt);
                }
                let player = world.get_component::<AnimationPlayer>(entity).unwrap();
                let params = build_skeleton_params(player, &names, 0, 1);
                assert_eq!(params.blend_weight, source_weight);
                assert_eq!(params.target_time, if player.blending { 0.2 } else { 0.0 });
                assert_eq!(params.clip_index, u32::from(!player.blending));
            }
            let state = execute(
                &mut world,
                AnimationOp::Inspect {
                    entity_id: entity.id(),
                },
            )
            .unwrap();
            assert_eq!(state["playback"]["clip"], "Run");
            assert_eq!(state["playback"]["playing"], false);
            assert!(state["playback"]["transition"].is_null());
            let player = world.get_component::<AnimationPlayer>(entity).unwrap();
            assert!(player.is_complete());
            assert_eq!(player.events.len(), 2);
        }
    }

    #[test]
    fn test_target_loop_policy_and_multiple_loops_carry_into_player() {
        let (mut world, entity) = world();
        world
            .get_component_mut::<AnimatedModel>(entity)
            .unwrap()
            .animations
            .get_mut("Run")
            .unwrap()
            .duration = 0.25;
        execute(&mut world, request(entity, "Run", 1.0, true)).unwrap();
        world.update(1.0);
        let player = world.get_component::<AnimationPlayer>(entity).unwrap();
        assert!(player.loop_animation && player.playing);
        assert_eq!(player.time, 0.0);
        assert_eq!(player.loop_count, 4);
        assert!(player.events.iter().any(
            |event| matches!(event, AnimationEvent::Looped { clip_name, .. } if clip_name == "Run")
        ));
        assert!(!player.events.iter().any(
            |event| matches!(event, AnimationEvent::Completed { clip_name } if clip_name == "Run")
        ));
    }

    #[test]
    fn test_looping_source_can_fade_to_a_nonlooping_target() {
        let (mut world, entity) = world();
        world
            .get_component_mut::<AnimationPlayer>(entity)
            .unwrap()
            .loop_animation = true;
        execute(&mut world, request(entity, "Run", 1.0, false)).unwrap();
        world.update(1.0);
        let player = world.get_component::<AnimationPlayer>(entity).unwrap();
        assert_eq!(player.current_clip.as_deref(), Some("Run"));
        assert_eq!(player.time, 0.2);
        assert!(!player.loop_animation);
        assert!(!player.playing);
        assert!(player.is_complete());
    }

    #[test]
    fn test_invalid_or_overlapping_requests_preserve_playback() {
        let (mut world, entity) = world();
        for op in [
            request(entity, "missing", 0.25, true),
            request(entity, "Run", -1.0, true),
            request(entity, "Run", f32::NAN, true),
            AnimationOp::Play {
                entity_id: entity.id(),
                clip: "Run".into(),
                fade_seconds: 0.25,
                looping: true,
                speed: f32::INFINITY,
            },
        ] {
            assert!(execute(&mut world, op).is_err());
            let player = world.get_component::<AnimationPlayer>(entity).unwrap();
            assert_eq!(player.current_clip.as_deref(), Some("Walk"));
            assert!(!player.blending);
        }
        execute(&mut world, request(entity, "Run", 1.0, true)).unwrap();
        world.update(0.25);
        assert!(execute(&mut world, request(entity, "Walk", 1.0, true)).is_err());
        assert_eq!(
            world
                .get_component::<AnimationPlayer>(entity)
                .unwrap()
                .blend_weight,
            0.75
        );
        execute(&mut world, request(entity, "Walk", 0.0, false)).unwrap();
        let player = world.get_component::<AnimationPlayer>(entity).unwrap();
        assert_eq!(player.current_clip.as_deref(), Some("Walk"));
        assert!(!player.blending);
        assert_eq!(player.time, 0.0);
        assert_eq!(player.target_time, 0.0);
        world.destroy_entity(entity);
        assert!(execute(&mut world, request(entity, "Run", 0.0, true)).is_err());
    }

    #[test]
    fn test_missing_player_and_zero_speed_have_defined_behavior() {
        let (mut world, entity) = world();
        world.remove_component::<AnimationPlayer>(entity);
        let op = serde_json::from_value(
            json!({"action":"play","entity_id":entity.id(),"clip":"Run","speed":0}),
        )
        .unwrap();
        execute(&mut world, op).unwrap();
        assert!(
            !world
                .get_component::<AnimationPlayer>(entity)
                .unwrap()
                .blending
        );
        execute(
            &mut world,
            AnimationOp::Play {
                entity_id: entity.id(),
                clip: "Walk".into(),
                fade_seconds: 0.5,
                looping: false,
                speed: 0.0,
            },
        )
        .unwrap();
        world.update(0.5);
        let player = world.get_component::<AnimationPlayer>(entity).unwrap();
        assert_eq!(player.current_clip.as_deref(), Some("Walk"));
        assert_eq!(player.time, 0.0);
        assert!(!player.blending);
    }

    #[test]
    fn test_scene_snapshot_preserves_pending_fade_and_completion_events() {
        let (mut world, entity) = world();
        execute(&mut world, request(entity, "Run", 1.0, false)).unwrap();
        world.update(0.25);
        let snapshot = crate::scene::AnimationDescriptor::from(
            world.get_component::<AnimationPlayer>(entity).unwrap(),
        );
        let snapshot: crate::scene::AnimationDescriptor =
            serde_json::from_value(serde_json::to_value(snapshot).unwrap()).unwrap();
        let mut restored = AnimationPlayer::stopped();
        snapshot.restore(&mut restored);
        world.add_component(entity, restored);
        for invalid_dt in [-1.0, f32::NAN, f32::INFINITY] {
            world.update(invalid_dt);
        }
        assert_eq!(
            world
                .get_component::<AnimationPlayer>(entity)
                .unwrap()
                .blend_weight,
            0.75
        );
        world.update(0.75);
        let player = world.get_component::<AnimationPlayer>(entity).unwrap();
        assert_eq!(player.current_clip.as_deref(), Some("Run"));
        assert_eq!(player.time, 0.2);
        assert!(player.is_complete());
        assert!(player.events.is_empty());
    }
}
