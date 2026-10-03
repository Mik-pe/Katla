//! Validate the complete document before allocating entities or GPU resources.

use super::{EntitySource, Scene, SceneEntityId, SceneError, SceneIssue, descriptors::*};
use std::collections::{HashMap, HashSet};

const MAX_ENTITIES: usize = 100_000;
const MAX_GEOMETRY_POINTS: u64 = 1_000_000;

struct Fields<'a> {
    id: SceneEntityId,
    issues: &'a mut Vec<SceneIssue>,
}
impl Fields<'_> {
    fn require(&mut self, field: &str, valid: bool, message: &str) {
        if !valid {
            self.issues.push(SceneIssue {
                entity: Some(self.id),
                field: field.into(),
                message: message.into(),
            });
        }
    }
    fn finite<const N: usize>(&mut self, field: &str, value: &[f32; N]) {
        self.require(
            field,
            value.iter().all(|v| v.is_finite()),
            "must contain only finite numbers",
        );
    }
    fn positive(&mut self, field: &str, value: f32) {
        self.require(
            field,
            value.is_finite() && value > 0.0,
            "must be finite and greater than zero",
        );
    }
    fn nonnegative(&mut self, field: &str, value: f32) {
        self.require(
            field,
            value.is_finite() && value >= 0.0,
            "must be finite and nonnegative",
        );
    }
    fn unit(&mut self, field: &str, value: f32) {
        self.require(
            field,
            value.is_finite() && (0.0..=1.0).contains(&value),
            "must be between zero and one",
        );
    }
    fn color<const N: usize>(&mut self, field: &str, value: &[f32; N]) {
        self.require(
            field,
            value.iter().all(|v| v.is_finite() && *v >= 0.0),
            "must contain finite nonnegative color values",
        );
    }
    fn asset(&mut self, field: &str, asset: &super::AssetRef) {
        if let Err(message) = asset.validate() {
            self.issues.push(SceneIssue {
                entity: Some(self.id),
                field: field.into(),
                message,
            });
        }
    }
}

