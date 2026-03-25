use serde::{Deserialize, Serialize};

use super::entity_source::EntitySource;

/// Transform data for serialization (plain arrays, no SIMD types).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformDescriptor {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

/// Drawable material properties (color + PBR params, no GPU handles).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawableDescriptor {
    pub color: Option<[f32; 4]>,
    pub metallic: f32,
    pub roughness: f32,
    pub ao: f32,
}

/// Point light data for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointLightDescriptor {
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
}

/// Particle emitter data for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub shape: u32,
    pub shape_params: [f32; 4],
    pub active: bool,
}

/// Animation state for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationDescriptor {
    pub current_clip: Option<String>,
    pub playing: bool,
    pub loop_animation: bool,
    pub speed: f32,
    pub time: f32,
}

/// Velocity data for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelocityDescriptor {
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
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
    pub entities: Vec<EntityDescriptor>,
}

impl Scene {
    /// Create a new empty scene.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: 1,
            name: name.into(),
            entities: Vec::new(),
        }
    }
}
