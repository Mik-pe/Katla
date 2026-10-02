use super::descriptors::{
    AnimationDescriptor, AudioEmitterDescriptor, ColliderShapeDescriptor,
    CollisionFilterDescriptor, DirectionalLightDescriptor, DrawableDescriptor, EntityDescriptor,
    ParticleEmitterDescriptor, PerspectiveDescriptor, PhysicsMaterialDescriptor,
    PointLightDescriptor, RigidBodyDescriptor, RigidBodyPropertiesDescriptor, Scene,
    ScriptDescriptor, TransformDescriptor, TriggerVolumeDescriptor, VelocityDescriptor,
};
use super::entity_source::EntitySource;
use log::{debug, info, warn};
use std::path::Path;

use katla_gfx::GpuRenderer;
use katla_gfx::primitives;

use crate::animation::AnimationPlayer;
use crate::application::Application;
use crate::components::ParticleEmitterComponent;
use crate::components::{
    DirectionalLight, DrawableComponent, NameComponent, PerspectiveComponent, PointLight,
    TransformComponent, VelocityComponent,
};
use katla_physics::{
    BodyType, BoxShape, CapsuleShape, ColliderShape, CollisionFilter, HeightfieldShape,
    PhysicsMaterial, RigidBody, SphereShape, TriggerVolume,
};
use katla_script::ScriptComponent;

use ron::extensions::Extensions;

/// Current scene format version.
pub const SCENE_VERSION: u32 = 1;

/// RON serialization extensions configuration.
///
/// Enables concise optional and newtype values. Comments and formatting are
/// accepted when reading but regenerated when saving.
const RON_EXTENSIONS: Extensions = Extensions::IMPLICIT_SOME
    .union(Extensions::UNWRAP_NEWTYPES)
    .union(Extensions::UNWRAP_VARIANT_NEWTYPES);

pub fn ron_pretty_config() -> ron::ser::PrettyConfig {
    ron::ser::PrettyConfig::new()
        .enumerate_arrays(true)
        .extensions(RON_EXTENSIONS)
}

/// Manages scene save/load operations.
pub struct SceneManager;

impl SceneManager {
    /// Serialize the current world state into a `Scene` descriptor.
    ///
    /// Queries all entities with `TransformComponent` and gathers their
    /// serializable data using `EntitySource` to determine origin.
    pub fn save_scene(app: &Application) -> Scene {
        let mut scene = app.scene_document.saved.clone();
        scene.entities.clear();
        scene.version = SCENE_VERSION;
        let timestamp = {
            use std::time::SystemTime;
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs().to_string())
        };
        scene.created_at = scene.created_at.or_else(|| timestamp.clone());
        scene.modified_at = timestamp;
        scene.engine_version = Some(env!("CARGO_PKG_VERSION").to_string());

