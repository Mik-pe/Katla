use katla_ecs::Component;
use katla_math::Quat;
use std::collections::HashMap;

/// Animation player component that controls animation playback.
///
/// Attach this to an entity to play animations on its animated model.
#[derive(Component, Debug, Clone)]
pub struct AnimationPlayer {
    /// Name of the currently playing animation clip
    pub current_clip: Option<String>,
    /// Current playback time in seconds
    pub time: f32,
    /// Whether the animation is currently playing
    pub playing: bool,
    /// Whether to loop the animation when it finishes
    pub loop_animation: bool,
    /// Playback speed multiplier (1.0 = normal speed, 2.0 = 2x speed)
    pub speed: f32,
    /// Animation blend weight (0.0 - 1.0) for blending between clips
    pub blend_weight: f32,
}

impl AnimationPlayer {
    /// Create a new animation player for a specific clip
    pub fn new(clip_name: impl Into<String>) -> Self {
        Self {
            current_clip: Some(clip_name.into()),
            time: 0.0,
            playing: true,
            loop_animation: false,
            speed: 1.0,
            blend_weight: 1.0,
        }
    }

    /// Create a stopped animation player
    pub fn stopped() -> Self {
        Self {
            current_clip: None,
            time: 0.0,
            playing: false,
            loop_animation: false,
            speed: 1.0,
            blend_weight: 1.0,
        }
    }

    /// Set the animation clip to play
    pub fn with_clip(mut self, clip: impl Into<String>) -> Self {
        self.current_clip = Some(clip.into());
        self
    }

    /// Enable looping
    pub fn looping(mut self) -> Self {
        self.loop_animation = true;
        self
    }

    /// Set playback speed
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Start playing
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pause playback
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Stop playback and reset to beginning
    pub fn stop(&mut self) {
        self.playing = false;
        self.time = 0.0;
    }

    /// Jump to a specific time
    pub fn seek(&mut self, time: f32) {
        self.time = time.clamp(0.0, self.get_duration());
    }

    /// Get the duration of the current animation
    /// Returns 0.0 if no clip is set
    pub fn get_duration(&self) -> f32 {
        // This will be set by the system based on the clip
        0.0
    }
}

/// Animated model component containing all animation clips for a model.
///
/// This component stores the animation data loaded from GLTF files.
#[derive(Component, Debug, Clone)]
pub struct AnimatedModel {
    /// Map of animation clip names to their data
    pub animations: HashMap<String, super::clips::AnimationClip>,
    /// Named animation sequences (can combine multiple clips)
    pub sequences: HashMap<String, AnimationSequence>,
}

/// A sequence of animation clips that play in order or simultaneously.
#[derive(Debug, Clone)]
pub struct AnimationSequence {
    /// Clips in this sequence and their blend weights
    pub clips: Vec<SequenceClip>,
    /// Duration of the sequence (0.0 = calculated from clips)
    pub duration: f32,
    /// Whether to loop the sequence
    pub loop_sequence: bool,
}

#[derive(Debug, Clone)]
pub struct SequenceClip {
    /// Name of the animation clip
    pub name: String,
    /// Blend weight for this clip (0.0 - 1.0)
    pub weight: f32,
    /// Time offset into the clip
    pub time_offset: f32,
}

/// Morph target weights for mesh deformation.
///
/// Used for facial animations and shape blending.
#[derive(Component, Debug, Clone)]
pub struct MorphTargetWeights {
    /// Weights for each morph target (0.0 - 1.0)
    pub weights: Vec<f32>,
}

impl MorphTargetWeights {
    /// Create morph target weights with all zeros
    pub fn new(count: usize) -> Self {
        Self {
            weights: vec![0.0; count],
        }
    }

    /// Set a specific morph target weight
    pub fn set_weight(&mut self, index: usize, weight: f32) {
        if index < self.weights.len() {
            self.weights[index] = weight.clamp(0.0, 1.0);
        }
    }

    /// Get a specific morph target weight
    pub fn get_weight(&self, index: usize) -> f32 {
        self.weights.get(index).copied().unwrap_or(0.0)
    }
}

/// Joint transform for skeletal animation.
///
/// Represents the transform of a single joint in a skeleton.
#[derive(Debug, Copy, Clone)]
pub struct JointTransform {
    /// Translation
    pub translation: [f32; 3],
    /// Rotation (quaternion)
    pub rotation: Quat,
    /// Scale
    pub scale: [f32; 3],
}

impl JointTransform {
    /// Create an identity joint transform
    pub fn identity() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: Quat::new(),
            scale: [1.0, 1.0, 1.0],
        }
    }

    /// Create from translation only
    pub fn from_translation(translation: [f32; 3]) -> Self {
        Self {
            translation,
            rotation: Quat::new(),
            scale: [1.0, 1.0, 1.0],
        }
    }

    /// Interpolate between two joint transforms
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let qa = katla_math::Quat::new_from_xyzw(
            self.rotation[0],
            self.rotation[1],
            self.rotation[2],
            self.rotation[3],
        );
        let qb = katla_math::Quat::new_from_xyzw(
            other.rotation[0],
            other.rotation[1],
            other.rotation[2],
            other.rotation[3],
        );
        let q_result = katla_math::Quat::slerp(qa, qb, t);

        Self {
            translation: [
                self.translation[0] + (other.translation[0] - self.translation[0]) * t,
                self.translation[1] + (other.translation[1] - self.translation[1]) * t,
                self.translation[2] + (other.translation[2] - self.translation[2]) * t,
            ],
            rotation: q_result,
            scale: [
                self.scale[0] + (other.scale[0] - self.scale[0]) * t,
                self.scale[1] + (other.scale[1] - self.scale[1]) * t,
                self.scale[2] + (other.scale[2] - self.scale[2]) * t,
            ],
        }
    }
}

impl Default for JointTransform {
    fn default() -> Self {
        Self::identity()
    }
}
