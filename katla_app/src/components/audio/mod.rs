use katla_ecs::Component;

#[derive(Component, Clone)]
pub struct AudioSource {
    pub path: String,
}

impl AudioSource {
    pub fn new(path: impl Into<String>) -> Self {
        AudioSource { path: path.into() }
    }
}

#[derive(Component, Clone, Default)]
pub struct AudioListener;

#[derive(Component, Clone)]
pub struct AudioEmitter {
    pub source_path: String,
    pub volume: f32,
    pub looping: bool,
    pub playing: bool,
    pub spatial: bool,
    pub min_distance: f32,
    pub max_distance: f32,
    pub rolloff_factor: f32,
    pub distance_model: DistanceModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DistanceModel {
    #[default]
    InverseClamped,
    Linear,
    Exponential,
}

impl AudioEmitter {
    pub fn new(source_path: impl Into<String>) -> Self {
        AudioEmitter {
            source_path: source_path.into(),
            volume: 1.0,
            looping: false,
            playing: true,
            spatial: false,
            min_distance: 1.0,
            max_distance: 100.0,
            rolloff_factor: 1.0,
            distance_model: DistanceModel::default(),
        }
    }

    pub fn with_spatial(mut self) -> Self {
        self.spatial = true;
        self
    }

    pub fn with_distance_params(
        mut self,
        min_distance: f32,
        max_distance: f32,
        rolloff_factor: f32,
    ) -> Self {
        self.min_distance = min_distance;
        self.max_distance = max_distance;
        self.rolloff_factor = rolloff_factor;
        self
    }
}