        let mut serialized_names = std::collections::HashMap::new();
        let mut used_names = std::collections::HashSet::new();
        let entities: Vec<_> = app
            .world
            .query_ref::<&TransformComponent>()
            .map(|(id, _)| id)
            .filter(|id| {
                app.world
                    .get_component::<crate::components::EditorHidden>(*id)
                    .is_none()
                    && app.world.get_component::<EntitySource>(*id).is_some()
            })
            .collect();
        let reserved_names: std::collections::HashSet<_> = entities
            .iter()
            .filter_map(|id| {
                app.world
                    .get_component::<NameComponent>(*id)
                    .map(|name| name.name.clone())
            })
            .collect();
        for id in entities {
            let base = app
                .world
                .get_component::<NameComponent>(id)
                .map(|name| name.name.clone())
                .unwrap_or_else(|| format!("Entity {}", id.id()));
            let mut name = base.clone();
            let mut suffix = 2;
            while used_names.contains(&name) || (name != base && reserved_names.contains(&name)) {
                name = format!("{base} ({suffix})");
                suffix += 1;
            }
            used_names.insert(name.clone());
            serialized_names.insert(id, name);
        }
        for (entity_id, transform) in app.world.query_ref::<&TransformComponent>() {
            let Some(name) = serialized_names.get(&entity_id).cloned() else {
                continue;
            };
            let name = Some(name);
            let parent = app
                .world
                .get_component::<crate::components::Parent>(entity_id)
                .and_then(|parent| serialized_names.get(&parent.parent).cloned());
            let t = &transform.transform;
            let transform_desc = TransformDescriptor {
                position: [t.position.x(), t.position.y(), t.position.z()],
                rotation: {
                    let (x, y, z, w) = t.rotation.xyzw();
                    [x, y, z, w]
                },
                scale: [t.scale.x(), t.scale.y(), t.scale.z()],
            };

            let Some(source) = app.world.get_component::<EntitySource>(entity_id).cloned() else {
                warn!(
                    "Entity {:?} has no EntitySource -- skipping (cannot round-trip without knowing origin)",
                    entity_id
                );
                continue;
            };

            let drawable = app
                .world
                .get_component::<DrawableComponent>(entity_id)
                .map(|d| DrawableDescriptor {
                    color: d.color.map(|c| {
                        let srgb = c.to_srgb();
                        [srgb.r, srgb.g, srgb.b, srgb.a]
                    }),
                    metallic: d.metallic,
                    roughness: d.roughness,
                    ao: d.ao,
                });

            let point_light =
                app.world
                    .get_component::<PointLight>(entity_id)
                    .map(|l| PointLightDescriptor {
                        color: l.color,
                        intensity: l.intensity,
                        range: l.range,
                    });

            let _particle_emitter = app
                .world
                .get_component::<ParticleEmitterComponent>(entity_id)
                .map(|p| ParticleEmitterDescriptor {
                    position: p.config.position,
                    emit_rate: p.config.emit_rate,
                    base_lifetime: p.config.base_lifetime,
                    lifetime_variation: p.config.lifetime_variation,
                    velocity_direction: p.config.velocity_direction,
                    velocity_magnitude: p.config.velocity_magnitude,
                    velocity_cone_angle: p.config.velocity_cone_angle,
                    base_scale: p.config.base_scale,
                    scale_variation: p.config.scale_variation,
                    color: p.config.color,
                    color_variation: p.config.color_variation,
                    gravity: p.config.gravity,
                    turbulence_strength: p.config.turbulence_strength,
                    turbulence_frequency: p.config.turbulence_frequency,
                    shape: p.config.shape,
                    shape_params: p.config.shape_params,
                    active: p.active,
                });
            let particle_emitter: Option<ParticleEmitterDescriptor> = None;

            let animation = app
                .world
                .get_component::<AnimationPlayer>(entity_id)
                .map(AnimationDescriptor::from);

            let velocity = app
                .world
                .get_component::<VelocityComponent>(entity_id)
                .map(|v| VelocityDescriptor {
                    velocity: [v.velocity.x(), v.velocity.y(), v.velocity.z()],
                    acceleration: [v.acceleration.x(), v.acceleration.y(), v.acceleration.z()],
                });

            let script = app
                .world
                .get_component::<ScriptComponent>(entity_id)
                .map(|s| ScriptDescriptor {
                    script_path: s.script_path.clone(),
                });

            let perspective = app
                .world
                .get_component::<PerspectiveComponent>(entity_id)
                .map(|p| PerspectiveDescriptor {
                    fov: p.fov,
                    near: p.near,
                    aspect_ratio: p.aspect_ratio,
                });

            let directional_light =
                app.world
                    .get_component::<DirectionalLight>(entity_id)
                    .map(|l| DirectionalLightDescriptor {
                        direction: [l.direction.x(), l.direction.y(), l.direction.z()],
                        color: l.color,
                        intensity: l.intensity,
                    });

            let audio_emitter = app
                .world
                .get_component::<crate::components::AudioEmitter>(entity_id)
                .map(|a| AudioEmitterDescriptor {
                    source_path: a.source_path.clone(),
                    volume: a.volume,
                    looping: a.looping,
                    playing: a.playing,
                    spatial: a.spatial,
                    min_distance: a.min_distance,
                    max_distance: a.max_distance,
                    rolloff_factor: a.rolloff_factor,
                    distance_model: a.distance_model,
                });

            let rigid_body =
                app.world
                    .get_component::<RigidBody>(entity_id)
                    .map(|rb| match rb.body_type {
                        BodyType::Static => RigidBodyDescriptor::Static,
                        BodyType::Dynamic => RigidBodyDescriptor::Dynamic,
                        BodyType::Kinematic => RigidBodyDescriptor::Kinematic,
                    });

            let rigid_body_properties =
                app.world.get_component::<RigidBody>(entity_id).map(|body| {
                    RigidBodyPropertiesDescriptor {
                        gravity_scale: body.gravity_scale,
                        ccd_enabled: body.ccd_enabled,
                        linear_velocity: [
                            body.linear_velocity.x(),
                            body.linear_velocity.y(),
                            body.linear_velocity.z(),
                        ],
                    }
                });
            let reverb_zone = app
                .world
                .get_component::<crate::components::ReverbZone>(entity_id)
                .cloned();

            let collider_shape = app
                .world
                .get_component::<ColliderShape>(entity_id)
                .map(|cs| match cs {
                    ColliderShape::Sphere(s) => ColliderShapeDescriptor::Sphere(s.radius),
                    ColliderShape::Box(b) => ColliderShapeDescriptor::Box(b.half_extents),
                    ColliderShape::Capsule(c) => ColliderShapeDescriptor::Capsule {
                        half_height: c.half_height,
                        radius: c.radius,
                    },
                    ColliderShape::Trimesh(_) => ColliderShapeDescriptor::Trimesh {
                        mesh_handle_index: 0,
                        mesh_handle_generation: 0,
                    },
                    ColliderShape::ConvexHull(_) => ColliderShapeDescriptor::ConvexHull {
                        mesh_handle_index: 0,
                        mesh_handle_generation: 0,
                    },
                    ColliderShape::Heightfield(h) => ColliderShapeDescriptor::Heightfield {
                        rows: h.rows,
                        cols: h.cols,
                        heights: h.heights.clone(),
                    },
                });

            let physics_material =
                app.world
                    .get_component::<PhysicsMaterial>(entity_id)
                    .map(|pm| PhysicsMaterialDescriptor {
                        friction: pm.friction,
                        restitution: pm.restitution,
                        density: pm.density,
                    });

            let trigger_volume = app
                .world
                .get_component::<TriggerVolume>(entity_id)
                .map(|_| TriggerVolumeDescriptor);

            let collision_filter =
                app.world
                    .get_component::<CollisionFilter>(entity_id)
                    .map(|cf| CollisionFilterDescriptor {
                        layers: cf.layers,
                        mask: cf.mask,
                    });

            scene.entities.push(EntityDescriptor {
                name,
                parent,
                transform: transform_desc,
                source,
                drawable,
                point_light,
                particle_emitter,
                animation,
                velocity,
                script,
                perspective,
                directional_light,
                audio_emitter,
                rigid_body,
                rigid_body_properties,
                reverb_zone,
                collider_shape,
                physics_material,
                trigger_volume,
                collision_filter,
            });
        }