impl Scene {
    /// Collect structural and value diagnostics without touching engine state.
    pub fn validate(&self) -> Result<(), SceneError> {
        let mut issues = Vec::new();
        let issue = |field: &str, message: &str| SceneIssue {
            entity: None,
            field: field.into(),
            message: message.into(),
        };
        if self.version != super::SCENE_VERSION {
            issues.push(issue(
                "version",
                "parse older files through SceneManager::parse before loading",
            ));
        }
        if self.name.trim().is_empty() {
            issues.push(issue("name", "a scene name is required"));
        }
        if self.entities.len() > MAX_ENTITIES {
            issues.push(issue("entities", "scene exceeds the 100,000 entity limit"));
        }
        let mut ids = HashMap::with_capacity(self.entities.len());
        for (index, entity) in self.entities.iter().enumerate() {
            if entity.id.0 == 0 || ids.insert(entity.id, index).is_some() {
                issues.push(SceneIssue {
                    entity: Some(entity.id),
                    field: "id".into(),
                    message: "keys must be nonzero and unique within this scene".into(),
                });
            }
            if entity.id.0 >= self.next_entity_id {
                issues.push(SceneIssue {
                    entity: Some(entity.id),
                    field: "next_entity_id".into(),
                    message: "the allocation counter must be greater than every entity key".into(),
                });
            }
            validate_entity(entity, &mut issues);
        }
        if self.next_entity_id == 0 {
            issues.push(issue(
                "next_entity_id",
                "the allocation counter cannot be zero",
            ));
        }
        for entity in &self.entities {
            if let Err(message) = crate::events::validate_rules(&entity.trigger_rules) {
                issues.push(SceneIssue {
                    entity: Some(entity.id),
                    field: "trigger_rules".into(),
                    message,
                });
            }
            if !entity.trigger_rules.is_empty()
                && (entity.trigger_volume.is_none()
                    || entity.collider_shape.is_none()
                    || entity.rigid_body.is_none())
            {
                issues.push(SceneIssue {
                    entity: Some(entity.id),
                    field: "trigger_rules".into(),
                    message: "trigger rules require TriggerVolume, ColliderShape and RigidBody"
                        .into(),
                });
            }
            for (index, rule) in entity.trigger_rules.iter().enumerate() {
                // Collect all bad references in a rule instead of stopping at the first one.
                let result: Result<_, std::convert::Infallible> = rule.map_entities(|key| {
                    if !ids.contains_key(key) {
                        issues.push(SceneIssue {
                            entity: Some(entity.id),
                            field: format!("trigger_rules[{index}]"),
                            message: format!("scene entity {key} does not exist"),
                        });
                    }
                    Ok(())
                });
                match result {
                    Ok(_) => {}
                    Err(never) => match never {},
                }
            }
            for (index, rule) in entity.trigger_rules.iter().enumerate() {
                for action in &rule.actions {
                    let target = match action {
                        katla_agent::events::EventAction::BurstParticles { target, .. }
                        | katla_agent::events::EventAction::SetParticlesActive { target, .. } => {
                            target
                        }
                        _ => continue,
                    };
                    let key = match target {
                        katla_agent::events::EventTarget::Trigger => entity.id,
                        katla_agent::events::EventTarget::Entity { entity } => *entity,
                        katla_agent::events::EventTarget::Other => continue,
                    };
                    if !ids
                        .get(&key)
                        .is_some_and(|index| self.entities[*index].particle_emitter.is_some())
                    {
                        issues.push(SceneIssue {
                            entity: Some(entity.id),
                            field: format!("trigger_rules[{index}]"),
                            message: format!("particle target {key} requires an emitter"),
                        });
                    }
                }
            }
            if let Some(parent) = entity.parent {
                if !ids.contains_key(&parent) {
                    issues.push(SceneIssue {
                        entity: Some(entity.id),
                        field: "parent".into(),
                        message: format!("scene entity {parent} does not exist"),
                    });
                }
                if parent == entity.id {
                    issues.push(SceneIssue {
                        entity: Some(entity.id),
                        field: "parent".into(),
                        message: "an entity cannot parent itself".into(),
                    });
                }
            }
            if let Some(joint) = &entity.joint {
                for (field, key) in [("joint.a", joint.a), ("joint.b", joint.b)] {
                    let valid_body =
                        ids.get(&key)
                            .map(|index| &self.entities[*index])
                            .is_some_and(|target| {
                                target.rigid_body.as_ref().is_some_and(|body| {
                                    body.kind != katla_physics::BodyType::Static
                                }) && target.collider_shape.is_some()
                            });
                    if !valid_body {
                        issues.push(SceneIssue { entity: Some(entity.id), field: field.into(), message: format!("scene entity {key} must have a dynamic or kinematic body and collider") });
                    }
                }
            }
        }
        // Each ancestry edge is visited once; malformed deep scenes cannot recurse.
        let mut finished = HashSet::new();
        for entity in &self.entities {
            if finished.contains(&entity.id) {
                continue;
            }
            let mut path = HashSet::new();
            let mut current = Some(entity.id);
            while let Some(id) = current {
                if finished.contains(&id) {
                    break;
                }
                if !path.insert(id) {
                    issues.push(SceneIssue {
                        entity: Some(id),
                        field: "parent".into(),
                        message: "hierarchy contains a cycle".into(),
                    });
                    break;
                }
                current = ids.get(&id).and_then(|index| self.entities[*index].parent);
            }
            finished.extend(path);
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(SceneError::Validation(issues))
        }
    }
}

