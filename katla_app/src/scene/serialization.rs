use super::descriptors::{
    AnimationDescriptor, DirectionalLightDescriptor, DragDescriptor, DrawableDescriptor,
    EntityDescriptor, MassDescriptor, ParticleEmitterDescriptor, PerspectiveDescriptor,
    PointLightDescriptor, Scene, ScriptDescriptor, TransformDescriptor, VelocityDescriptor,
};
use super::entity_source::EntitySource;
use log::{info, warn};
use std::path::Path;

use crate::animation::AnimationPlayer;
use crate::application::Application;
use crate::components::{
    DirectionalLight, DragComponent, DrawableComponent, MassComponent, NameComponent,
    ParticleEmitterComponent, PerspectiveComponent, PointLight, TransformComponent,
    VelocityComponent,
};
use katla_script::ScriptComponent;

use ron::extensions::Extensions;

/// Current scene format version.
pub const SCENE_VERSION: u32 = 1;

/// RON serialization extensions configuration.
///
/// Uses `extensions` so that comments and trailing commas in scene files
/// are preserved across save/load cycles, and unknown fields are silently
/// ignored for forward compatibility.
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
        Self::save_scene_with_created_at(app, None)
    }

    /// Serialize the current world state into a `Scene` descriptor.
    ///
    /// If `existing_created_at` is provided, it is used as `created_at`
    /// (preserving the original creation time across saves). Otherwise
    /// `created_at` is set to the current time (first save).
    pub fn save_scene_with_created_at(
        app: &Application,
        existing_created_at: Option<String>,
    ) -> Scene {
        let mut scene = Scene::new("Untitled");
        scene.version = SCENE_VERSION;
        let timestamp = {
            use std::time::SystemTime;
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs().to_string())
        };
        scene.created_at = existing_created_at.or_else(|| timestamp.clone());
        scene.modified_at = timestamp;
        scene.engine_version = Some(env!("CARGO_PKG_VERSION").to_string());

        let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (entity_id, transform) in app.world.query_ref::<&TransformComponent>() {
            // Skip editor-hidden entities (camera, etc.)
            if app
                .world
                .get_component::<crate::components::EditorHidden>(entity_id)
                .is_some()
            {
                continue;
            }

            let name = app
                .world
                .get_component::<NameComponent>(entity_id)
                .map(|n| n.name.clone());

            if let Some(ref n) = name
                && !seen_names.insert(n.clone())
            {
                warn!(
                    "Duplicate entity name '{}' -- parent resolution may be incorrect",
                    n
                );
            }

            // Parent relationship (by name lookup)
            let parent = app
                .world
                .get_component::<crate::components::Parent>(entity_id)
                .and_then(|p| {
                    app.world
                        .get_component::<NameComponent>(p.parent)
                        .map(|n| n.name.clone())
                });

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

            let particle_emitter = app
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
                    shape: p.config.get_shape(),
                    shape_params: p.config.shape_params,
                    active: p.active,
                });

            let animation = app
                .world
                .get_component::<AnimationPlayer>(entity_id)
                .map(|a| AnimationDescriptor {
                    current_clip: a.current_clip.clone(),
                    playing: a.playing,
                    loop_animation: a.loop_animation,
                    speed: a.speed,
                    time: a.time,
                    duration: a.duration,
                    blending: a.blending,
                    target_clip: a.target_clip.clone(),
                    blend_weight: a.blend_weight,
                    blend_time: a.blend_time,
                    blend_duration: a.blend_duration,
                    target_time: a.target_time,
                    target_duration: a.target_duration,
                    loop_count: a.loop_count,
                });

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

            let mass = app
                .world
                .get_component::<MassComponent>(entity_id)
                .map(|m| MassDescriptor { mass: m.mass });

            let drag = app
                .world
                .get_component::<DragComponent>(entity_id)
                .map(|d| DragDescriptor {
                    coefficient: d.coefficient,
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
                mass,
                drag,
                perspective,
                directional_light,
            });
        }

        info!(
            "Serialized scene '{}' with {} entities",
            scene.name,
            scene.entities.len()
        );
        scene
    }

    /// Save a scene to a RON file.
    ///
    /// Preserves the original `created_at` from the existing file (if any)
    /// so that the creation timestamp survives across repeated saves.
    pub fn save_to_file(app: &Application, path: &Path) -> Result<(), String> {
        let existing_created_at = std::fs::read_to_string(path)
            .ok()
            .and_then(|content| ron::from_str::<Scene>(&content).ok())
            .and_then(|scene| scene.created_at);

        let scene = Self::save_scene_with_created_at(app, existing_created_at);

        let ron_string = ron::ser::to_string_pretty(&scene, ron_pretty_config())
            .map_err(|e| format!("Failed to serialize scene: {}", e))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {:?}: {}", parent, e))?;
        }

        std::fs::write(path, ron_string)
            .map_err(|e| format!("Failed to write scene to {:?}: {}", path, e))?;

        info!("Saved scene '{}' to {:?}", scene.name, path);
        Ok(())
    }

    /// Load a scene from a RON file and populate the world.
    ///
    /// Clears existing entities and replays each entity descriptor through
    /// the appropriate spawn functions. Runs format migrations if the scene
    /// version is older than [`SCENE_VERSION`].
    pub fn load_from_file(app: &mut Application, path: &Path) -> Result<(), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read scene file {:?}: {}", path, e))?;

        let scene: Scene =
            ron::from_str(&content).map_err(|e| format!("Failed to parse scene file: {}", e))?;

        Self::load_scene(app, scene)
    }

    /// Load a scene descriptor into the world.
    ///
    /// Clears existing entities and replays each entity descriptor through
    /// the appropriate spawn functions. Runs format migrations if the scene
    /// version is older than [`SCENE_VERSION`]. Returns an error if the scene
    /// version is newer than this build supports.
    pub fn load_scene(app: &mut Application, mut scene: Scene) -> Result<(), String> {
        let loaded_version = scene.version;

        // Run migrations before spawning entities
        super::migration::run_migrations(&mut scene)
            .map_err(|e| format!("Cannot load scene '{}': {}", scene.name, e))?;

        info!(
            "Loading scene '{}' (version {}{} with {} entities",
            scene.name,
            loaded_version,
            if loaded_version != scene.version {
                format!(" → migrated to {}", scene.version)
            } else {
                String::new()
            },
            scene.entities.len()
        );

        // Wait for all in-flight GPU work to complete before freeing resources.
        // With FRAMES_IN_FLIGHT=2, the previous frame may still reference
        // these buffers on the GPU.
        app.renderer.wait_for_device();

        // Release all GPU resources before clearing entities.
        // The tracker returns handles whose ref count dropped to zero;
        // the renderer then frees the actual GPU memory.
        let to_destroy = app.gpu_resource_tracker.release_all();
        for handle in &to_destroy.meshes {
            app.renderer.destroy_mesh(*handle);
        }
        for handle in &to_destroy.materials {
            app.renderer.destroy_material(*handle);
        }
        for handle in &to_destroy.textures {
            app.renderer.destroy_texture(*handle);
        }
        for handle in &to_destroy.skeletons {
            app.renderer.destroy_skeleton(*handle);
        }

        info!(
            "Released GPU resources: {} meshes, {} materials, {} textures, {} skeletons",
            to_destroy.meshes.len(),
            to_destroy.materials.len(),
            to_destroy.textures.len(),
            to_destroy.skeletons.len()
        );

        app.world.clear_entities();

        // Re-create the editor camera entity (destroyed by clear_entities).
        // The camera is EditorHidden so it is never saved to disk and must be
        // re-spawned on every scene load.
        app.camera = crate::application::camera::Camera::new(&mut app.world);

        // Build a name -> entity_id mapping for parent resolution
        let mut name_to_entity: std::collections::HashMap<String, katla_ecs::EntityId> =
            std::collections::HashMap::new();

        // First pass: spawn all entities, track by index and name
        let mut spawned_ids: Vec<katla_ecs::EntityId> = Vec::with_capacity(scene.entities.len());
        for desc in &scene.entities {
            let entity_id = Self::spawn_entity(app, desc)?;
            spawned_ids.push(entity_id);
            if let Some(ref name) = desc.name {
                if name_to_entity.contains_key(name) {
                    warn!(
                        "Duplicate entity name '{}' on load -- keeping first occurrence for parent resolution",
                        name
                    );
                } else {
                    name_to_entity.insert(name.clone(), entity_id);
                }
            }
        }

        // Second pass: resolve parent relationships.
        // All entities are already spawned in the first pass, so entity ordering
        // in the scene file does not matter for parent resolution.
        // Uses index-based lookup for children so unnamed entities can have parents.
        for (idx, desc) in scene.entities.iter().enumerate() {
            let child_id = spawned_ids[idx];
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

        info!(
            "Scene '{}' loaded successfully ({} entities)",
            scene.name,
            scene.entities.len()
        );
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
            let mesh_handle = match &desc.source {
                EntitySource::Cube { size } => app.renderer.create_cube_mesh(*size),
                EntitySource::Sphere {
                    radius,
                    segments,
                    rings,
                    ..
                } => app.renderer.create_sphere_mesh(*radius, *segments, *rings),
                EntitySource::Plane { width, height } => {
                    app.renderer.create_plane_mesh(*width, *height)
                }
                EntitySource::Cylinder {
                    height,
                    radius,
                    segments,
                    ..
                } => app
                    .renderer
                    .create_cylinder_mesh(*height, *radius, *segments),
                EntitySource::Torus {
                    radius,
                    tube_radius,
                    segments,
                    tube_segments,
                    ..
                } => {
                    app.renderer
                        .create_torus_mesh(*radius, *tube_radius, *segments, *tube_segments)
                }
                _ => unreachable!(),
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
            emitter.config.set_shape(pe_desc.shape);
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
                if let Some(ref clip) = anim_desc.current_clip {
                    let duration = if anim_desc.duration > 0.0 {
                        anim_desc.duration
                    } else {
                        player.duration
                    };
                    player.set_clip(clip.clone(), duration);
                }
                player.playing = anim_desc.playing;
                player.loop_animation = anim_desc.loop_animation;
                player.speed = anim_desc.speed;
                player.time = anim_desc.time;
                player.duration = anim_desc.duration;
                player.loop_count = anim_desc.loop_count;
                if anim_desc.blending
                    && let Some(ref target) = anim_desc.target_clip
                {
                    player.target_clip = Some(target.clone());
                    player.blending = true;
                    player.blend_weight = anim_desc.blend_weight;
                    player.blend_time = anim_desc.blend_time;
                    player.blend_duration = anim_desc.blend_duration;
                    player.target_time = anim_desc.target_time;
                    player.target_duration = anim_desc.target_duration;
                }
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

        // Apply mass
        if let Some(ref mass_desc) = desc.mass {
            app.world.add_component(
                entity_id,
                MassComponent {
                    mass: mass_desc.mass,
                },
            );
        }

        // Apply drag
        if let Some(ref drag_desc) = desc.drag {
            app.world.add_component(
                entity_id,
                DragComponent {
                    coefficient: drag_desc.coefficient,
                },
            );
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

        // Attach EntitySource for future serialization
        app.world.add_component(entity_id, desc.source.clone());

        // Attach name
        if let Some(ref name) = desc.name {
            app.world
                .add_component(entity_id, NameComponent::new(name.clone()));
        }

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
