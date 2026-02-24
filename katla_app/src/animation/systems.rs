use crate::animation::components::{
    AnimatedModel, AnimationEvent, AnimationPlayer, MorphTargetWeights,
};
use crate::animation::skin::{Skeleton, Skin};
use crate::animation::{ChannelPath, SampleBuffer, SampledValue};
use katla_ecs::{EntityId, System, World};
use katla_math::{Mat4, Quat, Vec3};

pub struct AnimationUpdateSystem;

impl System for AnimationUpdateSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        let player_entities: Vec<EntityId> = world
            .query::<&AnimationPlayer>()
            .map(|(entity, _player)| entity)
            .collect();

        for player_entity in player_entities {
            let model_entity = Self::find_animated_model(world, player_entity);

            if let Some(model_entity) = model_entity {
                let clip_info: Option<(String, f32)> = world
                    .get_component::<AnimatedModel>(model_entity)
                    .and_then(|model| {
                        world
                            .get_component::<AnimationPlayer>(player_entity)
                            .and_then(|player| {
                                player.current_clip.as_ref().and_then(|name| {
                                    model
                                        .animations
                                        .get(name)
                                        .map(|clip| (name.clone(), clip.duration))
                                })
                            })
                    });

                let target_clip_info: Option<(String, f32)> = if world
                    .get_component::<AnimationPlayer>(player_entity)
                    .map(|p| p.blending)
                    .unwrap_or(false)
                {
                    world
                        .get_component::<AnimatedModel>(model_entity)
                        .and_then(|model| {
                            world
                                .get_component::<AnimationPlayer>(player_entity)
                                .and_then(|player| {
                                    player.target_clip.as_ref().and_then(|name| {
                                        model
                                            .animations
                                            .get(name)
                                            .map(|clip| (name.clone(), clip.duration))
                                    })
                                })
                        })
                } else {
                    None
                };

                if let Some(player) = world.get_component_mut::<AnimationPlayer>(player_entity) {
                    if player.playing {
                        if let Some((_, duration)) = &clip_info {
                            player.duration = *duration;
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
                            if let Some((_, target_duration)) = &target_clip_info {
                                player.target_duration = *target_duration;
                            }

                            player.target_time += delta_time * player.speed;
                            player.blend_time += delta_time;

                            if player.target_time >= player.target_duration
                                && player.target_duration > 0.0
                            {
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
                                player.blend_weight =
                                    1.0 - (player.blend_time / player.blend_duration);
                            }
                        }
                    }
                }
            }
        }
    }

    fn name(&self) -> &str {
        "AnimationUpdateSystem"
    }
}

impl AnimationUpdateSystem {
    fn find_animated_model(world: &World, entity: EntityId) -> Option<EntityId> {
        if world.get_component::<AnimatedModel>(entity).is_some() {
            return Some(entity);
        }
        None
    }
}

#[derive(Default)]
pub struct SkeletalAnimationSystem {
    sample_buffer: SampleBuffer,
}