        scene.entities.sort_by(|a, b| a.name.cmp(&b.name));
        debug!(
            "Serialized scene '{}' with {} entities",
            scene.name,
            scene.entities.len()
        );
        scene
    }

    /// Save a scene to a RON file.
    ///
    /// Preserves the loaded document metadata and updates its path and saved
    /// baseline only after the complete file has been atomically replaced.
    pub fn save_to_file(app: &mut Application, path: &Path) -> Result<(), String> {
        let scene = Self::save_scene(app);
        let ron_string = ron::ser::to_string_pretty(&scene, ron_pretty_config())
            .map_err(|e| format!("Failed to serialize scene: {e}"))?;
        crate::util::config::write_atomic(path, ron_string.as_bytes())
            .map_err(|e| format!("Failed to save scene to {}: {e}", path.display()))?;
        app.scene_document.saved = scene;
        app.scene_document.path = Some(path.to_path_buf());
        info!("Saved scene to {}", path.display());
        Ok(())
    }

    /// Load a scene from a RON file and populate the world.
    ///
    /// Stages descriptors before retiring the previous scene, preserving it
    /// if a model or resource cannot be loaded. Runs migrations if the scene
    /// version is older than [`SCENE_VERSION`].
    pub fn load_from_file(app: &mut Application, path: &Path) -> Result<(), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read scene file {:?}: {}", path, e))?;

        let scene: Scene =
            ron::from_str(&content).map_err(|e| format!("Failed to parse scene file: {}", e))?;

        Self::load_scene(app, scene)?;
        app.scene_document.path = Some(path.to_path_buf());
        Ok(())
    }

    /// Load a scene descriptor into the world.
    ///
    /// Stages descriptors before retiring the previous scene, preserving it
    /// if a model or resource cannot be loaded. Runs migrations if the scene
    /// version is older than [`SCENE_VERSION`]. Returns an error if the scene
    /// version is newer than this build supports.
    pub fn load_scene(app: &mut Application, mut scene: Scene) -> Result<(), String> {
        let loaded_version = scene.version;

        // Run migrations before spawning entities
        super::migration::run_migrations(&mut scene)
            .map_err(|e| format!("Cannot load scene '{}': {}", scene.name, e))?;

        info!(
            "Loading scene '{}' (version {}{}) with {} entities",
            scene.name,
            loaded_version,
            if loaded_version != scene.version {
                format!(" → migrated to {}", scene.version)
            } else {
                String::new()
            },
            scene.entities.len()
        );

        validate_hierarchy(&scene)?;
        app.renderer.wait_for_device();
        let previous_entities: std::collections::HashSet<_> = app.world.entity_ids().collect();
        let mut previous_tracker = app.gpu_resource_tracker.clone();
        let mut spawned_ids = Vec::with_capacity(scene.entities.len());
        let mut name_to_entity = std::collections::HashMap::new();
        for desc in &scene.entities {
            match Self::spawn_entity(app, desc) {
                Ok(entity) => {
                    if let Some(name) = &desc.name {
                        name_to_entity.entry(name.clone()).or_insert(entity);
                    }
                    spawned_ids.push(Some(entity));
                }
                Err(error) => {
                    let prepared: Vec<_> = app
                        .world
                        .entity_ids()
                        .filter(|id| !previous_entities.contains(id))
                        .collect();
                    for id in prepared {
                        app.world.destroy_entity(id);
                    }
                    let abandoned = app.gpu_resource_tracker.rollback_to(previous_tracker);
                    destroy_resources(app, abandoned);
                    return Err(format!(
                        "Cannot load scene '{}': {error}. The current scene was kept.",
                        scene.name
                    ));
                }
            }
        }

        // Second pass: resolve parent relationships.
        // All entities are already spawned in the first pass, so entity ordering
        // in the scene file does not matter for parent resolution.
        // Uses index-based lookup for children so unnamed entities can have parents.
        for (idx, desc) in scene.entities.iter().enumerate() {
            let Some(child_id) = spawned_ids[idx] else {
                continue;
            };
            if let Some(ref parent_name) = desc.parent {
                if let Some(&parent_id) = name_to_entity.get(parent_name) {
                    app.world
                        .add_component(child_id, crate::components::Parent::new(parent_id));
                    if let Some(children) = app
                        .world
                        .get_component_mut::<crate::components::Children>(parent_id)
                    {
                        children.children.push(child_id);
                    } else {
                        app.world.add_component(
                            parent_id,
                            crate::components::Children::new(vec![child_id]),
                        );
                    }
                } else {
                    warn!(
                        "Parent '{}' not found for entity '{:?}'",
                        parent_name, desc.name
                    );
                }
            }
        }

        for id in previous_entities {
            if app
                .world
                .get_component::<crate::components::EditorHidden>(id)
                .is_some()
            {
                if let Some(drawable) = app.world.get_component::<DrawableComponent>(id) {
                    previous_tracker.release_drawable(
                        drawable.mesh_handle,
                        drawable.material_handle,
                        drawable.skeleton_handle,
                    );
                }
                continue;
            }
            if let Some(emitter) = app.world.get_component_mut::<ParticleEmitterComponent>(id)
                && let Some(handle) = emitter.emitter_handle.take()
                && let Some(features) = &mut app.scene_features
            {
                katla_gfx::ParticleEmitterDriver::destroy_emitter(
                    &mut features.particles,
                    handle,
                    emitter.kill_on_destroy,
                );
            }
            app.world.destroy_entity(id);
        }
        let retired = app.gpu_resource_tracker.retire_snapshot(previous_tracker);
        destroy_resources(app, retired);
        app.scene_document.saved = scene;
        app.scene_document.saved = Self::save_scene(app);
        app.scene_document.path = None;
        #[cfg(feature = "editor")]
        app.editor.clear_entity_references();
        Ok(())
    }

    /// Spawn a single entity from its descriptor.
    fn spawn_entity(
        app: &mut Application,
        desc: &EntityDescriptor,
    ) -> Result<katla_ecs::EntityId, String> {
        let pos = desc.transform.position;
        let (qx, qy, qz, qw) = (
            desc.transform.rotation[0],
            desc.transform.rotation[1],
            desc.transform.rotation[2],
            desc.transform.rotation[3],
        );
        let (sx, sy, sz) = (
            desc.transform.scale[0],
            desc.transform.scale[1],
            desc.transform.scale[2],
        );

        let entity_id = if desc.source.is_mesh_primitive() {
            let mesh_result = match &desc.source {
                EntitySource::Cube { size } => primitives::create_cube(&mut app.renderer, *size),
                EntitySource::Sphere {
                    radius,
                    segments,
                    rings,
                    ..
                } => primitives::create_sphere(&mut app.renderer, *radius, *segments, *rings),
                EntitySource::Plane { width, height } => {
                    primitives::create_plane(&mut app.renderer, *width, *height)
                }
                EntitySource::Cylinder {
                    height,
                    radius,
                    segments,
                    ..
                } => primitives::create_cylinder(&mut app.renderer, *height, *radius, *segments),
                EntitySource::Torus {
                    radius,
                    tube_radius,
                    segments,
                    tube_segments,
                    ..
                } => primitives::create_torus(
                    &mut app.renderer,
                    *radius,
                    *tube_radius,
                    *segments,
                    *tube_segments,
                ),
                _ => unreachable!(),
            };
            let mesh_handle = match mesh_result {
                Ok(mesh_handle) => mesh_handle,
                Err(error) => return Err(format!("primitive mesh creation failed: {error}")),
            };

            let material_handle = app.default_material();
            let srgb_color = color_from_desc(&desc.drawable);
            let linear_color = srgb_color.to_linear();

            let drawable = DrawableComponent::with_handles_and_color(
                mesh_handle,
                material_handle,
                linear_color,
            )
            .with_bounds(crate::application::spawning::local_bounds_for_source(
                &desc.source,
            ));
            app.gpu_resource_tracker.track_drawable(
                mesh_handle,
                material_handle,
                drawable.skeleton_handle,
            );

            app.world.spawn((
                TransformComponent::from_position(katla_math::Vec3::new(pos[0], pos[1], pos[2])),
                drawable,
            ))
        } else {
            match &desc.source {
                EntitySource::GltfModel { path } => app
                    .spawn_gltf_model(path, pos, None)
                    .map_err(|e| format!("{e}"))?,
                EntitySource::StlModel { path } => {
                    app.spawn_stl_model(path, pos).map_err(|e| format!("{e}"))?
                }
                EntitySource::ParticleEmitter => {
                    let config = katla_gfx::particles::EmitterConfig {
                        position: desc
                            .particle_emitter
                            .as_ref()
                            .map(|p| p.position)
                            .unwrap_or(pos),
                        ..Default::default()
                    };
                    let mut emitter = ParticleEmitterComponent::with_config(config);
                    if let Some(ref pe) = desc.particle_emitter {
                        emitter.active = pe.active;
                    }
                    let transform = TransformComponent::from_position(katla_math::Vec3::new(
                        pos[0], pos[1], pos[2],
                    ));
                    let entity_id = app.world.spawn((transform, emitter));
                    app.attach_billboard_icon(
                        entity_id,
                        crate::components::billboard::BillboardIcon::Fire,
                    );
                    entity_id
                }
                EntitySource::Light => {
                    let point_light = desc
                        .point_light
                        .as_ref()
                        .map(|pl| PointLight::new(pl.color, pl.intensity, pl.range))
                        .unwrap_or_default();

                    let transform = TransformComponent::from_position(katla_math::Vec3::new(
                        pos[0], pos[1], pos[2],
                    ));

                    let entity_id = app.world.spawn((transform, point_light));
                    app.attach_billboard_icon(
                        entity_id,
                        crate::components::billboard::BillboardIcon::Lightbulb,
                    );
                    entity_id
                }
                _ => return Err(format!("Unknown entity source: {:?}", desc.source)),
            }
        };

        // Apply transform (rotation + scale) -- spawn functions only set position
        if let Some(transform) = app.world.get_component_mut::<TransformComponent>(entity_id) {
            transform.transform.rotation = katla_math::Quat::new(qx, qy, qz, qw);
            transform.transform.scale = katla_math::Vec3::new(sx, sy, sz);
        }

        // Apply drawable material overrides
        if let Some(ref drawable_desc) = desc.drawable
            && let Some(drawable) = app.world.get_component_mut::<DrawableComponent>(entity_id)
        {
            drawable.metallic = drawable_desc.metallic;
            drawable.roughness = drawable_desc.roughness;
            drawable.ao = drawable_desc.ao;
            if let Some(c) = drawable_desc.color {
                let srgb = katla_math::Color::new(c[0], c[1], c[2], c[3]);
                drawable.color = Some(srgb.to_linear());
            }
        }

        // Apply particle emitter config overrides
        if let Some(ref pe_desc) = desc.particle_emitter
            && let Some(emitter) = app
                .world
                .get_component_mut::<ParticleEmitterComponent>(entity_id)
        {
            emitter.config.position = pe_desc.position;
            emitter.config.emit_rate = pe_desc.emit_rate;
            emitter.config.base_lifetime = pe_desc.base_lifetime;
            emitter.config.lifetime_variation = pe_desc.lifetime_variation;
            emitter.config.velocity_direction = pe_desc.velocity_direction;
            emitter.config.velocity_magnitude = pe_desc.velocity_magnitude;
            emitter.config.velocity_cone_angle = pe_desc.velocity_cone_angle;
            emitter.config.base_scale = pe_desc.base_scale;
            emitter.config.scale_variation = pe_desc.scale_variation;
            emitter.config.color = pe_desc.color;
            emitter.config.color_variation = pe_desc.color_variation;
            emitter.config.gravity = pe_desc.gravity;
            emitter.config.turbulence_strength = pe_desc.turbulence_strength;
            emitter.config.turbulence_frequency = pe_desc.turbulence_frequency;
            emitter.config.shape = pe_desc.shape;
            emitter.config.shape_params = pe_desc.shape_params;
            emitter.active = pe_desc.active;
        }

        // Apply animation state
        if let Some(ref anim_desc) = desc.animation {
            // For GLTF models, spawn_gltf_model only creates AnimationPlayer when
            // default_animation is Some. Ensure the component exists before restoring.
            if app
                .world
                .get_component::<AnimationPlayer>(entity_id)
                .is_none()
            {
                let player = if let Some(ref clip) = anim_desc.current_clip {
                    AnimationPlayer::new(clip.clone())
                } else {
                    AnimationPlayer::stopped()
                };
                app.world.add_component(entity_id, player);
            }

            if let Some(player) = app.world.get_component_mut::<AnimationPlayer>(entity_id) {
                anim_desc.restore(player);
            }
        }

        // Apply velocity
        if let Some(ref vel_desc) = desc.velocity {
            app.world.add_component(
                entity_id,
                VelocityComponent::new(
                    katla_math::Vec3::new(
                        vel_desc.velocity[0],
                        vel_desc.velocity[1],
                        vel_desc.velocity[2],
                    ),
                    katla_math::Vec3::new(
                        vel_desc.acceleration[0],
                        vel_desc.acceleration[1],
                        vel_desc.acceleration[2],
                    ),
                ),
            );
        }

        // Attach script
        if let Some(ref script_desc) = desc.script {
            app.world
                .add_component(entity_id, ScriptComponent::new(&script_desc.script_path));
        }

        // Apply perspective
        if let Some(ref persp_desc) = desc.perspective {
            app.world.add_component(
                entity_id,
                PerspectiveComponent::new(persp_desc.fov, persp_desc.near, persp_desc.aspect_ratio),
            );
        }

        // Apply directional light
        if let Some(ref dl_desc) = desc.directional_light {
            app.world.add_component(
                entity_id,
                DirectionalLight::new(
                    katla_math::Vec3::new(
                        dl_desc.direction[0],
                        dl_desc.direction[1],
                        dl_desc.direction[2],
                    ),
                    dl_desc.color,
                    dl_desc.intensity,
                ),
            );
        }

        // Attach audio emitter
        if let Some(ref audio_desc) = desc.audio_emitter {
            app.world.add_component(
                entity_id,
                crate::components::AudioEmitter {
                    source_path: audio_desc.source_path.clone(),
                    volume: audio_desc.volume,
                    looping: audio_desc.looping,
                    playing: audio_desc.playing,
                    spatial: audio_desc.spatial,
                    min_distance: audio_desc.min_distance,
                    max_distance: audio_desc.max_distance,
                    rolloff_factor: audio_desc.rolloff_factor,
                    distance_model: audio_desc.distance_model,
                },
            );
        }

        if let Some(zone) = &desc.reverb_zone {
            app.world.add_component(entity_id, zone.clone());
        }

        // Apply rigid body
        if let Some(ref rb_desc) = desc.rigid_body {
            let mut rb = match rb_desc {
                RigidBodyDescriptor::Static => RigidBody::static_body(),
                RigidBodyDescriptor::Dynamic => RigidBody::dynamic(),
                RigidBodyDescriptor::Kinematic => RigidBody::kinematic(),
            };
            if let Some(properties) = &desc.rigid_body_properties {
                rb.gravity_scale = properties.gravity_scale;
                rb.ccd_enabled = properties.ccd_enabled;
                rb.linear_velocity = katla_math::Vec3::new(
                    properties.linear_velocity[0],
                    properties.linear_velocity[1],
                    properties.linear_velocity[2],
                );
            }
            app.world.add_component(entity_id, rb);
        }

        // Apply collider shape
        if let Some(ref cs_desc) = desc.collider_shape {
            let shape = match cs_desc {
                ColliderShapeDescriptor::Sphere(radius) => {
                    ColliderShape::Sphere(SphereShape::new(*radius))
                }
                ColliderShapeDescriptor::Box(half_extents) => ColliderShape::Box(BoxShape {
                    half_extents: *half_extents,
                }),
                ColliderShapeDescriptor::Capsule {
                    half_height,
                    radius,
                } => ColliderShape::Capsule(CapsuleShape::new(*half_height, *radius)),
                ColliderShapeDescriptor::Trimesh { .. } => ColliderShape::Trimesh(
                    app.world
                        .get_component::<DrawableComponent>(entity_id)
                        .ok_or("Mesh collider requires a drawable")?
                        .mesh_handle,
                ),
                ColliderShapeDescriptor::ConvexHull { .. } => ColliderShape::ConvexHull(
                    app.world
                        .get_component::<DrawableComponent>(entity_id)
                        .ok_or("Convex collider requires a drawable")?
                        .mesh_handle,
                ),
                ColliderShapeDescriptor::Heightfield {
                    rows,
                    cols,
                    heights,
                } => {
                    ColliderShape::Heightfield(HeightfieldShape::new(*rows, *cols, heights.clone()))
                }
            };
            app.world.add_component(entity_id, shape);
        }

        // Apply physics material
        if let Some(ref pm_desc) = desc.physics_material {
            app.world.add_component(
                entity_id,
                PhysicsMaterial::new(pm_desc.friction, pm_desc.restitution, pm_desc.density),
            );
        }

        // Apply trigger volume
        if desc.trigger_volume.is_some() {
            app.world.add_component(entity_id, TriggerVolume::new());
        }

        // Apply collision filter
        if let Some(ref cf_desc) = desc.collision_filter {
            app.world.add_component(
                entity_id,
                CollisionFilter::new(cf_desc.layers, cf_desc.mask),
            );
        }

        // Attach EntitySource for future serialization
        app.world.add_component(entity_id, desc.source.clone());

        // Attach name
        let name = desc
            .name
            .clone()
            .unwrap_or_else(|| desc.source.display_name());
        app.world.add_component(entity_id, NameComponent::new(name));

        Ok(entity_id)
    }
}