fn validate_entity(entity: &EntityDescriptor, issues: &mut Vec<SceneIssue>) {
    let mut f = Fields {
        id: entity.id,
        issues,
    };
    f.finite("transform.position", &entity.transform.position);
    f.finite("transform.scale", &entity.transform.scale);
    f.require(
        "transform.scale",
        entity.transform.scale.iter().all(|value| *value != 0.0),
        "zero scale makes transforms singular",
    );
    f.finite("transform.rotation", &entity.transform.rotation);
    let length_squared: f64 = entity
        .transform
        .rotation
        .iter()
        .map(|v| (*v as f64).powi(2))
        .sum();
    f.require(
        "transform.rotation",
        (length_squared - 1.0).abs() <= 0.002,
        "rotation must be a normalized XYZW quaternion",
    );
    match &entity.source {
        EntitySource::Empty
        | EntitySource::Light
        | EntitySource::ParticleEmitter
        | EntitySource::Trigger => {}
        EntitySource::Cube { size } => {
            for value in size {
                f.positive("source.size", *value);
            }
        }
        EntitySource::Plane { width, height } => {
            f.positive("source.width", *width);
            f.positive("source.height", *height);
        }
        EntitySource::Sphere {
            radius,
            segments,
            rings,
        } => {
            f.positive("source.radius", *radius);
            f.require(
                "source.segments",
                *segments >= 3,
                "spheres need at least 3 segments",
            );
            f.require("source.rings", *rings >= 3, "spheres need at least 3 rings");
            f.require(
                "source",
                (*segments as u64 + 1).saturating_mul(*rings as u64 + 1) <= MAX_GEOMETRY_POINTS,
                "mesh exceeds the one million vertex budget",
            );
        }
        EntitySource::Cylinder {
            radius,
            height,
            segments,
        } => {
            f.positive("source.radius", *radius);
            f.positive("source.height", *height);
            f.require(
                "source.segments",
                *segments >= 3 && *segments as u64 * 4 + 6 <= MAX_GEOMETRY_POINTS,
                "cylinder segments must fit the vertex budget and be at least 3",
            );
        }
        EntitySource::Torus {
            radius,
            tube_radius,
            segments,
            tube_segments,
        } => {
            f.positive("source.radius", *radius);
            f.positive("source.tube_radius", *tube_radius);
            f.require(
                "source.segments",
                *segments >= 3 && *tube_segments >= 3,
                "torus rings need at least 3 segments",
            );
            f.require(
                "source",
                (*segments as u64 + 1).saturating_mul(*tube_segments as u64 + 1)
                    <= MAX_GEOMETRY_POINTS,
                "mesh exceeds the one million vertex budget",
            );
        }
        EntitySource::GltfModel { path }
        | EntitySource::GltfGroup { path }
        | EntitySource::GltfPrimitive { path, .. }
        | EntitySource::StlModel { path }
        | EntitySource::MeshAsset { path } => f.asset("source.path", path),
    }
    if let Some(d) = &entity.drawable {
        f.require(
            "drawable",
            entity.source.is_mesh_primitive()
                || matches!(
                    entity.source,
                    EntitySource::GltfModel { .. }
                        | EntitySource::GltfPrimitive { .. }
                        | EntitySource::StlModel { .. }
                        | EntitySource::MeshAsset { .. }
                ),
            "drawable parameters require a reproducible mesh source",
        );
        f.unit("drawable.metallic", d.metallic);
        f.unit("drawable.roughness", d.roughness);
        f.unit("drawable.ao", d.ao);
        if let Some(color) = d.color {
            f.color("drawable.color", &color);
            f.unit("drawable.color.alpha", color[3]);
        }
    }
    if let Some(light) = &entity.point_light {
        f.color("point_light.color", &light.color);
        f.nonnegative("point_light.intensity", light.intensity);
        f.positive("point_light.range", light.range);
    }
    if let Some(light) = &entity.directional_light {
        f.color("directional_light.color", &light.color);
        f.nonnegative("directional_light.intensity", light.intensity);
        f.finite("directional_light.direction", &light.direction);
        f.require(
            "directional_light.direction",
            light.direction.iter().any(|v| *v != 0.0),
            "a nonzero direction is required",
        );
    }
    if let Some(velocity) = &entity.velocity {
        f.finite("velocity.velocity", &velocity.velocity);
        f.finite("velocity.acceleration", &velocity.acceleration);
    }
    if let Some(camera) = &entity.perspective {
        f.positive("perspective.near", camera.near);
        f.positive("perspective.aspect_ratio", camera.aspect_ratio);
        f.require(
            "perspective.fov",
            camera.fov.is_finite() && camera.fov > 0.0 && camera.fov < 180.0,
            "field of view must be in degrees between zero and 180",
        );
    }
    if let Some(script) = &entity.script {
        f.asset("script.path", &script.path);
    }
    if let Some(audio) = &entity.audio_emitter {
        f.asset("audio_emitter.path", &audio.path);
        f.unit("audio_emitter.volume", audio.volume);
        f.positive("audio_emitter.min_distance", audio.min_distance);
        f.require(
            "audio_emitter.max_distance",
            audio.max_distance.is_finite() && audio.max_distance > audio.min_distance,
            "maximum distance must exceed minimum distance",
        );
        f.nonnegative("audio_emitter.rolloff_factor", audio.rolloff_factor);
    }
    if let Some(zone) = &entity.reverb_zone {
        f.require(
            "reverb_zone.decay",
            zone.decay.is_finite() && (0.0..=0.99).contains(&zone.decay),
            "reverb decay must be between zero and 0.99",
        );
        f.unit("reverb_zone.wet", zone.wet);
        f.unit("reverb_zone.dampening", zone.dampening);
        for value in zone.half_extents {
            f.positive("reverb_zone.half_extents", value);
        }
    }
    if let Some(body) = &entity.rigid_body {
        f.require(
            "rigid_body.gravity_scale",
            body.gravity_scale.is_finite(),
            "must be finite",
        );
        f.finite("rigid_body.linear_velocity", &body.linear_velocity);
    }
    if let Some(material) = &entity.physics_material {
        f.nonnegative("physics_material.friction", material.friction);
        f.unit("physics_material.restitution", material.restitution);
        f.positive("physics_material.density", material.density);
    }
    if entity.trigger_volume.is_some()
        || entity.collision_filter.is_some()
        || entity.physics_material.is_some()
    {
        f.require(
            "collider_shape",
            entity.collider_shape.is_some(),
            "collision properties require a collider",
        );
    }
    if let Some(shape) = &entity.collider_shape {
        match shape {
            ColliderShapeDescriptor::Sphere(radius) => f.positive("collider_shape.radius", *radius),
            ColliderShapeDescriptor::Box(extents) => {
                for value in extents {
                    f.positive("collider_shape.half_extents", *value);
                }
            }
            ColliderShapeDescriptor::Capsule {
                half_height,
                radius,
            } => {
                f.nonnegative("collider_shape.half_height", *half_height);
                f.positive("collider_shape.radius", *radius);
            }
            ColliderShapeDescriptor::Trimesh | ColliderShapeDescriptor::ConvexHull => f.require(
                "collider_shape",
                matches!(
                    entity.source,
                    EntitySource::GltfModel { .. }
                        | EntitySource::GltfGroup { .. }
                        | EntitySource::GltfPrimitive { .. }
                        | EntitySource::StlModel { .. }
                        | EntitySource::MeshAsset { .. }
                ),
                "mesh colliders require a model with retained CPU geometry",
            ),
            ColliderShapeDescriptor::Heightfield {
                rows,
                cols,
                heights,
            } => {
                let count = *rows as u64 * *cols as u64;
                f.require(
                    "collider_shape.heights",
                    *rows >= 2
                        && *cols >= 2
                        && count <= MAX_GEOMETRY_POINTS
                        && count == heights.len() as u64,
                    "height dimensions must match finite data and fit the geometry budget",
                );
                f.require(
                    "collider_shape.heights",
                    heights.iter().all(|value| value.is_finite()),
                    "must contain finite heights",
                );
            }
        }
    }
    if let Some(animation) = &entity.animation {
        for (field, value) in [
            ("time", animation.time),
            ("duration", animation.duration),
            ("target_time", animation.target_time),
            ("target_duration", animation.target_duration),
            ("blend_time", animation.blend_time),
            ("blend_duration", animation.blend_duration),
        ] {
            f.nonnegative(&format!("animation.{field}"), value);
        }
        f.require(
            "animation.speed",
            animation.speed.is_finite(),
            "must be finite",
        );
        f.unit("animation.blend_weight", animation.blend_weight);
        f.require(
            "animation.current_clip",
            !animation.playing
                || animation
                    .current_clip
                    .as_ref()
                    .is_some_and(|clip| !clip.is_empty()),
            "playing requires a clip name",
        );
        f.require(
            "animation.target_clip",
            !animation.blending
                || (animation
                    .target_clip
                    .as_ref()
                    .is_some_and(|clip| !clip.is_empty())
                    && animation.blend_duration > 0.0),
            "blending requires a target clip and a positive duration",
        );
    }
    if let Some(p) = &entity.particle_emitter {
        f.nonnegative("particle_emitter.emit_rate", p.emit_rate);
        f.positive("particle_emitter.base_lifetime", p.base_lifetime);
        f.unit("particle_emitter.lifetime_variation", p.lifetime_variation);
        f.finite("particle_emitter.velocity_direction", &p.velocity_direction);
        f.require(
            "particle_emitter.velocity_direction",
            p.velocity_direction.iter().any(|v| *v != 0.0),
            "a nonzero direction is required",
        );
        f.nonnegative("particle_emitter.velocity_magnitude", p.velocity_magnitude);
        f.nonnegative(
            "particle_emitter.velocity_cone_angle",
            p.velocity_cone_angle,
        );
        f.positive("particle_emitter.base_scale", p.base_scale);
        f.unit("particle_emitter.scale_variation", p.scale_variation);
        f.nonnegative("particle_emitter.scale_end", p.scale_end);
        f.color("particle_emitter.color", &p.color);
        f.color("particle_emitter.color_end", &p.color_end);
        f.unit("particle_emitter.color_variation", p.color_variation);
        f.finite("particle_emitter.shape_params", &p.shape_params);
        f.require(
            "particle_emitter.gravity",
            p.gravity.is_finite(),
            "must be finite",
        );
        f.nonnegative(
            "particle_emitter.turbulence_strength",
            p.turbulence_strength,
        );
        f.nonnegative(
            "particle_emitter.turbulence_frequency",
            p.turbulence_frequency,
        );
        if let Some(time) = p.timed_emission {
            f.nonnegative("particle_emitter.timed_emission", time);
        }
        f.require(
            "particle_emitter.burst_queue",
            p.burst_queue.len() <= 1024
                && p.burst_queue
                    .iter()
                    .all(|count| (1..=100_000).contains(count)),
            "burst queue requires at most 1024 entries of 1..100000 particles",
        );
    }
    if let Some(joint) = &entity.joint {
        f.require(
            "joint",
            joint.a != joint.b,
            "joint endpoints must be distinct",
        );
        f.finite("joint.anchor_a", &joint.anchor_a);
        f.finite("joint.anchor_b", &joint.anchor_b);
        if let Some([min, max]) = joint.limits {
            f.require(
                "joint.limits",
                min.is_finite() && max.is_finite() && min <= max,
                "limits must be finite and ordered",
            );
        }
    }
    for (key, component) in &entity.components {
        f.require(
            &format!("components.{key}"),
            super::component_registry::valid_component_key(key),
            "component keys must be namespaced",
        );
        f.require(
            &format!("components.{key}.version"),
            component.version > 0,
            "versions start at one",
        );
        f.require(
            &format!("components.{key}.data"),
            component.data.len() <= 1024 * 1024,
            "component payload exceeds one MiB",
        );
    }
}
