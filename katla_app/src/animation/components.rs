use katla_ecs::Component;
use katla_math::Quat;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationEvent {
    Completed { clip_name: String },
    Looped { clip_name: String, loop_count: u32 },
}

#[derive(Component, Debug, Clone)]
pub struct AnimationPlayer {
    pub current_clip: Option<String>,
    pub duration: f32,
    pub time: f32,
    pub playing: bool,
    pub loop_animation: bool,
    pub speed: f32,
    pub blend_weight: f32,
    pub target_clip: Option<String>,
    pub target_duration: f32,
    pub target_time: f32,
    pub blend_duration: f32,
    pub blend_time: f32,
    pub blending: bool,
    pub events: Vec<AnimationEvent>,
    pub loop_count: u32,
}

impl AnimationPlayer {
    pub fn new(clip_name: impl Into<String>) -> Self {
        Self {
            current_clip: Some(clip_name.into()),
            duration: 0.0,
            time: 0.0,
            playing: true,
            loop_animation: false,
            speed: 1.0,
            blend_weight: 1.0,
            target_clip: None,
            target_duration: 0.0,
            target_time: 0.0,
            blend_duration: 0.0,
            blend_time: 0.0,
            blending: false,
            events: Vec::new(),
            loop_count: 0,
        }
    }

    pub fn with_duration(mut self, duration: f32) -> Self {
        self.duration = duration;
        self
    }

    pub fn stopped() -> Self {
        Self {
            current_clip: None,
            duration: 0.0,
            time: 0.0,
            playing: false,
            loop_animation: false,
            speed: 1.0,
            blend_weight: 1.0,
            target_clip: None,
            target_duration: 0.0,
            target_time: 0.0,
            blend_duration: 0.0,
            blend_time: 0.0,
            blending: false,
            events: Vec::new(),
            loop_count: 0,
        }
    }

    pub fn with_clip(mut self, clip: impl Into<String>) -> Self {
        self.current_clip = Some(clip.into());
        self
    }

    pub fn looping(mut self) -> Self {
        self.loop_animation = true;
        self
    }

    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    pub fn set_clip(&mut self, clip: impl Into<String>, duration: f32) {
        self.current_clip = Some(clip.into());
        self.duration = duration;
        self.time = 0.0;
        self.loop_count = 0;
        self.blending = false;
        self.target_clip = None;
        self.blend_weight = 1.0;
    }

    pub fn crossfade_to(&mut self, clip: impl Into<String>, duration: f32, blend_duration: f32) {
        self.target_clip = Some(clip.into());
        self.target_duration = duration;
        self.target_time = 0.0;
        self.blend_duration = blend_duration;
        self.blend_time = 0.0;
        self.blending = true;
        self.blend_weight = 1.0;
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.time = 0.0;
        self.loop_count = 0;
        self.blending = false;
        self.target_clip = None;
        self.blend_weight = 1.0;
    }

    pub fn seek(&mut self, time: f32) {
        self.time = time.clamp(0.0, self.duration.max(0.0));
    }

    pub fn get_duration(&self) -> f32 {
        self.duration
    }

    pub fn take_events(&mut self) -> Vec<AnimationEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn is_complete(&self) -> bool {
        !self.playing && self.time >= self.duration && !self.loop_animation
    }
}

#[derive(Component, Debug, Clone)]
pub struct AnimatedModel {
    pub animations: HashMap<String, super::clips::AnimationClip>,
    pub sequences: HashMap<String, AnimationSequence>,
}

#[derive(Debug, Clone)]
pub struct AnimationSequence {
    pub clips: Vec<SequenceClip>,
    pub duration: f32,
    pub loop_sequence: bool,
}

#[derive(Debug, Clone)]
pub struct SequenceClip {
    pub name: String,
    pub weight: f32,
    pub time_offset: f32,
}

#[derive(Component, Debug, Clone)]
pub struct MorphTargetWeights {
    pub weights: Vec<f32>,
}

impl MorphTargetWeights {
    pub fn new(count: usize) -> Self {
        Self {
            weights: vec![0.0; count],
        }
    }

    pub fn set_weight(&mut self, index: usize, weight: f32) {
        if index < self.weights.len() {
            self.weights[index] = weight.clamp(0.0, 1.0);
        }
    }

    pub fn get_weight(&self, index: usize) -> f32 {
        self.weights.get(index).copied().unwrap_or(0.0)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct JointTransform {
    pub translation: [f32; 3],
    pub rotation: Quat,
    pub scale: [f32; 3],
}

impl JointTransform {
    pub fn identity() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: Quat::new(),
            scale: [1.0, 1.0, 1.0],
        }
    }

    pub fn from_translation(translation: [f32; 3]) -> Self {
        Self {
            translation,
            rotation: Quat::new(),
            scale: [1.0, 1.0, 1.0],
        }
    }

    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self::lerp_arrays(self, other, t)
    }

    pub fn lerp_arrays(a: &Self, b: &Self, t: f32) -> Self {
        let qa = katla_math::Quat::new_from_xyzw(
            a.rotation[0],
            a.rotation[1],
            a.rotation[2],
            a.rotation[3],
        );
        let qb = katla_math::Quat::new_from_xyzw(
            b.rotation[0],
            b.rotation[1],
            b.rotation[2],
            b.rotation[3],
        );
        let q_result = katla_math::Quat::slerp(qa, qb, t);

        let one_minus_t = 1.0 - t;
        Self {
            translation: [
                a.translation[0] * one_minus_t + b.translation[0] * t,
                a.translation[1] * one_minus_t + b.translation[1] * t,
                a.translation[2] * one_minus_t + b.translation[2] * t,
            ],
            rotation: q_result,
            scale: [
                a.scale[0] * one_minus_t + b.scale[0] * t,
                a.scale[1] * one_minus_t + b.scale[1] * t,
                a.scale[2] * one_minus_t + b.scale[2] * t,
            ],
        }
    }

    pub fn blend(a: &Self, b: &Self, weight_a: f32, weight_b: f32) -> Self {
        let total = weight_a + weight_b;
        if total == 0.0 {
            return Self::identity();
        }
        let t = weight_b / total;
        Self::lerp_arrays(a, b, t)
    }
}

impl Default for JointTransform {
    fn default() -> Self {
        Self::identity()
    }
}