impl System for SkeletalAnimationSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        let entities: Vec<EntityId> = world
            .query::<(&AnimatedModel, &Skin, &AnimationPlayer)>()
            .map(|(entity, _, _, _)| entity)
            .collect();

        for entity in entities {
            // Fetch all components once to avoid repeated lookups
            let (player_info, skin_info, model_info) = {
                let player = world.get_component::<AnimationPlayer>(entity);
                let skin = world.get_component::<Skin>(entity);
                let model = world.get_component::<AnimatedModel>(entity);

                // Check if all components exist
                if player.is_none() || skin.is_none() || model.is_none() {
                    continue;
                }

                let player = player.unwrap();
                let skin = skin.unwrap();
                let model = model.unwrap();

                // Early exit if not playing
                if !player.playing {
                    continue;
                }

                // Get clip name
                let clip_name = match &player.current_clip {
                    Some(name) => name.clone(),
                    None => continue,
                };

                // Check if clip exists
                if !model.animations.contains_key(&clip_name) {
                    continue;
                }

                // Sample the animation before moving clip_name into the tuple
                let sampled = model
                    .animations
                    .get(&clip_name)
                    .map(|c| c.sample(player.time));

                (
                    (player.time, clip_name),
                    (
                        skin.joint_count(),
                        skin.joints.clone(),
                        skin.inverse_bind_matrices.clone(),
                    ),
                    sampled,
                )
            };

            let (_player_time, _clip_name) = player_info;
            let (joint_count, skin_joints, inverse_bind_matrices) = skin_info;
            let sampled_values = model_info.unwrap_or_default();

            // Ensure skeleton component exists
            if world.get_component::<Skeleton>(entity).is_none() {
                world.add_component(entity, Skeleton::new("skeleton", joint_count));
            }

            if let Some(skeleton) = world.get_component_mut::<Skeleton>(entity) {
                // Step 1: Update LOCAL transforms from animation samples
                for (node_index, path, value) in sampled_values {
                    if let Some(joint_index) = skin_joints.iter().position(|&j| j == node_index) {
                        if joint_index < skeleton.local_transforms.len() {
                            let transform = &skeleton.local_transforms[joint_index];
                            let decomposed = transform.decompose();

                            let new_transform = match (path, value) {
                                (ChannelPath::Translation, SampledValue::Vec3(t)) => {
                                    let t_vec = Vec3::new(t[0], t[1], t[2]);
                                    Mat4::from_trs(t_vec, decomposed.rotation, decomposed.scale)
                                }
                                (ChannelPath::Rotation, SampledValue::Quat(q)) => {
                                    let q_quat = Quat::new_from_xyzw(q[0], q[1], q[2], q[3]);
                                    Mat4::from_trs(decomposed.position, q_quat, decomposed.scale)
                                }
                                (ChannelPath::Scale, SampledValue::Vec3(s)) => {
                                    let s_vec = Vec3::new(s[0], s[1], s[2]);
                                    Mat4::from_trs(decomposed.position, decomposed.rotation, s_vec)
                                }
                                _ => continue,
                            };

                            skeleton.local_transforms[joint_index] = new_transform;
                        }
                    }
                }

                // Step 2: Compute WORLD transforms from hierarchy
                skeleton.compute_world_transforms();

                // Step 3: Compute final skinning matrices (world * IBM)
                skeleton.compute_skinning_matrices(&inverse_bind_matrices);
            }
        }
    }

    fn name(&self) -> &str {
        "SkeletalAnimationSystem"
    }
}

pub struct MorphTargetSystem;

impl System for MorphTargetSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        let entities: Vec<EntityId> = world
            .query::<(&AnimatedModel, &AnimationPlayer, &MorphTargetWeights)>()
            .map(|(entity, _, _, _)| entity)
            .collect();

        for entity in entities {
            // Fetch components once and extract needed data
            let sampled_values = {
                let player = world.get_component::<AnimationPlayer>(entity);
                let model = world.get_component::<AnimatedModel>(entity);

                match (player, model) {
                    (Some(player), Some(model)) => {
                        // Early exit if not playing
                        if !player.playing {
                            continue;
                        }

                        // Get clip name and sample
                        match &player.current_clip {
                            Some(clip_name) => model
                                .animations
                                .get(clip_name)
                                .map(|clip| clip.sample(player.time))
                                .unwrap_or_default(),
                            None => continue,
                        }
                    }
                    _ => continue,
                }
            };

            if let Some(morph_weights) = world.get_component_mut::<MorphTargetWeights>(entity) {
                for (_node_index, path, value) in sampled_values {
                    if path == ChannelPath::Weights {
                        if let SampledValue::Float(weight) = value {
                            for w in morph_weights.weights.iter_mut() {
                                *w = weight;
                            }
                        }
                    }
                }
            }
        }
    }

    fn name(&self) -> &str {
        "MorphTargetSystem"
    }
}
