use serde::{Deserialize, Serialize};

use super::entity_source::EntitySource;

/// Transform data for serialization (plain arrays, no SIMD types).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformDescriptor {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl TransformDescriptor {
    pub fn default_transform() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

/// Drawable material properties (color + PBR params, no GPU handles).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawableDescriptor {
    pub color: Option<[f32; 4]>,
    pub metallic: f32,
    pub roughness: f32,
    pub ao: f32,
}

/// Point light data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointLightDescriptor {
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
}

/// Particle emitter data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParticleEmitterDescriptor {
    pub position: [f32; 3],
    pub emit_rate: f32,
    pub base_lifetime: f32,
    pub lifetime_variation: f32,
    pub velocity_direction: [f32; 3],
    pub velocity_magnitude: f32,
    pub velocity_cone_angle: f32,
    pub base_scale: f32,
    pub scale_variation: f32,
    pub color: [f32; 4],
    pub color_variation: f32,
    pub gravity: f32,
    pub turbulence_strength: f32,
    pub turbulence_frequency: f32,
    pub shape: katla_gfx::particles::EmitterShape,
    pub shape_params: [f32; 4],
    pub active: bool,
}

/// Animation state for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationDescriptor {
    pub current_clip: Option<String>,
    pub playing: bool,
    pub loop_animation: bool,
    pub speed: f32,
    pub time: f32,
    #[serde(default)]
    pub duration: f32,
    #[serde(default)]
    pub blending: bool,
    #[serde(default)]
    pub target_clip: Option<String>,
    #[serde(default)]
    pub blend_weight: f32,
    #[serde(default)]
    pub blend_time: f32,
    #[serde(default)]
    pub blend_duration: f32,
    #[serde(default)]
    pub target_time: f32,
    #[serde(default)]
    pub target_duration: f32,
    #[serde(default)]
    pub loop_count: u32,
}

impl Default for AnimationDescriptor {
    fn default() -> Self {
        Self {
            current_clip: None,
            playing: false,
            loop_animation: false,
            speed: 1.0,
            time: 0.0,
            duration: 0.0,
            blending: false,
            target_clip: None,
            blend_weight: 1.0,
            blend_time: 0.0,
            blend_duration: 0.0,
            target_time: 0.0,
            target_duration: 0.0,
            loop_count: 0,
        }
    }
}

/// Velocity data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VelocityDescriptor {
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
}

/// Perspective camera data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerspectiveDescriptor {
    pub fov: f32,
    pub near: f32,
    pub aspect_ratio: f32,
}

/// Directional light data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectionalLightDescriptor {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
}

/// Script attachment data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptDescriptor {
    pub script_path: String,
}

/// Audio emitter data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioEmitterDescriptor {
    pub source_path: String,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub looping: bool,
    #[serde(default = "default_playing")]
    pub playing: bool,
    #[serde(default)]
    pub spatial: bool,
    #[serde(default = "default_min_distance")]
    pub min_distance: f32,
    #[serde(default = "default_max_distance")]
    pub max_distance: f32,
    #[serde(default = "default_rolloff")]
    pub rolloff_factor: f32,
    #[serde(default)]
    pub distance_model: crate::components::audio::DistanceModel,
}

fn default_volume() -> f32 {
    1.0
}

fn default_playing() -> bool {
    true
}

fn default_min_distance() -> f32 {
    1.0
}

fn default_max_distance() -> f32 {
    100.0
}

fn default_rolloff() -> f32 {
    1.0
}

/// Descriptor for a single entity in a scene file.
///
/// Uses `#[serde(deny_unknown_fields)] = false` (the default) so that
/// scene files from newer engine versions with additional fields can be
/// loaded by older versions without error. Unknown fields are silently ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDescriptor {
    pub name: Option<String>,
    pub parent: Option<String>,
    pub transform: TransformDescriptor,
    pub source: EntitySource,
    pub drawable: Option<DrawableDescriptor>,
    pub point_light: Option<PointLightDescriptor>,
    pub particle_emitter: Option<ParticleEmitterDescriptor>,
    pub animation: Option<AnimationDescriptor>,
    pub velocity: Option<VelocityDescriptor>,
    #[serde(default)]
    pub script: Option<ScriptDescriptor>,
    #[serde(default)]
    pub perspective: Option<PerspectiveDescriptor>,
    #[serde(default)]
    pub directional_light: Option<DirectionalLightDescriptor>,
    #[serde(default)]
    pub audio_emitter: Option<AudioEmitterDescriptor>,
}

/// Top-level scene file structure.
///
/// Unknown top-level keys in the RON file are silently ignored (RON default),
/// providing forward compatibility when the engine adds new scene-level metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    /// Scene format version. Enables migration when the format changes.
    /// The loader uses this to apply any necessary transformations.
    #[serde(default)]
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub modified_at: Option<String>,
    #[serde(default)]
    pub engine_version: Option<String>,
    pub entities: Vec<EntityDescriptor>,
}

impl Scene {
    /// Create a new empty scene.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: 1,
            name: name.into(),
            author: None,
            created_at: None,
            modified_at: None,
            engine_version: None,
            entities: Vec::new(),
        }
    }
}
