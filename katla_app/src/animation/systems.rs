use crate::animation::components::{AnimatedModel, AnimationPlayer, MorphTargetWeights};
use crate::animation::skin::{Skeleton, Skin};
use crate::animation::{ChannelPath, SampledValue};
use katla_ecs::{EntityId, System, World};
use katla_math::{Mat4, Quat, Vec3};

/// Updates animation players based on elapsed time.
///
/// This system handles:
/// - Advancing animation time for playing animations
/// - Looping animations that have finished
/// - Resetting animations that have finished (if not looping)
///
/// Runs before skeletal animation system to update player states.
pub struct AnimationUpdateSystem;

impl System for AnimationUpdateSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        // Collect all entities with AnimationPlayer
        let player_entities: Vec<EntityId> = world
            .query::<&AnimationPlayer>()
            .map(|(entity, _player)| entity)
            .collect();

        // Process each player separately to avoid borrow conflicts
        for player_entity in player_entities {
            // Find the animated model for this player
            let model_entity = Self::find_animated_model(world, player_entity);

            if let Some(model_entity) = model_entity {
                // Clone the animation data we need
                let animation_data = world
                    .get_component::<AnimatedModel>(model_entity)
                    .map(|model| model.animations.clone());

                // Update the player using the cloned data
                if let (Some(player), Some(animations)) = (
                    world.get_component_mut::<AnimationPlayer>(player_entity),
                    animation_data,
                ) {
                    // Create a temporary AnimatedModel reference for the update function
                    if let Some(clip_name) = &player.current_clip {
                        if let Some(clip) = animations.get(clip_name) {
                            // Update the player time directly
                            if player.playing {
                                player.time += delta_time * player.speed;

                                if player.time >= clip.duration {
                                    if player.loop_animation {
                                        player.time %= clip.duration;
                                    } else {
                                        player.time = clip.duration;
                                        player.playing = false;
                                    }
                                }
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
    /// Find the AnimatedModel component for an entity.
    ///
    /// This could be on the same entity or a related entity.
    fn find_animated_model(world: &World, entity: EntityId) -> Option<EntityId> {
        // First check if the entity itself has AnimatedModel
        if world.get_component::<AnimatedModel>(entity).is_some() {
            return Some(entity);
        }

        // TODO: Check parent/child relationships via Parent/Children components
        // For now, return None
        None
    }
}

/// Applies skeletal animation transforms to the scene.
///
/// This system:
/// - Samples animation clips at the current time
/// - Computes joint transforms for the skeleton
/// - Updates joint matrices for vertex skinning
///
/// Requires animation data to be loaded from GLTF files first.
pub struct SkeletalAnimationSystem;

impl System for SkeletalAnimationSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        // Find all entities with AnimatedModel, Skin, and AnimationPlayer components
        let entities: Vec<EntityId> = world
            .query::<(&AnimatedModel, &Skin, &AnimationPlayer)>()
            .map(|(entity, _, _, _)| entity)
            .collect();

        for entity in entities {
            // Get the components
            let (animated_model, skin, player) = match (
                world.get_component::<AnimatedModel>(entity),
                world.get_component::<Skin>(entity),
                world.get_component::<AnimationPlayer>(entity),
            ) {
                (Some(model), Some(s), Some(p)) => (model, s, p),
                _ => continue,
            };

            // Only process if playing and we have a current clip
            if !player.playing {
                continue;
            }

            let clip_name = match &player.current_clip {
                Some(name) => name,
                None => continue,
            };

            let clip = match animated_model.animations.get(clip_name) {
                Some(c) => c,
                None => continue,
            };

            // Sample the animation at the current time
            let sampled_values = clip.sample(player.time);

            // Create or get skeleton component
            let skeleton = if let Some(skel) = world.get_component::<Skeleton>(entity) {
                skel.clone()
            } else {
                Skeleton::new("skeleton", skin.joint_count())
            };

            // Apply animation samples to joint transforms
            let mut joint_transforms = skeleton.joint_transforms;

            for (node_index, path, value) in sampled_values {
                // Find which joint this node corresponds to
                if let Some(joint_index) = skin.joints.iter().position(|&j| j == node_index) {
                    if joint_index < joint_transforms.len() {
                        let transform = joint_transforms[joint_index].clone();
                        let transform_decomposed = transform.decompose();

                        let new_transform = match (path, value) {
                            (ChannelPath::Translation, SampledValue::Vec3(t)) => {
                                let t_vec = Vec3::new(t[0], t[1], t[2]);
                                Mat4::from_trs(
                                    t_vec,
                                    transform_decomposed.rotation,
                                    transform_decomposed.scale,
                                )
                            }
                            (ChannelPath::Rotation, SampledValue::Quat(q)) => {
                                let q_quat = Quat::new_from_xyzw(q[0], q[1], q[2], q[3]);
                                Mat4::from_trs(
                                    transform_decomposed.position,
                                    q_quat,
                                    transform_decomposed.scale,
                                )
                            }
                            (ChannelPath::Scale, SampledValue::Vec3(s)) => {
                                let s_vec = Vec3::new(s[0], s[1], s[2]);
                                Mat4::from_trs(
                                    transform_decomposed.position,
                                    transform_decomposed.rotation,
                                    s_vec,
                                )
                            }
                            _ => transform,
                        };

                        joint_transforms[joint_index] = new_transform;
                    }
                }
            }

            // Apply inverse bind matrices
            for (i, joint_transform) in joint_transforms.iter_mut().enumerate() {
                if i < skin.inverse_bind_matrices.len() {
                    *joint_transform = joint_transform.clone() * skin.inverse_bind_matrices[i].clone();
                }
            }

            // Update skeleton component
            let mut updated_skeleton = Skeleton::new("skeleton", joint_transforms.len());
            updated_skeleton.joint_transforms = joint_transforms;

            // TODO: In a real implementation, upload joint matrices to GPU uniform buffer
            // This would typically involve:
            // 1. Getting the VulkanRenderer from world
            // 2. Updating a uniform buffer with joint matrices
            // 3. Binding the buffer to the vertex shader for skinning

            world.add_component(entity, updated_skeleton);
        }
    }

    fn name(&self) -> &str {
        "SkeletalAnimationSystem"
    }
}

/// Applies morph target animations to meshes.
///
/// Morph targets are used for:
/// - Facial animations
/// - Shape blending
/// - Mesh deformation
pub struct MorphTargetSystem;

impl System for MorphTargetSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        // Find all entities with AnimatedModel, AnimationPlayer, and MorphTargetWeights components
        let entities: Vec<EntityId> = world
            .query::<(&AnimatedModel, &AnimationPlayer, &MorphTargetWeights)>()
            .map(|(entity, _, _, _)| entity)
            .collect();

        for entity in entities {
            // Get the components - clone immutable data first, then get mutable
            let (animated_model_clone, player_clone) = match (
                world.get_component::<AnimatedModel>(entity),
                world.get_component::<AnimationPlayer>(entity),
            ) {
                (Some(model), Some(player)) => (model.animations.clone(), player.clone()),
                _ => continue,
            };

            let mut morph_weights = match world.get_component_mut::<MorphTargetWeights>(entity) {
                Some(w) => w,
                None => continue,
            };

            // Only process if playing and we have a current clip
            if !player_clone.playing {
                continue;
            }

            let clip_name = match &player_clone.current_clip {
                Some(name) => name,
                None => continue,
            };

            let clip = match animated_model_clone.get(clip_name) {
                Some(c) => c,
                None => continue,
            };

            // Sample the animation at the current time
            let sampled_values = clip.sample(player_clone.time);

            // Apply weight animations to morph target weights
            for (_node_index, path, value) in sampled_values {
                if path == ChannelPath::Weights {
                    if let SampledValue::Float(weight) = value {
                        // For now, apply to all weights
                        // TODO: In a real implementation, this would need to map specific
                        // animation channels to specific morph target indices
                        for i in 0..morph_weights.weights.len() {
                            morph_weights.weights[i] = weight;
                        }
                    }
                }
            }

            // TODO: In a real implementation, this would need to:
            // 1. Interpolate vertex positions based on morph targets
            // 2. Re-upload vertex buffer to GPU when weights change
            // 3. Update material uniforms with morph target weights array
        }
    }

    fn name(&self) -> &str {
        "MorphTargetSystem"
    }
}
