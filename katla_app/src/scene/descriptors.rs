use serde::{Deserialize, Serialize};

use super::{AssetRef, SceneEntityId, entity_source::EntitySource};
use std::collections::BTreeMap;

/// Transform data for serialization (plain arrays, no SIMD types).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct DrawableDescriptor {
    /// Omission retains the source's original images; choices contain no GPU handles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub textures: Option<crate::material_images::TextureAssignments>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<crate::rendering::MaterialSurface>,
    /// Omission preserves the mesh source’s imported sampling settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampling: Option<crate::rendering::MaterialSampling>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 4]>,
    pub metallic: f32,
    pub roughness: f32,
    pub ao: f32,
}

/// Point light data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointLightDescriptor {
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
}

/// Particle emitter data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ParticleEmitterDescriptor {
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
    pub color_end: [f32; 4],
    pub scale_end: f32,
    #[serde(default)]
    pub kill_on_destroy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timed_emission: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub burst_queue: Vec<u32>,
}

/// Animation state for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationDescriptor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_clip: Option<String>,
    pub playing: bool,
    pub loop_animation: bool,
    pub speed: f32,
    pub time: f32,
    #[serde(default)]
    pub duration: f32,
    #[serde(default)]
    pub blending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    /// Whether the current clip has already emitted completion.
    #[serde(default)]
    pub completed: bool,
    /// Whether the fade target has already emitted completion.
    #[serde(default)]
    pub target_completed: bool,
    /// Independent looping policy committed when the fade completes.
    #[serde(default)]
    pub target_loop_animation: bool,
    /// Loop count carried into the target's playback state.
    #[serde(default)]
    pub target_loop_count: u32,
    #[serde(default)]
    pub loop_count: u32,
}

impl From<&crate::animation::AnimationPlayer> for AnimationDescriptor {
    fn from(player: &crate::animation::AnimationPlayer) -> Self {
        Self {
            current_clip: player.current_clip.clone(),
            playing: player.playing,
            loop_animation: player.loop_animation,
            speed: player.speed,
            time: player.time,
            duration: player.duration,
            blending: player.blending,
            target_clip: player.target_clip.clone(),
            blend_weight: player.blend_weight,
            blend_time: player.blend_time,
            blend_duration: player.blend_duration,
            target_time: player.target_time,
            target_duration: player.target_duration,
            loop_count: player.loop_count,
            completed: player.completed,
            target_completed: player.target_completed,
            target_loop_animation: player.target_loop_animation,
            target_loop_count: player.target_loop_count,
        }
    }
}

impl AnimationDescriptor {
    pub(crate) fn restore(&self, player: &mut crate::animation::AnimationPlayer) {
        player.stop();
        player.current_clip = self.current_clip.clone();
        player.playing = self.playing;
        player.loop_animation = self.loop_animation;
        player.speed = self.speed;
        player.time = self.time;
        player.duration = self.duration;
        player.loop_count = self.loop_count;
        player.completed = self.completed;
        if self.blending && self.target_clip.is_some() {
            player.blending = self.blending;
            player.target_clip = self.target_clip.clone();
            player.blend_weight = self.blend_weight;
            player.blend_time = self.blend_time;
            player.blend_duration = self.blend_duration;
            player.target_time = self.target_time;
            player.target_duration = self.target_duration;
            player.target_completed = self.target_completed;
            player.target_loop_animation = self.target_loop_animation;
            player.target_loop_count = self.target_loop_count;
        }
    }
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
            completed: false,
            target_completed: false,
            target_loop_animation: false,
            target_loop_count: 0,
            loop_count: 0,
        }
    }
}

/// Velocity data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VelocityDescriptor {
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
}

/// Perspective camera data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerspectiveDescriptor {
    /// Vertical field of view in degrees, matching PerspectiveComponent.
    pub fov: f32,
    pub near: f32,
    pub aspect_ratio: f32,
}

/// Directional light data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectionalLightDescriptor {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
}

/// Script attachment data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptDescriptor {
    pub path: AssetRef,
}

/// Audio emitter data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioEmitterDescriptor {
    pub path: AssetRef,
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

/// Authored physics settings; native Rapier handles are recreated on load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigidBodyDescriptor {
    pub kind: katla_physics::BodyType,
    #[serde(default = "default_one")]
    pub gravity_scale: f32,
    #[serde(default)]
    pub ccd_enabled: bool,
    #[serde(default)]
    pub linear_velocity: [f32; 3],
}

fn default_one() -> f32 {
    1.0
}

impl RigidBodyDescriptor {
    /// Construct body settings without a live physics handle.
    pub fn new(kind: katla_physics::BodyType) -> Self {
        Self {
            kind,
            gravity_scale: 1.0,
            ccd_enabled: false,
            linear_velocity: [0.0; 3],
        }
    }
}

/// Joint endpoints use persistent scene keys rather than native entity handles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointDescriptor {
    pub kind: katla_physics::JointType,
    pub a: SceneEntityId,
    pub b: SceneEntityId,
    pub anchor_a: [f32; 3],
    pub anchor_b: [f32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<[f32; 2]>,
}

/// Versioned application component payload, retained even without its plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomComponentDescriptor {
    pub version: u32,
    pub data: String,
}

/// Collider shape data for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColliderShapeDescriptor {
    Sphere(f32),
    Box([f32; 3]),
    Capsule {
        half_height: f32,
        radius: f32,
    },
    Trimesh,
    ConvexHull,
    Heightfield {
        rows: u32,
        cols: u32,
        heights: Vec<f32>,
    },
}

