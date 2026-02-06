use super::samplers::Interpolation;
use std::fmt;

/// A complete animation clip that can be played on an animated model.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    /// Name of this animation clip
    pub name: String,
    /// Duration in seconds
    pub duration: f32,
    /// Animation channels (one per animated property)
    pub channels: Vec<AnimationChannel>,
}

/// An animation channel animates a specific property of a node.
#[derive(Debug, Clone)]
pub struct AnimationChannel {
    /// Index of the target node (joint/mesh) in the GLTF scene
    pub target_node: usize,
    /// What property is being animated
    pub path: ChannelPath,
    /// Keyframe sampler for this channel
    pub sampler: AnimationSampler,
}

/// Properties that can be animated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelPath {
    /// Translation (position in 3D space)
    Translation,
    /// Rotation (quaternion orientation)
    Rotation,
    /// Scale (size)
    Scale,
    /// Morph target weights
    Weights,
}

impl fmt::Display for ChannelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelPath::Translation => write!(f, "translation"),
            ChannelPath::Rotation => write!(f, "rotation"),
            ChannelPath::Scale => write!(f, "scale"),
            ChannelPath::Weights => write!(f, "weights"),
        }
    }
}

/// Animation sampler containing keyframe data and interpolation method.
#[derive(Debug, Clone)]
pub struct AnimationSampler {
    /// Input (time) values for keyframes
    pub inputs: Vec<f32>,
    /// Output values as typed arrays
    ///
    /// We store separate typed vectors to avoid unsafe casting
    pub translations: Option<Vec<[f32; 3]>>,
    pub rotations: Option<Vec<[f32; 4]>>,
    pub scales: Option<Vec<[f32; 3]>>,
    pub weights: Option<Vec<f32>>,
    /// Interpolation method
    pub interpolation: Interpolation,
}

impl AnimationSampler {
    /// Create a new animation sampler for translation animations
    pub fn new_translation(inputs: Vec<f32>, translations: Vec<[f32; 3]>, interpolation: Interpolation) -> Self {
        Self {
            inputs,
            translations: Some(translations),
            rotations: None,
            scales: None,
            weights: None,
            interpolation,
        }
    }

    /// Create a new animation sampler for rotation animations
    pub fn new_rotation(inputs: Vec<f32>, rotations: Vec<[f32; 4]>, interpolation: Interpolation) -> Self {
        Self {
            inputs,
            translations: None,
            rotations: Some(rotations),
            scales: None,
            weights: None,
            interpolation,
        }
    }

    /// Create a new animation sampler for scale animations
    pub fn new_scale(inputs: Vec<f32>, scales: Vec<[f32; 3]>, interpolation: Interpolation) -> Self {
        Self {
            inputs,
            translations: None,
            rotations: None,
            scales: Some(scales),
            weights: None,
            interpolation,
        }
    }

    /// Create a new animation sampler for morph target weights
    pub fn new_weights(inputs: Vec<f32>, weights: Vec<f32>, interpolation: Interpolation) -> Self {
        Self {
            inputs,
            translations: None,
            rotations: None,
            scales: None,
            weights: Some(weights),
            interpolation,
        }
    }

    /// Get the number of keyframes
    pub fn keyframe_count(&self) -> usize {
        self.inputs.len()
    }

    /// Get the duration of this sampler
    pub fn duration(&self) -> f32 {
        self.inputs.last().copied().unwrap_or(0.0)
    }
}

/// Sampled animation value at a specific time.
#[derive(Debug, Clone, Copy)]
pub enum SampledValue {
    /// 3D vector (translation or scale)
    Vec3([f32; 3]),
    /// Quaternion (rotation)
    Quat([f32; 4]),
    /// Single scalar (morph target weight)
    Float(f32),
    /// Unknown format
    Unknown,
}