fn color_from_desc(drawable: &Option<DrawableDescriptor>) -> katla_math::Color {
    drawable
        .as_ref()
        .and_then(|d| d.color)
        .map(|c| katla_math::Color::new(c[0], c[1], c[2], c[3]))
        .unwrap_or(katla_math::Color::WHITE)
}

fn destroy_resources(
    app: &mut Application,
    resources: crate::gpu_resource_tracker::GpuResourcesToDestroy,
) {
    for handle in resources.meshes {
        app.geometry_cache.remove(handle);
        if let Some(cache) = app
            .world
            .get_resource_mut::<crate::geometry_cache::GeometryCache>()
        {
            cache.remove(handle);
        }
        app.renderer.destroy_mesh(handle);
    }
    for handle in resources.materials {
        app.renderer.destroy_material(handle);
    }
    for handle in resources.textures {
        app.renderer.destroy_texture(handle);
    }
    for handle in resources.skeletons {
        app.renderer.destroy_skeleton(handle);
    }
}

pub(crate) fn validate_hierarchy(scene: &Scene) -> Result<(), String> {
    let mut names = std::collections::HashMap::new();
    for (index, entity) in scene.entities.iter().enumerate() {
        if let Some(name) = &entity.name {
            names.entry(name.as_str()).or_insert(index);
        }
    }
    for (index, entity) in scene.entities.iter().enumerate() {
        let mut visited = std::collections::HashSet::from([index]);
        let mut parent = entity.parent.as_deref();
        while let Some(name) = parent {
            let parent_index = names
                .get(name)
                .copied()
                .ok_or_else(|| format!("Parent '{name}' does not exist"))?;
            if !visited.insert(parent_index) {
                return Err(format!("Scene hierarchy contains a cycle at '{name}'"));
            }
            parent = scene.entities[parent_index].parent.as_deref();
        }
    }
    Ok(())
}
