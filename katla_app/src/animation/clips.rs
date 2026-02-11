use super::samplers::Interpolation;
use katla_math::Quat;
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
    pub fn new_translation(
        inputs: Vec<f32>,
        translations: Vec<[f32; 3]>,
        interpolation: Interpolation,
    ) -> Self {
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
    pub fn new_rotation(
        inputs: Vec<f32>,
        rotations: Vec<[f32; 4]>,
        interpolation: Interpolation,
    ) -> Self {
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
    pub fn new_scale(
        inputs: Vec<f32>,
        scales: Vec<[f32; 3]>,
        interpolation: Interpolation,
    ) -> Self {
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

impl AnimationClip {
    /// Sample all channels at a specific time.
    ///
    /// Returns a vector of (node_index, ChannelPath, SampledValue) tuples.
    pub fn sample(&self, time: f32) -> Vec<(usize, ChannelPath, SampledValue)> {
        let mut results = Vec::new();
        for channel in &self.channels {
            let sampled = channel.sample(time);
            results.push((channel.target_node, channel.path, sampled));
        }
        results
    }
}

impl AnimationChannel {
    /// Sample this channel at a specific time.
    pub fn sample(&self, time: f32) -> SampledValue {
        self.sampler.sample(time)
    }
}

impl AnimationSampler {
    /// Sample this sampler at a specific time.
    pub fn sample(&self, time: f32) -> SampledValue {
        if self.inputs.is_empty() {
            return SampledValue::Unknown;
        }

        // Clamp time to sampler range
        let time = time.clamp(0.0, self.duration());

        match self.interpolation {
            Interpolation::Linear => self.sample_linear(time),
            Interpolation::Step => self.sample_step(time),
            Interpolation::CubicSpline => self.sample_cubic_spline(time),
        }
    }

    fn sample_linear(&self, time: f32) -> SampledValue {
        let index = self.find_keyframe_index(time);
        if index >= self.inputs.len() - 1 {
            return self.get_keyframe_value(index);
        }

        let t0 = self.inputs[index];
        let t1 = self.inputs[index + 1];
        let alpha = (time - t0) / (t1 - t0);

        self.interpolate_values(index, index + 1, alpha)
    }

    fn sample_step(&self, time: f32) -> SampledValue {
        let index = self.find_keyframe_index(time);
        self.get_keyframe_value(index)
    }

    fn sample_cubic_spline(&self, time: f32) -> SampledValue {
        // GLTF cubic spline stores data as: [tangent_out, value, tangent_in]
        // Each component has the same size (3 for vec3, 4 for quat, 1 for scalar)
        // We need to extract every 3rd element to get the actual values and tangents

        let index = self.find_keyframe_index(time);
        if index >= self.inputs.len() - 1 {
            return self.get_keyframe_value(index);
        }

        let t0 = self.inputs[index];
        let t1 = self.inputs[index + 1];
        let dt = t1 - t0;
        let t = (time - t0) / dt;

        // GLTF cubic spline: for each keyframe, data is [out_tangent, value, in_tangent]
        // Tangents need to be scaled by dt
        let t2 = t * t;
        let t3 = t2 * t;

        // Hermite basis functions
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;

        self.interpolate_cubic_spline(index, index + 1, h00, h10, h01, h11, dt)
    }

    fn interpolate_cubic_spline(
        &self,
        index0: usize,
        index1: usize,
        h00: f32,
        h10: f32,
        h01: f32,
        h11: f32,
        dt: f32,
    ) -> SampledValue {
        // GLTF cubic spline layout: [out_tangent, value, in_tangent] repeated for each keyframe
        // Stride is 3x the component size
        if let Some(ref translations) = self.translations {
            // Vec3 data: stride of 9 floats (3 for out_tangent, 3 for value, 3 for in_tangent)
            let v0 = [
                translations[index0 * 3 + 1][0],
                translations[index0 * 3 + 1][1],
                translations[index0 * 3 + 1][2],
            ];
            let v1 = [
                translations[index1 * 3 + 1][0],
                translations[index1 * 3 + 1][1],
                translations[index1 * 3 + 1][2],
            ];
            let m0 = [
                translations[index0 * 3 + 2][0] * dt,
                translations[index0 * 3 + 2][1] * dt,
                translations[index0 * 3 + 2][2] * dt,
            ];
            let m1 = [
                translations[index1 * 3 + 0][0] * dt,
                translations[index1 * 3 + 0][1] * dt,
                translations[index1 * 3 + 0][2] * dt,
            ];

            let result = [
                h00 * v0[0] + h10 * m0[0] + h01 * v1[0] + h11 * m1[0],
                h00 * v0[1] + h10 * m0[1] + h01 * v1[1] + h11 * m1[1],
                h00 * v0[2] + h10 * m0[2] + h01 * v1[2] + h11 * m1[2],
            ];
            SampledValue::Vec3(result)
        } else if let Some(ref rotations) = self.rotations {
            // Quaternions are more complex - we use slerp-like interpolation for cubic splines
            // For now, fall back to linear for quaternions as proper quaternion cubic spline
            // requires spherical interpolation
            self.interpolate_values(index0, index1, h00 + h01)
        } else if let Some(ref scales) = self.scales {
            let v0 = [
                scales[index0 * 3 + 1][0],
                scales[index0 * 3 + 1][1],
                scales[index0 * 3 + 1][2],
            ];
            let v1 = [
                scales[index1 * 3 + 1][0],
                scales[index1 * 3 + 1][1],
                scales[index1 * 3 + 1][2],
            ];
            let m0 = [
                scales[index0 * 3 + 2][0] * dt,
                scales[index0 * 3 + 2][1] * dt,
                scales[index0 * 3 + 2][2] * dt,
            ];
            let m1 = [
                scales[index1 * 3 + 0][0] * dt,
                scales[index1 * 3 + 0][1] * dt,
                scales[index1 * 3 + 0][2] * dt,
            ];

            let result = [
                h00 * v0[0] + h10 * m0[0] + h01 * v1[0] + h11 * m1[0],
                h00 * v0[1] + h10 * m0[1] + h01 * v1[1] + h11 * m1[1],
                h00 * v0[2] + h10 * m0[2] + h01 * v1[2] + h11 * m1[2],
            ];
            SampledValue::Vec3(result)
        } else if let Some(ref weights) = self.weights {
            // Scalar data: stride of 3 floats (out_tangent, value, in_tangent)
            let v0 = weights[index0 * 3 + 1];
            let v1 = weights[index1 * 3 + 1];
            let m0 = weights[index0 * 3 + 2] * dt;
            let m1 = weights[index1 * 3 + 0] * dt;

            let result = h00 * v0 + h10 * m0 + h01 * v1 + h11 * m1;
            SampledValue::Float(result)
        } else {
            SampledValue::Unknown
        }
    }

    fn find_keyframe_index(&self, time: f32) -> usize {
        match self
            .inputs
            .binary_search_by(|probe| probe.partial_cmp(&time).unwrap())
        {
            Ok(index) => index,
            Err(index) => {
                if index == 0 {
                    0
                } else if index >= self.inputs.len() {
                    self.inputs.len() - 1
                } else {
                    index - 1
                }
            }
        }
    }

    fn get_keyframe_value(&self, index: usize) -> SampledValue {
        if let Some(ref translations) = self.translations {
            SampledValue::Vec3(translations[index.min(translations.len() - 1)])
        } else if let Some(ref rotations) = self.rotations {
            SampledValue::Quat(rotations[index.min(rotations.len() - 1)])
        } else if let Some(ref scales) = self.scales {
            SampledValue::Vec3(scales[index.min(scales.len() - 1)])
        } else if let Some(ref weights) = self.weights {
            SampledValue::Float(weights[index.min(weights.len() - 1)])
        } else {
            SampledValue::Unknown
        }
    }

    fn interpolate_values(&self, index0: usize, index1: usize, alpha: f32) -> SampledValue {
        if let Some(ref translations) = self.translations {
            let v0 = translations[index0.min(translations.len() - 1)];
            let v1 = translations[index1.min(translations.len() - 1)];
            let result = [
                v0[0] + (v1[0] - v0[0]) * alpha,
                v0[1] + (v1[1] - v0[1]) * alpha,
                v0[2] + (v1[2] - v0[2]) * alpha,
            ];
            SampledValue::Vec3(result)
        } else if let Some(ref rotations) = self.rotations {
            let q0 = katla_math::Quat::new_from_xyzw(
                rotations[index0][0],
                rotations[index0][1],
                rotations[index0][2],
                rotations[index0][3],
            );
            let q1 = katla_math::Quat::new_from_xyzw(
                rotations[index1][0],
                rotations[index1][1],
                rotations[index1][2],
                rotations[index1][3],
            );
            let q_result = katla_math::Quat::slerp(q0, q1, alpha);
            let (x, y, z, w) = q_result.xyzw();
            SampledValue::Quat([x, y, z, w])
        } else if let Some(ref scales) = self.scales {
            let v0 = scales[index0.min(scales.len() - 1)];
            let v1 = scales[index1.min(scales.len() - 1)];
            let result = [
                v0[0] + (v1[0] - v0[0]) * alpha,
                v0[1] + (v1[1] - v0[1]) * alpha,
                v0[2] + (v1[2] - v0[2]) * alpha,
            ];
            SampledValue::Vec3(result)
        } else if let Some(ref weights) = self.weights {
            let w0 = weights[index0.min(weights.len() - 1)];
            let w1 = weights[index1.min(weights.len() - 1)];
            SampledValue::Float(w0 + (w1 - w0) * alpha)
        } else {
            SampledValue::Unknown
        }
    }
}