/// Physics material properties for serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsMaterialDescriptor {
    pub friction: f32,
    pub restitution: f32,
    pub density: f32,
}

/// Trigger volume marker for serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerVolumeDescriptor;

/// Collision filter layers for serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollisionFilterDescriptor {
    pub layers: u32,
    pub mask: u32,
}

/// Descriptor for a single entity in a scene file.
///
/// Built-in fields are strict. Application data lives in versioned `components`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityDescriptor {
    pub id: SceneEntityId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<SceneEntityId>,
    #[serde(default = "TransformDescriptor::default_transform")]
    pub transform: TransformDescriptor,
    #[serde(default)]
    pub source: EntitySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawable: Option<DrawableDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point_light: Option<PointLightDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particle_emitter: Option<ParticleEmitterDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<AnimationDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub velocity: Option<VelocityDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<ScriptDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perspective: Option<PerspectiveDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directional_light: Option<DirectionalLightDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_emitter: Option<AudioEmitterDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rigid_body: Option<RigidBodyDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverb_zone: Option<crate::components::ReverbZone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collider_shape: Option<ColliderShapeDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physics_material: Option<PhysicsMaterialDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_volume: Option<TriggerVolumeDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_filter: Option<CollisionFilterDescriptor>,
    /// Stable document keys resolved after all scene entities are staged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trigger_rules: Vec<katla_agent::events::TriggerRule<SceneEntityId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint: Option<JointDescriptor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: BTreeMap<String, CustomComponentDescriptor>,
}

/// Top-level scene file structure.
///
/// Header versions are checked before parsing the strict versioned schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    /// Scene format version. Enables migration when the format changes.
    /// The loader uses this to apply any necessary transformations.
    #[serde(default)]
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    /// Never reuse a key after an entity has been removed.
    pub next_entity_id: u64,
    pub entities: Vec<EntityDescriptor>,
}

impl Scene {
    /// Create a new empty scene.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: super::SCENE_VERSION,
            name: name.into(),
            author: None,
            created_at: None,
            modified_at: None,
            engine_version: None,
            next_entity_id: 1,
            entities: Vec::new(),
        }
    }
}

impl EntityDescriptor {
    /// Construct a transform-only entity or a mesh/source instance with no components.
    pub fn new(id: SceneEntityId, source: EntitySource) -> Self {
        Self {
            id,
            name: None,
            parent: None,
            transform: TransformDescriptor::default_transform(),
            source,
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
            script: None,
            perspective: None,
            directional_light: None,
            audio_emitter: None,
            rigid_body: None,
            reverb_zone: None,
            collider_shape: None,
            physics_material: None,
            trigger_volume: None,
            collision_filter: None,
            trigger_rules: vec![],
            joint: None,
            components: BTreeMap::new(),
        }
    }
}

impl ParticleEmitterDescriptor {
    pub(crate) fn from_component(emitter: &crate::components::ParticleEmitterComponent) -> Self {
        let config = emitter.config;
        Self {
            emit_rate: config.emit_rate,
            base_lifetime: config.base_lifetime,
            lifetime_variation: config.lifetime_variation,
            velocity_direction: config.velocity_direction,
            velocity_magnitude: config.velocity_magnitude,
            velocity_cone_angle: config.velocity_cone_angle,
            base_scale: config.base_scale,
            scale_variation: config.scale_variation,
            color: config.color,
            color_variation: config.color_variation,
            color_end: config.color_end.0,
            scale_end: config.scale_end,
            gravity: config.gravity,
            turbulence_strength: config.turbulence_strength,
            turbulence_frequency: config.turbulence_frequency,
            shape: config.shape,
            shape_params: config.shape_params,
            active: emitter.active,
            kill_on_destroy: emitter.kill_on_destroy,
            timed_emission: emitter.timed_emission,
            burst_queue: emitter.burst_queue.clone(),
        }
    }

    pub(crate) fn to_component(
        &self,
        position: [f32; 3],
    ) -> crate::components::ParticleEmitterComponent {
        let config = katla_gfx::particles::EmitterConfig {
            position,
            emit_rate: self.emit_rate,
            base_lifetime: self.base_lifetime,
            lifetime_variation: self.lifetime_variation,
            velocity_direction: self.velocity_direction,
            velocity_magnitude: self.velocity_magnitude,
            velocity_cone_angle: self.velocity_cone_angle,
            base_scale: self.base_scale,
            scale_variation: self.scale_variation,
            color: self.color,
            color_variation: self.color_variation,
            color_end: katla_gfx::particles::Align16Vec4(self.color_end),
            scale_end: self.scale_end,
            gravity: self.gravity,
            turbulence_strength: self.turbulence_strength,
            turbulence_frequency: self.turbulence_frequency,
            shape: self.shape,
            shape_params: self.shape_params,
            ..Default::default()
        };
        let mut emitter = crate::components::ParticleEmitterComponent::with_config(config);
        emitter.active = self.active;
        emitter.kill_on_destroy = self.kill_on_destroy;
        emitter.timed_emission = self.timed_emission;
        emitter.burst_queue = self.burst_queue.clone();
        emitter
    }
}

impl Default for ParticleEmitterDescriptor {
    fn default() -> Self {
        Self::from_component(&crate::components::ParticleEmitterComponent::default())
    }
}

impl Default for TransformDescriptor {
    fn default() -> Self {
        Self::default_transform()
    }
}
