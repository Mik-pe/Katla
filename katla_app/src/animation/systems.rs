use katla_ecs::{EntityId, System, World};
use crate::animation::components::{AnimationPlayer, AnimatedModel};

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
                let animation_data = world.get_component::<AnimatedModel>(model_entity).map(|model| {
                    model.animations.clone()
                });

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

        // TODO: Check parent/child relationships
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
    fn update(&mut self, _world: &mut World, _delta_time: f32) {
        // TODO: Implement skeletal animation
        // This requires:
        // 1. Sampling animation clips at current time
        // 2. Computing joint hierarchies
        // 3. Updating joint matrices for GPU skinning
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
    fn update(&mut self, _world: &mut World, _delta_time: f32) {
        // TODO: Implement morph target animation
        // This requires:
        // 1. Sampling weight animations
        // 2. Updating vertex positions based on morph targets
        // 3. Re-uploading vertex data to GPU
    }

    fn name(&self) -> &str {
        "MorphTargetSystem"
    }
}
