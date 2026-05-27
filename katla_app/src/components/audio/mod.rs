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

/// Defines a reverb zone in the scene as an axis-aligned box centered on the entity's position.
///
/// When the audio listener is inside the box, the zone's reverb parameters are blended
/// into the global zone reverb bus. Multiple overlapping zones have their parameters
/// averaged.
#[derive(Component, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ReverbZone {
    /// Reverb feedback/decay (0.0-0.99). Higher = longer tail.
    pub decay: f32,
    /// Wet/dry mix of the reverb effect (0.0-1.0).
    pub wet: f32,
    /// High-frequency dampening (0.0-1.0). Higher = more dampened.
    pub dampening: f32,
    /// Half-extents of the AABB box shape (distance from center to each face).
    pub half_extents: [f32; 3],
}

impl Default for ReverbZone {
    fn default() -> Self {
        ReverbZone {
            decay: 0.7,
            wet: 0.4,
            dampening: 0.3,
            half_extents: [5.0, 3.0, 5.0],
        }
    }
}

impl ReverbZone {
    pub fn new(half_extents: [f32; 3]) -> Self {
        ReverbZone {
            half_extents,
            ..Default::default()
        }
    }

    pub fn with_params(mut self, decay: f32, wet: f32, dampening: f32) -> Self {
        self.decay = decay;
        self.wet = wet;
        self.dampening = dampening;
        self
    }

    /// Check if a point is inside this zone, given the zone's world position.
    pub fn contains(&self, zone_position: &[f32; 3], point: &[f32; 3]) -> bool {
        let dx = (point[0] - zone_position[0]).abs();
        let dy = (point[1] - zone_position[1]).abs();
        let dz = (point[2] - zone_position[2]).abs();
        dx <= self.half_extents[0] && dy <= self.half_extents[1] && dz <= self.half_extents[2]
    }
}
