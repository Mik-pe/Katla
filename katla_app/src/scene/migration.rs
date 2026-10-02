//! Explicit v0/v1/v2 readers and migration into the single current runtime schema.

use super::{AssetRef, EntitySource, SCENE_VERSION, SceneEntityId, SceneError, descriptors::*};
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf};

#[derive(Deserialize)]
#[serde(rename = "Scene")]
struct Header {
    #[serde(default)]
    version: u32,
}

/// Decode the header first so future versions fail before parsing unknown variants.
pub fn parse_scene(content: &str) -> Result<Scene, SceneError> {
    if content.len() > 64 * 1024 * 1024 {
        return Err(SceneError::Limit {
            field: "file size in bytes",
            maximum: 64 * 1024 * 1024,
        });
    }
    let header: Header = ron::from_str(content).map_err(SceneError::Parse)?;
    match header.version {
        0..=2 => migrate_legacy(ron::from_str(content).map_err(SceneError::Parse)?),
        SCENE_VERSION => ron::from_str(content).map_err(SceneError::Parse),
        version => Err(SceneError::UnsupportedVersion {
            found: version,
            supported: SCENE_VERSION,
        }),
    }
}

#[derive(Deserialize)]
#[serde(rename = "Scene", deny_unknown_fields)]
struct LegacyScene {
    #[serde(default, rename = "version")]
    _version: u32,
    name: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    modified_at: Option<String>,
    #[serde(default)]
    engine_version: Option<String>,
    entities: Vec<LegacyEntity>,
}
#[derive(Deserialize)]
#[serde(rename = "EntityDescriptor", deny_unknown_fields)]
struct LegacyEntity {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    pub transform: TransformDescriptor,
    pub source: LegacySource,
    #[serde(default)]
    pub drawable: Option<DrawableDescriptor>,
    #[serde(default)]
    pub point_light: Option<PointLightDescriptor>,
    #[serde(default)]
    pub particle_emitter: Option<LegacyParticles>,
    #[serde(default)]
    pub animation: Option<AnimationDescriptor>,
    #[serde(default)]
    pub velocity: Option<VelocityDescriptor>,
    #[serde(default)]
    pub script: Option<LegacyScript>,
    #[serde(default)]
    pub perspective: Option<PerspectiveDescriptor>,
    #[serde(default)]
    pub directional_light: Option<DirectionalLightDescriptor>,
    #[serde(default)]
    pub audio_emitter: Option<LegacyAudio>,
    #[serde(default)]
    pub rigid_body: Option<LegacyBody>,
    #[serde(default)]
    pub rigid_body_properties: Option<LegacyBodyProperties>,
    #[serde(default)]
    pub reverb_zone: Option<crate::components::ReverbZone>,
    #[serde(default)]
    pub collider_shape: Option<LegacyCollider>,
    #[serde(default)]
    pub physics_material: Option<PhysicsMaterialDescriptor>,
    #[serde(default)]
    pub trigger_volume: Option<TriggerVolumeDescriptor>,
    #[serde(default)]
    pub collision_filter: Option<CollisionFilterDescriptor>,
    #[serde(default)]
    pub trigger_rules: Vec<katla_agent::events::TriggerRule<String>>,
}

