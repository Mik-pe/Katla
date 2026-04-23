use super::samplers::Interpolation;
use std::fmt;

#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<AnimationChannel>,
}

#[derive(Debug, Clone)]
pub struct AnimationChannel {
    pub target_node: usize,
    pub path: ChannelPath,
    pub sampler: AnimationSampler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelPath {
    Translation,
    Rotation,
    Scale,
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

#[derive(Debug, Clone)]
pub struct AnimationSampler {
    pub inputs: Vec<f32>,
    pub translations: Option<Vec<[f32; 3]>>,
    pub rotations: Option<Vec<[f32; 4]>>,
    pub scales: Option<Vec<[f32; 3]>>,
    pub weights: Option<Vec<f32>>,
    pub interpolation: Interpolation,
}

#[derive(Debug, Clone, Copy)]
pub enum SampledValue {
    Vec3([f32; 3]),
    Quat([f32; 4]),
    Float(f32),
    Unknown,
}

pub struct SampleBuffer {
    samples: Vec<(usize, ChannelPath, SampledValue)>,
}

impl SampleBuffer {
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            samples: Vec::with_capacity(capacity),
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn samples(&self) -> &[(usize, ChannelPath, SampledValue)] {
        &self.samples
    }

    pub fn into_samples(self) -> Vec<(usize, ChannelPath, SampledValue)> {
        self.samples
    }
}

impl Default for SampleBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimationSampler {
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

    pub fn keyframe_count(&self) -> usize {
        self.inputs.len()
    }

    pub fn duration(&self) -> f32 {
        self.inputs.last().copied().unwrap_or(0.0)
    }
}

impl AnimationClip {
    pub fn sample(&self, time: f32) -> Vec<(usize, ChannelPath, SampledValue)> {
        let mut results = Vec::with_capacity(self.channels.len());
        for channel in &self.channels {
            let sampled = channel.sample(time);
            results.push((channel.target_node, channel.path, sampled));
        }
        results
    }

    pub fn sample_into(&self, time: f32, buffer: &mut SampleBuffer) {
        buffer.clear();
        buffer.samples.reserve(self.channels.len());
        for channel in &self.channels {
            let sampled = channel.sample(time);
            buffer
                .samples
                .push((channel.target_node, channel.path, sampled));
        }
    }

    pub fn get_duration(&self) -> f32 {
        self.duration
    }
}

impl AnimationChannel {
    pub fn sample(&self, time: f32) -> SampledValue {
        self.sampler.sample(time)
    }
}

/// Hermite basis functions for cubic spline interpolation.
struct HermiteBasis {
    h00: f32,
    h10: f32,
    h01: f32,
    h11: f32,
}

impl AnimationSampler {
    pub fn sample(&self, time: f32) -> SampledValue {
        if self.inputs.is_empty() {
            return SampledValue::Unknown;
        }

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
        let alpha = if (t1 - t0).abs() > f32::EPSILON {
            (time - t0) / (t1 - t0)
        } else {
            0.0
        };

        self.interpolate_values(index, index + 1, alpha)
    }

    fn sample_step(&self, time: f32) -> SampledValue {
        let index = self.find_keyframe_index(time);
        self.get_keyframe_value(index)
    }

    fn sample_cubic_spline(&self, time: f32) -> SampledValue {
        let index = self.find_keyframe_index(time);
        if index >= self.inputs.len() - 1 {
            return self.get_keyframe_value(index);
        }

        let t0 = self.inputs[index];
        let t1 = self.inputs[index + 1];
        let dt = t1 - t0;
        let t = (time - t0) / dt;

        let t2 = t * t;
        let t3 = t2 * t;

        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;

        let hermite = HermiteBasis { h00, h10, h01, h11 };
        self.interpolate_cubic_spline(index, index + 1, &hermite, dt)
    }

    fn interpolate_cubic_vec3(
        values: &[[f32; 3]],
        index0: usize,
        index1: usize,
        h: &HermiteBasis,
        dt: f32,
    ) -> [f32; 3] {
        let v0 = [
            values[index0 * 3 + 1][0],
            values[index0 * 3 + 1][1],
            values[index0 * 3 + 1][2],
        ];
        let v1 = [
            values[index1 * 3 + 1][0],
            values[index1 * 3 + 1][1],
            values[index1 * 3 + 1][2],
        ];
        let m0 = [
            values[index0 * 3 + 2][0] * dt,
            values[index0 * 3 + 2][1] * dt,
            values[index0 * 3 + 2][2] * dt,
        ];
        let m1 = [
            values[index1 * 3][0] * dt,
            values[index1 * 3][1] * dt,
            values[index1 * 3][2] * dt,
        ];

        [
            h.h00 * v0[0] + h.h10 * m0[0] + h.h01 * v1[0] + h.h11 * m1[0],
            h.h00 * v0[1] + h.h10 * m0[1] + h.h01 * v1[1] + h.h11 * m1[1],
            h.h00 * v0[2] + h.h10 * m0[2] + h.h01 * v1[2] + h.h11 * m1[2],
        ]
    }

    fn interpolate_cubic_spline(
        &self,
        index0: usize,
        index1: usize,
        h: &HermiteBasis,
        dt: f32,
    ) -> SampledValue {
        if let Some(ref translations) = self.translations {
            let result = Self::interpolate_cubic_vec3(translations, index0, index1, h, dt);
            SampledValue::Vec3(result)
        } else if let Some(ref rotations) = self.rotations {
            let q0 = katla_math::Quat::new(
                rotations[index0 * 3 + 1][0],
                rotations[index0 * 3 + 1][1],
                rotations[index0 * 3 + 1][2],
                rotations[index0 * 3 + 1][3],
            );
            let q1 = katla_math::Quat::new(
                rotations[index1 * 3 + 1][0],
                rotations[index1 * 3 + 1][1],
                rotations[index1 * 3 + 1][2],
                rotations[index1 * 3 + 1][3],
            );
            let m0 = katla_math::Quat::new(
                rotations[index0 * 3 + 2][0] * dt,
                rotations[index0 * 3 + 2][1] * dt,
                rotations[index0 * 3 + 2][2] * dt,
                rotations[index0 * 3 + 2][3] * dt,
            );
            let m1 = katla_math::Quat::new(
                rotations[index1 * 3][0] * dt,
                rotations[index1 * 3][1] * dt,
                rotations[index1 * 3][2] * dt,
                rotations[index1 * 3][3] * dt,
            );

            let (x0, y0, z0, w0) = q0.xyzw();
            let (x1, y1, z1, w1) = q1.xyzw();
            let (mx0, my0, mz0, mw0) = m0.xyzw();
            let (mx1, my1, mz1, mw1) = m1.xyzw();

            let result = [
                h.h00 * x0 + h.h10 * mx0 + h.h01 * x1 + h.h11 * mx1,
                h.h00 * y0 + h.h10 * my0 + h.h01 * y1 + h.h11 * my1,
                h.h00 * z0 + h.h10 * mz0 + h.h01 * z1 + h.h11 * mz1,
                h.h00 * w0 + h.h10 * mw0 + h.h01 * w1 + h.h11 * mw1,
            ];
            SampledValue::Quat(result)
        } else if let Some(ref scales) = self.scales {
            let result = Self::interpolate_cubic_vec3(scales, index0, index1, h, dt);
            SampledValue::Vec3(result)
        } else if let Some(ref weights) = self.weights {
            let v0 = weights[index0 * 3 + 1];
            let v1 = weights[index1 * 3 + 1];
            let m0 = weights[index0 * 3 + 2] * dt;
            let m1 = weights[index1 * 3] * dt;

            let result = h.h00 * v0 + h.h10 * m0 + h.h01 * v1 + h.h11 * m1;
            SampledValue::Float(result)
        } else {
            SampledValue::Unknown
        }
    }

    fn find_keyframe_index(&self, time: f32) -> usize {
        match self.inputs.binary_search_by(|probe| {
            probe
                .partial_cmp(&time)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
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

    pub fn find_keyframe_index_from(&self, time: f32, start_index: usize) -> usize {
        let len = self.inputs.len();
        if len == 0 {
            return 0;
        }

        let start_index = start_index.min(len - 1);

        if time >= self.inputs[start_index] {
            for i in start_index..len - 1 {
                if time < self.inputs[i + 1] {
                    return i;
                }
            }
            return len - 1;
        }

        self.find_keyframe_index(time)
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
            let q0 = katla_math::Quat::new(
                rotations[index0][0],
                rotations[index0][1],
                rotations[index0][2],
                rotations[index0][3],
            );
            let q1 = katla_math::Quat::new(
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
