//! Animation system for skeletal and transform-based animations.
//!
//! This module provides:
//! - Animation clip loading from GLTF files
//! - Skeletal animation with skinning
//! - Transform animation (translation, rotation, scale)
//! - Animation playback control (play, pause, loop, speed)
//! - Animation blending between multiple clips
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
//! // Animation system will update joint transforms each frame
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

// Re-export commonly used types
pub use components::{AnimationPlayer, AnimatedModel, MorphTargetWeights};
pub use clips::{AnimationClip, AnimationChannel, ChannelPath};
pub use systems::{AnimationUpdateSystem, SkeletalAnimationSystem};

use katla_ecs::World;

/// Animation system manager
///
/// High-level API for loading and managing animated models.
pub struct AnimationManager;

impl AnimationManager {
    /// Load animations from a GLTF model into the world
    pub fn load_gltf_animations(world: &mut World, model: &crate::util::GLTFModel) {
        // Delegate to GLTF loader
        gltf_loader::load_animations(world, model);
    }
}
