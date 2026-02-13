//! Animation system for skeletal and transform-based animations.
//!
//! This module provides:
//! - Animation clip loading from GLTF files
//! - Skeletal animation with skinning
//! - Transform animation (translation, rotation, scale)
//! - Animation playback control (play, pause, loop, speed)
//! - Animation blending between multiple clips
//! - Animation events (completion, loop)
//!
//! # Example
//!
//! ```ignore
//! // Load an animated model
//! let model = load_animated_gltf("Fox.glb", &mut world);
//!
//! // Play an animation
//! world.add_component(entity, AnimationPlayer::new("Walk").looping());
//!
//! // Crossfade to another animation
//! if let Some(player) = world.get_component_mut::<AnimationPlayer>(entity) {
//!     player.crossfade_to("Run", 1.5, 0.5); // 0.5 second blend
//! }
//!
//! // Check for animation events
//! if let Some(player) = world.get_component_mut::<AnimationPlayer>(entity) {
//!     for event in player.take_events() {
//!         match event {
//!             AnimationEvent::Completed { clip_name } => println!("{} finished", clip_name),
//!             AnimationEvent::Looped { clip_name, loop_count } => println!("{} loop {}", clip_name, loop_count),
//!         }
//!     }
//! }
//! ```

pub mod clips;
pub mod components;
pub mod gltf_loader;
pub mod samplers;
pub mod skin;
pub mod systems;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;

pub use clips::{
    AnimationChannel, AnimationClip, AnimationSampler, ChannelPath, SampleBuffer, SampledValue,
};
pub use components::{
    AnimatedModel, AnimationEvent, AnimationPlayer, JointTransform, MorphTargetWeights,
};
pub use samplers::{CachedSampler, Interpolation};
pub use skin::{JointWeights, Skeleton, Skin};
pub use systems::{AnimationUpdateSystem, MorphTargetSystem, SkeletalAnimationSystem};

use katla_ecs::World;

/// Animation system manager
///
/// High-level API for loading and managing animated models.
pub struct AnimationManager;

impl AnimationManager {
    /// Load animations from a GLTF model into the world
    pub fn load_gltf_animations(world: &mut World, model: &crate::util::GLTFModel) {
        gltf_loader::load_animations(world, model);
    }

    /// Load skins from a GLTF model into the world
    pub fn load_gltf_skins(world: &mut World, model: &crate::util::GLTFModel) {
        gltf_loader::load_skins(world, model);
    }

    /// Set up an animated model entity with all required components for skeletal animation.
    ///
    /// This loads animations and skins onto the given entity, making it ready for
    /// playback with `SkeletalAnimationSystem`.
    ///
    /// # Arguments
    /// * `world` - The ECS world
    /// * `entity` - The entity to add components to (usually the model entity)
    /// * `model` - The GLTF model containing animations and skins
    /// * `default_animation` - Optional name of animation to play by default
    ///
    /// # Returns
    /// `true` if animation data was loaded, `false` if model has no animations
    pub fn setup_animated_model(
        world: &mut World,
        entity: katla_ecs::EntityId,
        model: &crate::util::GLTFModel,
        default_animation: Option<&str>,
    ) -> bool {
        let document = &model.document;

        // Check if model has animations
        let animations: Vec<_> = document.animations().collect();
        if animations.is_empty() {
            log::info!("Model has no animations, skipping animation setup");
            return false;
        }

        // Load animations into AnimatedModel component
        let mut animated_model = AnimatedModel {
            animations: std::collections::HashMap::new(),
            sequences: std::collections::HashMap::new(),
        };

        for (index, gltf_animation) in animations.iter().enumerate() {
            let name = gltf_animation
                .name()
                .unwrap_or(&format!("Animation_{}", index))
                .to_string();

            log::info!("Loading animation '{}' for entity {:?}", name, entity);

            let clip = gltf_loader::load_animation_clip(&model.buffers, gltf_animation);
            animated_model.animations.insert(name, clip);
        }

        world.add_component(entity, animated_model);

        // Load skins into Skin component
        let skins: Vec<_> = document.skins().collect();
        if let Some(gltf_skin) = skins.first() {
            let joints: Vec<usize> = gltf_skin.joints().map(|node| node.index()).collect();
            let inverse_bind_matrices = if let Some(accessor) = gltf_skin.inverse_bind_matrices() {
                gltf_loader::parse_mat4_from_accessor(&model.buffers, accessor)
            } else {
                vec![katla_math::Mat4::identity(); joints.len()]
            };

            let skin = Skin::new("main_skin".to_string(), joints, inverse_bind_matrices);
            world.add_component(entity, skin);

            // Create skeleton component
            world.add_component(entity, Skeleton::new("skeleton", gltf_skin.joints().count()));
        }

        // Add AnimationPlayer if default animation specified
        if let Some(anim_name) = default_animation {
            let player = AnimationPlayer::new(anim_name).looping();
            world.add_component(entity, player);
            log::info!("Started playing animation '{}' on entity {:?}", anim_name, entity);
        }

        true
    }
}
