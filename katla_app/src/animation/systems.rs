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

            player.time += delta_time * player.speed;

            if player.time >= player.duration {
                if player.loop_animation {
                    player.time %= player.duration;
                    player.loop_count += 1;
                    player.events.push(AnimationEvent::Looped {
                        clip_name: player.current_clip.clone().unwrap_or_default(),
                        loop_count: player.loop_count,
                    });
                } else {
                    player.time = player.duration;
                    player.playing = false;
                    player.events.push(AnimationEvent::Completed {
                        clip_name: player.current_clip.clone().unwrap_or_default(),
                    });
                }
            }

            if player.blending {
                if let Some(target_duration) = target_clip_duration {
                    player.target_duration = target_duration;
                }

                player.target_time += delta_time * player.speed;
                player.blend_time += delta_time;

                if player.target_time >= player.target_duration && player.target_duration > 0.0 {
                    player.target_time %= player.target_duration;
                }

                if player.blend_time >= player.blend_duration {
                    if let Some(target) = player.target_clip.take() {
                        player.current_clip = Some(target);
                        player.duration = player.target_duration;
                        player.time = player.target_time;
                    }
                    player.target_clip = None;
                    player.target_duration = 0.0;
                    player.target_time = 0.0;
                    player.blend_time = 0.0;
                    player.blending = false;
                    player.blend_weight = 1.0;
                } else if player.blend_duration > 0.0 {
                    player.blend_weight = 1.0 - (player.blend_time / player.blend_duration);
                }
            }
        }
    }

    fn name(&self) -> &str {
        "AnimationUpdateSystem"
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