#[derive(Deserialize)]
#[serde(rename = "ParticleEmitterDescriptor", deny_unknown_fields)]
struct LegacyParticles {
    #[serde(rename = "position")]
    pub _position: [f32; 3],
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

#[derive(Deserialize)]
enum LegacyBody {
    Static,
    Dynamic,
    Kinematic,
}

/// Rigid body settings and velocity, without native physics handles.
#[derive(Deserialize)]
#[serde(rename = "RigidBodyPropertiesDescriptor", deny_unknown_fields)]
struct LegacyBodyProperties {
    pub gravity_scale: f32,
    pub ccd_enabled: bool,
    pub linear_velocity: [f32; 3],
}

/// Collider shape data for serialization.
#[derive(Deserialize)]
enum LegacyCollider {
    Sphere(f32),
    Box([f32; 3]),
    Capsule {
        half_height: f32,
        radius: f32,
    },
    Trimesh {
        #[serde(rename = "mesh_handle_index")]
        _index: u32,
        #[serde(rename = "mesh_handle_generation")]
        _generation: u32,
    },
    ConvexHull {
        #[serde(rename = "mesh_handle_index")]
        _index: u32,
        #[serde(rename = "mesh_handle_generation")]
        _generation: u32,
    },
    Heightfield {
        rows: u32,
        cols: u32,
        heights: Vec<f32>,
    },
}

#[derive(Deserialize)]
#[serde(rename = "ScriptDescriptor", deny_unknown_fields)]
struct LegacyScript {
    pub script_path: String,
}

#[derive(Deserialize)]
#[serde(rename = "AudioEmitterDescriptor", deny_unknown_fields)]
struct LegacyAudio {
    pub source_path: String,
    #[serde(default = "default_one")]
    pub volume: f32,
    #[serde(default)]
    pub looping: bool,
    #[serde(default = "default_playing")]
    pub playing: bool,
    #[serde(default)]
    pub spatial: bool,
    #[serde(default = "default_one")]
    pub min_distance: f32,
    #[serde(default = "default_max_distance")]
    pub max_distance: f32,
    #[serde(default = "default_one")]
    pub rolloff_factor: f32,
    #[serde(default)]
    pub distance_model: crate::components::audio::DistanceModel,
}

fn default_one() -> f32 {
    1.0
}
fn default_playing() -> bool {
    true
}
fn default_max_distance() -> f32 {
    100.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
enum LegacySource {
    Cube {
        size: [f32; 3],
    },
    Sphere {
        radius: f32,
        segments: u32,
        rings: u32,
    },
    Plane {
        width: f32,
        height: f32,
    },
    Cylinder {
        height: f32,
        radius: f32,
        segments: u32,
    },
    Torus {
        radius: f32,
        tube_radius: f32,
        segments: u32,
        tube_segments: u32,
    },
    GltfModel {
        path: String,
    },
    StlModel {
        path: String,
    },
    ParticleEmitter,
    Light,
    Trigger,
}

fn migrate_legacy(old: LegacyScene) -> Result<Scene, SceneError> {
    let mut names: HashMap<&str, Vec<SceneEntityId>> = HashMap::new();
    for (index, entity) in old.entities.iter().enumerate() {
        if let Some(name) = &entity.name {
            names
                .entry(name)
                .or_default()
                .push(SceneEntityId(index as u64 + 1));
        }
    }
    let mut parents = Vec::with_capacity(old.entities.len());
    let mut trigger_rules = Vec::with_capacity(old.entities.len());
    for entity in &old.entities {
        trigger_rules.push(entity.trigger_rules.iter().map(|rule| {
            rule.map_entities(|name| match names.get(name.as_str()).map(Vec::as_slice) {
                Some([id]) => Ok(*id),
                Some(_) => Err(SceneError::Migration(format!("Trigger target '{name}' is ambiguous; rename duplicate targets in the old file"))),
                None => Err(SceneError::Migration(format!("Trigger target '{name}' does not exist"))),
            })
        }).collect::<Result<Vec<_>, SceneError>>()?);
        parents.push(match entity.parent.as_deref() {
            None => None,
            Some(name) => match names.get(name).map(Vec::as_slice) {
                Some([id]) => Some(*id),
                Some(_) => {
                    return Err(SceneError::Migration(format!(
                        "Parent '{name}' is ambiguous; rename duplicate parents in the old file"
                    )));
                }
                None => {
                    return Err(SceneError::Migration(format!(
                        "Parent '{name}' does not exist"
                    )));
                }
            },
        });
    }
    let mut scene = Scene::new(old.name);
    scene.author = old.author;
    scene.created_at = old.created_at;
    scene.modified_at = old.modified_at;
    scene.engine_version = old.engine_version;
    scene.next_entity_id = old.entities.len() as u64 + 1;
    for (index, ((entity, parent), trigger_rules)) in old
        .entities
        .into_iter()
        .zip(parents)
        .zip(trigger_rules)
        .enumerate()
    {
        let id = SceneEntityId(index as u64 + 1);
        let source = match entity.source {
            LegacySource::Cube { size } => EntitySource::Cube { size },
            LegacySource::Sphere {
                radius,
                segments,
                rings,
            } => EntitySource::Sphere {
                radius,
                segments,
                rings,
            },
            LegacySource::Plane { width, height } => EntitySource::Plane { width, height },
            LegacySource::Cylinder {
                radius,
                height,
                segments,
            } => EntitySource::Cylinder {
                radius,
                height,
                segments,
            },
            LegacySource::Torus {
                radius,
                tube_radius,
                segments,
                tube_segments,
            } => EntitySource::Torus {
                radius,
                tube_radius,
                segments,
                tube_segments,
            },
            LegacySource::GltfModel { path } => EntitySource::GltfModel {
                path: legacy_asset(path),
            },
            LegacySource::StlModel { path } => EntitySource::StlModel {
                path: legacy_asset(path),
            },
            LegacySource::ParticleEmitter => EntitySource::ParticleEmitter,
            LegacySource::Light => EntitySource::Light,
            LegacySource::Trigger => EntitySource::Trigger,
        };
        let rigid_body = entity.rigid_body.map(|kind| {
            let kind = match kind {
                LegacyBody::Static => katla_physics::BodyType::Static,
                LegacyBody::Dynamic => katla_physics::BodyType::Dynamic,
                LegacyBody::Kinematic => katla_physics::BodyType::Kinematic,
            };
            let mut body = RigidBodyDescriptor::new(kind);
            if let Some(properties) = entity.rigid_body_properties {
                body.gravity_scale = properties.gravity_scale;
                body.ccd_enabled = properties.ccd_enabled;
                body.linear_velocity = properties.linear_velocity;
            }
            body
        });
        let collider_shape = entity.collider_shape.map(|shape| match shape {
            LegacyCollider::Sphere(radius) => ColliderShapeDescriptor::Sphere(radius),
            LegacyCollider::Box(extents) => ColliderShapeDescriptor::Box(extents),
            LegacyCollider::Capsule {
                half_height,
                radius,
            } => ColliderShapeDescriptor::Capsule {
                half_height,
                radius,
            },
            LegacyCollider::Trimesh { .. } => ColliderShapeDescriptor::Trimesh,
            LegacyCollider::ConvexHull { .. } => ColliderShapeDescriptor::ConvexHull,
            LegacyCollider::Heightfield {
                rows,
                cols,
                heights,
            } => ColliderShapeDescriptor::Heightfield {
                rows,
                cols,
                heights,
            },
        });
        let particle_emitter = entity.particle_emitter.map(|p| ParticleEmitterDescriptor {
            emit_rate: p.emit_rate,
            base_lifetime: p.base_lifetime,
            lifetime_variation: p.lifetime_variation,
            velocity_direction: p.velocity_direction,
            velocity_magnitude: p.velocity_magnitude,
            velocity_cone_angle: p.velocity_cone_angle,
            base_scale: p.base_scale,
            scale_variation: p.scale_variation,
            color: p.color,
            color_variation: p.color_variation,
            gravity: p.gravity,
            turbulence_strength: p.turbulence_strength,
            turbulence_frequency: p.turbulence_frequency,
            shape: p.shape,
            shape_params: p.shape_params,
            active: p.active,
            ..Default::default()
        });
        let particle_emitter = particle_emitter.or_else(|| {
            matches!(source, EntitySource::ParticleEmitter).then(ParticleEmitterDescriptor::default)
        });
        let point_light = entity.point_light.or_else(|| {
            matches!(source, EntitySource::Light).then(|| PointLightDescriptor {
                color: [1.0; 3],
                intensity: 1.0,
                range: 10.0,
            })
        });
        let drawable = if matches!(source, EntitySource::Light | EntitySource::ParticleEmitter) {
            None
        } else {
            entity.drawable
        };
        let script = entity.script.map(|value| {
            let path = if !value.script_path.contains('/') && !value.script_path.contains('\\') {
                AssetRef::Resource(format!(
                    "scripts/{}",
                    PathBuf::from(value.script_path)
                        .with_extension("luau")
                        .display()
                ))
            } else {
                legacy_asset(value.script_path)
            };
            ScriptDescriptor { path }
        });
        let audio_emitter = entity.audio_emitter.map(|a| AudioEmitterDescriptor {
            path: legacy_asset(a.source_path),
            volume: a.volume,
            looping: a.looping,
            playing: a.playing,
            spatial: a.spatial,
            min_distance: a.min_distance,
            max_distance: a.max_distance,
            rolloff_factor: a.rolloff_factor,
            distance_model: a.distance_model,
        });
        scene.entities.push(EntityDescriptor {
            id,
            name: entity.name,
            parent,
            transform: entity.transform,
            source,
            drawable,
            point_light,
            particle_emitter,
            animation: entity.animation,
            velocity: entity.velocity,
            script,
            perspective: entity.perspective,
            directional_light: entity.directional_light,
            audio_emitter,
            rigid_body,
            reverb_zone: entity.reverb_zone,
            collider_shape,
            physics_material: entity.physics_material,
            trigger_volume: entity.trigger_volume,
            collision_filter: entity.collision_filter,
            trigger_rules,
            joint: None,
            components: Default::default(),
        });
    }
    Ok(scene)
}

fn legacy_asset(path: String) -> AssetRef {
    let path = path.replace('\\', "/");
    if let Some(relative) = path.strip_prefix("resources/") {
        AssetRef::Resource(relative.into())
    } else if PathBuf::from(&path).is_absolute() {
        AssetRef::File(path.into())
    } else {
        AssetRef::Scene(path)
    }
}
