#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interpolation {
    #[default]
    Linear,
    Step,
    CubicSpline,
}

impl Interpolation {
    pub fn from_gltf(value: &str) -> Self {
        match value {
            "LINEAR" => Interpolation::Linear,
            "STEP" => Interpolation::Step,
            "CUBICSPLINE" => Interpolation::CubicSpline,
            _ => {
                log::warn!(
                    "Unknown interpolation type: {}, defaulting to LINEAR",
                    value
                );
                Interpolation::Linear
            }
        }
    }

    pub fn to_gltf(&self) -> &'static str {
        match self {
            Interpolation::Linear => "LINEAR",
            Interpolation::Step => "STEP",
            Interpolation::CubicSpline => "CUBICSPLINE",
        }
    }
}

pub struct CachedSampler<'a> {
    sampler: &'a super::clips::AnimationSampler,
    last_index: usize,
}

impl<'a> CachedSampler<'a> {
    pub fn new(sampler: &'a super::clips::AnimationSampler) -> Self {
        Self {
            sampler,
            last_index: 0,
        }
    }

    pub fn sample(&mut self, time: f32) -> super::clips::SampledValue {
        self.last_index = self.sampler.find_keyframe_index_from(time, self.last_index);
        self.sampler.sample(time)
    }

    pub fn last_index(&self) -> usize {
        self.last_index
    }

    pub fn reset(&mut self) {
        self.last_index = 0;
    }
}
