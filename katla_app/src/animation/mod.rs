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
}
