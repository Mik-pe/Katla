use crate::animation::components::{
    AnimatedModel, AnimationEvent, AnimationPlayer, MorphTargetWeights,
};
use crate::animation::{ChannelPath, SampledValue};
use katla_ecs::{ComponentAccess, EntityId, System};

pub struct AnimationUpdateSystem;

impl System for AnimationUpdateSystem {
    fn update(&mut self, world: &mut katla_ecs::World, delta_time: f32) {
        struct PlayerData {
            entity: EntityId,
            clip_duration: Option<f32>,
            target_clip_duration: Option<f32>,
            blending: bool,
        }

        let players: Vec<PlayerData> = world
            .query::<(&AnimationPlayer, &AnimatedModel)>()
            .map(|(entity, player, model)| {
                let clip_duration = player
                    .current_clip
                    .as_ref()
                    .and_then(|name| model.animations.get(name))
                    .map(|clip| clip.duration);

                let target_clip_duration = if player.blending {
                    player
                        .target_clip
                        .as_ref()
                        .and_then(|name| model.animations.get(name))
                        .map(|clip| clip.duration)
                } else {
                    None
                };

                PlayerData {
                    entity,
                    clip_duration,
                    target_clip_duration,
                    blending: player.blending,
                }
            })
            .collect();

        for data in players {
            let Some(player) = world.get_component_mut::<AnimationPlayer>(data.entity) else {
                continue;
            };

            if !player.playing {
                continue;
            }

            if let Some(duration) = data.clip_duration {
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

            if data.blending {
                if let Some(target_duration) = data.target_clip_duration {
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

    fn component_access() -> Vec<ComponentAccess> {
        vec![
            ComponentAccess::read::<AnimatedModel>(),
            ComponentAccess::write::<AnimationPlayer>(),
        ]
    }

    fn component_access_dyn(&self) -> Vec<ComponentAccess> {
        vec![
            ComponentAccess::read::<AnimatedModel>(),
            ComponentAccess::write::<AnimationPlayer>(),
        ]
    }
}

pub struct MorphTargetSystem;

impl System for MorphTargetSystem {
    fn update(&mut self, world: &mut katla_ecs::World, _delta_time: f32) {
        struct PendingUpdate {
            entity: katla_ecs::EntityId,
            weight: f32,
        }

        let mut pending: Vec<PendingUpdate> = Vec::new();

        for (entity, player, model, _morph) in
            world.query::<(&AnimationPlayer, &AnimatedModel, &MorphTargetWeights)>()
        {
            if !player.playing {
                continue;
            }

            let Some(clip_name) = &player.current_clip else {
                continue;
            };

            let time = player.time;
            let sampled_values = model
                .animations
                .get(clip_name)
                .map(|clip| clip.sample(time))
                .unwrap_or_default();

            for (_node_index, path, value) in sampled_values {
                if path == ChannelPath::Weights
                    && let SampledValue::Float(weight) = value
                {
                    pending.push(PendingUpdate { entity, weight });
                    break;
                }
            }
        }

        for update in pending {
            if let Some(morph_weights) =
                world.get_component_mut::<MorphTargetWeights>(update.entity)
            {
                for w in morph_weights.weights.iter_mut() {
                    *w = update.weight;
                }
            }
        }
    }

    fn name(&self) -> &str {
        "MorphTargetSystem"
    }

    fn component_access() -> Vec<ComponentAccess> {
        vec![
            ComponentAccess::read::<AnimationPlayer>(),
            ComponentAccess::read::<AnimatedModel>(),
            ComponentAccess::write::<MorphTargetWeights>(),
        ]
    }

    fn component_access_dyn(&self) -> Vec<ComponentAccess> {
        vec![
            ComponentAccess::read::<AnimationPlayer>(),
            ComponentAccess::read::<AnimatedModel>(),
            ComponentAccess::write::<MorphTargetWeights>(),
        ]
    }
}
