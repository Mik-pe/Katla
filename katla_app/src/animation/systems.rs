use crate::animation::components::{
    AnimatedModel, AnimationEvent, AnimationPlayer, MorphTargetWeights,
};
use crate::animation::{ChannelPath, SampledValue};
use katla_ecs::{Query, Read, SystemParam, TypedSystem, Write};

pub struct AnimationUpdateSystem;

impl TypedSystem for AnimationUpdateSystem {
    type Params = Query<(Write<AnimationPlayer>, Read<AnimatedModel>)>;

    fn run(&mut self, mut players: <Self::Params as SystemParam>::Item<'_>, delta_time: f32) {
        for (_entity, (player, model)) in players.iter_mut() {
            let clip_duration = player
                .current_clip
                .as_ref()
                .and_then(|name| model.animations.get(name))
                .map(|clip| clip.duration);
            let target_clip_duration = player
                .target_clip
                .as_ref()
                .and_then(|name| model.animations.get(name))
                .map(|clip| clip.duration);
            if !player.playing {
                continue;
            }

            if let Some(duration) = clip_duration {
                player.duration = duration;
            }

            if !delta_time.is_finite()
                || delta_time <= 0.0
                || !player.speed.is_finite()
                || player.speed < 0.0
            {
                continue;
            }
            let advance = f64::from(delta_time) * f64::from(player.speed);
            if let Some(name) = &player.current_clip {
                advance_clip(
                    name,
                    ClipClock {
                        time: &mut player.time,
                        duration: player.duration,
                        looping: player.loop_animation,
                        completed: &mut player.completed,
                        loop_count: &mut player.loop_count,
                    },
                    advance,
                    &mut player.events,
                );
            }
            if player.blending {
                if let Some(duration) = target_clip_duration {
                    player.target_duration = duration;
                }
                if let Some(name) = &player.target_clip {
                    advance_clip(
                        name,
                        ClipClock {
                            time: &mut player.target_time,
                            duration: player.target_duration,
                            looping: player.target_loop_animation,
                            completed: &mut player.target_completed,
                            loop_count: &mut player.target_loop_count,
                        },
                        advance,
                        &mut player.events,
                    );
                }
                player.blend_time = (f64::from(player.blend_time) + f64::from(delta_time))
                    .min(f64::from(player.blend_duration))
                    as f32;
                if player.blend_time >= player.blend_duration {
                    if let Some(target) = player.target_clip.take() {
                        player.current_clip = Some(target);
                        player.duration = player.target_duration;
                        player.time = player.target_time;
                        player.completed = player.target_completed;
                        player.loop_animation = player.target_loop_animation;
                        player.loop_count = player.target_loop_count;
                    }
                    player.clear_transition();
                } else {
                    player.blend_weight = 1.0 - player.blend_time / player.blend_duration;
                }
            }
            if !player.blending && player.completed {
                player.playing = false;
            }
        }
    }

    fn name(&self) -> &str {
        "AnimationUpdateSystem"
    }
}

struct ClipClock<'a> {
    time: &'a mut f32,
    duration: f32,
    looping: bool,
    completed: &'a mut bool,
    loop_count: &'a mut u32,
}

#[inline]
fn advance_clip(name: &str, clock: ClipClock<'_>, advance: f64, events: &mut Vec<AnimationEvent>) {
    let ClipClock {
        time,
        duration,
        looping,
        completed,
        loop_count,
    } = clock;
    if *completed {
        return;
    }
    let duration = f64::from(duration.max(0.0));
    let next = f64::from(*time) + advance;
    if looping {
        if duration == 0.0 {
            *time = 0.0;
        } else if next >= duration {
            let loops = (next / duration).floor().min(f64::from(u32::MAX)) as u32;
            *time = (next % duration) as f32;
            *loop_count = loop_count.saturating_add(loops);
            events.push(AnimationEvent::Looped {
                clip_name: name.into(),
                loop_count: *loop_count,
            });
        } else {
            *time = next as f32;
        }
    } else {
        *time = next.min(duration) as f32;
        if next >= duration {
            *completed = true;
            events.push(AnimationEvent::Completed {
                clip_name: name.into(),
            });
        }
    }
}

pub struct MorphTargetSystem;

impl TypedSystem for MorphTargetSystem {
    type Params = Query<(
        Read<AnimationPlayer>,
        Read<AnimatedModel>,
        Write<MorphTargetWeights>,
    )>;

    fn run(&mut self, mut players: <Self::Params as SystemParam>::Item<'_>, _delta_time: f32) {
        for (_entity, (player, model, morph)) in players.iter_mut() {
            if !player.playing {
                continue;
            }

            let Some(clip_name) = &player.current_clip else {
                continue;
            };

            let Some(clip) = model.animations.get(clip_name) else {
                continue;
            };
            for channel in &clip.channels {
                if channel.path == ChannelPath::Weights
                    && let SampledValue::Float(weight) = channel.sample(player.time)
                {
                    morph.weights.fill(weight);
                    break;
                }
            }
        }
    }

    fn name(&self) -> &str {
        "MorphTargetSystem"
    }
}

#[cfg(test)]
mod typed_system_tests {
    use super::*;
    use crate::animation::{AnimationChannel, AnimationClip, AnimationSampler, Interpolation};
    use katla_ecs::{SystemExecutionOrder, World};
    use std::collections::HashMap;

    fn transition_world(
        source_duration: f32,
        target_duration: f32,
    ) -> (World, katla_ecs::EntityId) {
        let animations = [("source", source_duration), ("target", target_duration)]
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
            .collect();
        let mut world = World::new();
        let entity = world.spawn((
            AnimationPlayer::new("source"),
            AnimatedModel {
                animations,
                sequences: HashMap::new(),
            },
        ));
        world.register_typed_system(AnimationUpdateSystem, SystemExecutionOrder::NORMAL);
        (world, entity)
    }

    #[test]
    fn test_fade_outlives_nonlooping_source_and_clamps_target() {
        let (mut world, entity) = transition_world(0.1, 0.2);
        world
            .get_component_mut::<AnimationPlayer>(entity)
            .unwrap()
            .crossfade_to("target", 0.2, 1.0)
            .unwrap();
        for _ in 0..4 {
            world.update(0.25);
        }
        let player = world.get_component::<AnimationPlayer>(entity).unwrap();
        assert_eq!(player.current_clip.as_deref(), Some("target"));
        assert_eq!(player.time, 0.2);
        assert!(!player.playing);
        assert!(!player.blending);
        assert_eq!(
            player
                .events
                .iter()
                .filter(|event| matches!(event,
            AnimationEvent::Completed { clip_name } if clip_name == "source"))
                .count(),
            1
        );
        assert_eq!(
            player
                .events
                .iter()
                .filter(|event| matches!(event,
            AnimationEvent::Completed { clip_name } if clip_name == "target"))
                .count(),
            1
        );
    }

    #[test]
    fn test_constant_looping_clip_remains_finite_without_loop_events() {
        let (mut world, entity) = transition_world(0.0, 1.0);
        world
            .get_component_mut::<AnimationPlayer>(entity)
            .unwrap()
            .loop_animation = true;
        world.update(0.25);
        world.update(10.0);
        let player = world.get_component::<AnimationPlayer>(entity).unwrap();
        assert_eq!(player.time, 0.0);
        assert!(player.playing);
        assert!(player.events.is_empty());
    }

    #[test]
    fn test_pause_freezes_fade_and_completion_emits_once() {
        let (mut world, entity) = transition_world(1.0, 1.0);
        world
            .get_component_mut::<AnimationPlayer>(entity)
            .unwrap()
            .crossfade_to("target", 1.0, 0.5)
            .unwrap();
        world.update(0.25);
        world
            .get_component_mut::<AnimationPlayer>(entity)
            .unwrap()
            .pause();
        world.update(5.0);
        let player = world.get_component::<AnimationPlayer>(entity).unwrap();
        assert_eq!(player.blend_weight, 0.5);
        assert_eq!(player.target_time, 0.25);
        world
            .get_component_mut::<AnimationPlayer>(entity)
            .unwrap()
            .play();
        world.update(0.75);
        world.update(5.0);
        let player = world.get_component::<AnimationPlayer>(entity).unwrap();
        assert_eq!(player.current_clip.as_deref(), Some("target"));
        assert_eq!(player.time, 1.0);
        assert_eq!(
            player
                .events
                .iter()
                .filter(|event| matches!(event,
            AnimationEvent::Completed { clip_name } if clip_name == "target"))
                .count(),
            1
        );
    }

    fn model() -> AnimatedModel {
        let clip = AnimationClip {
            name: "weights".into(),
            duration: 1.0,
            channels: vec![AnimationChannel {
                target_node: 0,
                path: ChannelPath::Weights,
                sampler: AnimationSampler::new_weights(
                    vec![0.0, 1.0],
                    vec![0.0, 1.0],
                    Interpolation::Linear,
                ),
            }],
        };
        AnimatedModel {
            animations: HashMap::from([("weights".into(), clip)]),
            sequences: HashMap::new(),
        }
    }

    #[test]
    fn test_typed_animation_and_morph_preserve_order_and_playback() {
        for parallel in [false, true] {
            let mut world = World::new();
            let playing = world.spawn((
                AnimationPlayer::new("weights").looping(),
                model(),
                MorphTargetWeights::new(2),
            ));
            let paused = world.spawn((
                AnimationPlayer::stopped(),
                model(),
                MorphTargetWeights::new(2),
            ));
            world.register_typed_system(AnimationUpdateSystem, SystemExecutionOrder::NORMAL);
            world.register_typed_system(MorphTargetSystem, SystemExecutionOrder::NORMAL);
            for delta_time in [0.25, 1.0] {
                if parallel {
                    world.update_parallel(delta_time);
                } else {
                    world.update(delta_time);
                }
            }
            let player = world.get_component::<AnimationPlayer>(playing).unwrap();
            assert_eq!(player.time, 0.25);
            assert_eq!(player.loop_count, 1);
            assert_eq!(
                player.events,
                vec![AnimationEvent::Looped {
                    clip_name: "weights".into(),
                    loop_count: 1
                }]
            );
            assert_eq!(
                world
                    .get_component::<MorphTargetWeights>(playing)
                    .unwrap()
                    .weights,
                vec![0.25, 0.25]
            );
            assert_eq!(
                world.get_component::<AnimationPlayer>(paused).unwrap().time,
                0.0
            );
            assert_eq!(
                world
                    .get_component::<MorphTargetWeights>(paused)
                    .unwrap()
                    .weights,
                vec![0.0, 0.0]
            );
        }
    }
}
