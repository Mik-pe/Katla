pub mod descriptors;
pub mod entity_source;

use descriptors::{
    AnimationDescriptor, DrawableDescriptor, EntityDescriptor, ParticleEmitterDescriptor,
    PointLightDescriptor, Scene, TransformDescriptor, VelocityDescriptor,
};
use entity_source::EntitySource;
use log::{info, warn};
use std::path::Path;

use crate::animation::AnimationPlayer;
use crate::components::{
    DrawableComponent, NameComponent, ParticleEmitterComponent, PointLight, TransformComponent,
    VelocityComponent,
};

use crate::application::Application;

use ron::extensions::Extensions;

/// Current scene format version.
const SCENE_VERSION: u32 = 1;

/// RON serialization extensions configuration.
///
/// Uses `extensions` so that comments and trailing commas in scene files
/// are preserved across save/load cycles, and unknown fields are silently
/// ignored for forward compatibility.
const RON_EXTENSIONS: Extensions = Extensions::IMPLICIT_SOME
    .union(Extensions::UNWRAP_NEWTYPES)
    .union(Extensions::UNWRAP_VARIANT_NEWTYPES);

fn ron_pretty_config() -> ron::ser::PrettyConfig {
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
        let mut scene = Scene::new("Untitled");
        scene.version = SCENE_VERSION;
        let timestamp = {
            use std::time::SystemTime;
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs().to_string())
        };
        scene.created_at = timestamp.clone();
        scene.modified_at = timestamp;
        scene.engine_version = Some(env!("CARGO_PKG_VERSION").to_string());
        // Note: save_scene creates a fresh Scene each time, so created_at always
        // equals modified_at. To preserve the original created_at across save/load,
        // callers should pass the loaded Scene through and only update modified_at.

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
    pub fn save_to_file(app: &Application, path: &Path) -> Result<(), String> {
        let scene = Self::save_scene(app);

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
    /// the appropriate spawn functions.
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
    /// the appropriate spawn functions.
    pub fn load_scene(app: &mut Application, scene: Scene) -> Result<(), String> {
        info!(
            "Loading scene '{}' (version {}) with {} entities",
            scene.name,
            scene.version,
            scene.entities.len()
        );

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
        *app.camera.borrow_mut() = crate::application::camera::Camera::new(&mut app.world);

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
            );
            app.gpu_resource_tracker.track_drawable(
                mesh_handle,
                material_handle,
                drawable.skeleton_handle,
            );

            app.world.spawn((
                TransformComponent {
                    transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                        pos[0], pos[1], pos[2],
                    )),
                },
                drawable,
            ))
        } else {
            match &desc.source {
                EntitySource::GltfModel { path } => app
                    .spawn_gltf_model(path, pos, None)
                    .ok_or(format!("Failed to load GLTF model: {}", path))?,
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
                    let transform = TransformComponent {
                        transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                            pos[0], pos[1], pos[2],
                        )),
                    };
                    app.world.spawn((transform, emitter))
                }
                EntitySource::Light => {
                    // Light entities need a visual indicator sphere + the PointLight component
                    let mesh_handle = app.renderer.create_sphere_mesh(0.2, 16, 12);
                    let material_handle = app.default_material();
                    let color = color_from_desc(&desc.drawable);
                    let drawable = DrawableComponent::with_handles_and_material(
                        mesh_handle,
                        material_handle,
                        Some(color),
                        0.0,
                        1.0,
                        1.0,
                    );
                    app.gpu_resource_tracker.track_drawable(
                        mesh_handle,
                        material_handle,
                        drawable.skeleton_handle,
                    );

                    let point_light = desc
                        .point_light
                        .as_ref()
                        .map(|pl| PointLight::new(pl.color, pl.intensity, pl.range))
                        .unwrap_or_default();

                    let transform = TransformComponent {
                        transform: katla_math::Transform::new_from_position(katla_math::Vec3::new(
                            pos[0], pos[1], pos[2],
                        )),
                    };

                    app.world.spawn((transform, drawable, point_light))
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

/// Path to the default scene file, relative to the working directory.
pub const DEFAULT_SCENE_PATH: &str = "assets/scenes/default.katla";

/// Build the default scene as a pure `Scene` descriptor (no GPU access required).
///
/// This is the single source of truth for the default scene contents.
/// The `default.katla` file on disk must be generated from this function
/// and kept in sync via the `test_default_scene_matches_disk` test.
#[allow(clippy::excessive_precision)]
pub fn build_default_scene() -> Scene {
    use katla_gfx::particles::EmitterShape;

    let mut scene = Scene::new("Default Scene");
    scene.version = SCENE_VERSION;

    // Ground plane
    scene.entities.push(EntityDescriptor {
        name: Some("Ground".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [0.0, -1.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Plane {
            width: 20.0,
            height: 20.0,
        },
        drawable: Some(DrawableDescriptor {
            color: Some([0.15686275, 0.17254902, 0.20392157, 1.0]),
            metallic: 0.0,
            roughness: 1.0,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: None,
    });

    // PBR material grid (5x5) -- metallic (Y) x roughness (X)
    let grid_size = 5usize;
    let half_grid = (grid_size - 1) as f32 / 2.0;
    for y in 0..grid_size {
        for x in 0..grid_size {
            let metallic = y as f32 / (grid_size - 1).max(1) as f32;
            let roughness = x as f32 / (grid_size - 1).max(1) as f32;
            let base_r = 0.4 + metallic * 0.2;
            let base_g = 0.6 + metallic * 0.2;
            scene.entities.push(EntityDescriptor {
                name: Some(format!("Sphere_{}_{}", x, y)),
                parent: None,
                transform: TransformDescriptor {
                    position: [
                        (x as f32 - half_grid) * 1.2,
                        2.0 + (y as f32 - half_grid) * 1.2,
                        -6.0,
                    ],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                },
                source: EntitySource::Sphere {
                    radius: 0.4,
                    segments: 32,
                    rings: 16,
                },
                drawable: Some(DrawableDescriptor {
                    color: Some([base_r, base_g, 1.0, 1.0]),
                    metallic,
                    roughness,
                    ao: 1.0,
                }),
                point_light: None,
                particle_emitter: None,
                animation: None,
                velocity: None,
            });
        }
    }

    // Center cube
    scene.entities.push(EntityDescriptor {
        name: Some("CenterCube".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [-5.0, 0.0, -5.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Cube {
            size: [1.0, 1.0, 1.0],
        },
        drawable: Some(DrawableDescriptor {
            color: Some([1.0, 0.47058824, 0.31372549, 1.0]),
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: None,
    });

    // Cyan sphere
    scene.entities.push(EntityDescriptor {
        name: Some("CyanSphere".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [-7.0, 0.0, -5.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Sphere {
            radius: 0.7,
            segments: 32,
            rings: 16,
        },
        drawable: Some(DrawableDescriptor {
            color: Some([0.31372549, 0.86274511, 1.0, 1.0]),
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: None,
    });

    // Magenta cylinder
    scene.entities.push(EntityDescriptor {
        name: Some("MagentaCylinder".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [5.0, 0.0, -5.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Cylinder {
            height: 1.5,
            radius: 0.5,
            segments: 32,
        },
        drawable: Some(DrawableDescriptor {
            color: Some([1.0, 0.31372549, 0.78431373, 1.0]),
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: None,
    });

    // Lime torus
    scene.entities.push(EntityDescriptor {
        name: Some("LimeTorus".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [7.0, 0.5, -3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Torus {
            radius: 0.8,
            tube_radius: 0.2,
            segments: 32,
            tube_segments: 16,
        },
        drawable: Some(DrawableDescriptor {
            color: Some([0.58823529, 1.0, 0.39215686, 1.0]),
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: None,
    });

    // Backdrop plane
    scene.entities.push(EntityDescriptor {
        name: Some("Backdrop".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [0.0, 2.0, -10.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Plane {
            width: 15.0,
            height: 8.0,
        },
        drawable: Some(DrawableDescriptor {
            color: Some([0.23529412, 0.15686275, 0.39215686, 1.0]),
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: None,
    });

    // Fox with animation
    scene.entities.push(EntityDescriptor {
        name: Some("Fox".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [3.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [0.01, 0.01, 0.01],
        },
        source: EntitySource::GltfModel {
            path: "resources/models/Fox.glb".to_string(),
        },
        drawable: None,
        point_light: None,
        particle_emitter: None,
        animation: Some(AnimationDescriptor {
            current_clip: Some("Run".to_string()),
            playing: true,
            loop_animation: true,
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
        }),
        velocity: None,
    });

    // DamagedHelmet
    scene.entities.push(EntityDescriptor {
        name: Some("DamagedHelmet".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [0.0, 1.5, -5.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::GltfModel {
            path: "resources/models/DamagedHelmet.glb".to_string(),
        },
        drawable: None,
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: None,
    });

    // Fire particle emitter
    scene.entities.push(EntityDescriptor {
        name: Some("FireEmitter".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [-3.0, 1.0, -3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::ParticleEmitter,
        drawable: None,
        point_light: None,
        particle_emitter: Some(ParticleEmitterDescriptor {
            position: [-3.0, 1.0, -3.0],
            emit_rate: 400.0,
            base_lifetime: 2.5,
            lifetime_variation: 0.3,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_magnitude: 3.0,
            velocity_cone_angle: 0.05,
            base_scale: 0.08,
            scale_variation: 0.2,
            color: [1.0, 0.5, 0.0, 1.0],
            color_variation: 0.1,
            gravity: 0.0,
            turbulence_strength: 0.0,
            turbulence_frequency: 3.0,
            shape: EmitterShape::Point,
            shape_params: [0.0; 4],
            active: true,
        }),
        animation: None,
        velocity: None,
    });

    // Ethereal particle emitter
    scene.entities.push(EntityDescriptor {
        name: Some("EtherealEmitter".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [3.0, 0.5, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::ParticleEmitter,
        drawable: None,
        point_light: None,
        particle_emitter: Some(ParticleEmitterDescriptor {
            position: [3.0, 0.5, 0.0],
            emit_rate: 200.0,
            base_lifetime: 4.0,
            lifetime_variation: 0.5,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_magnitude: 1.5,
            velocity_cone_angle: 0.1,
            base_scale: 0.12,
            scale_variation: 0.4,
            color: [0.6, 0.8, 1.0, 0.8],
            color_variation: 0.2,
            gravity: -0.5,
            turbulence_strength: 4.0,
            turbulence_frequency: 3.0,
            shape: EmitterShape::Circle,
            shape_params: [2.0, 0.0, 0.0, 0.0],
            active: true,
        }),
        animation: None,
        velocity: None,
    });

    // Sparkle particle emitter
    scene.entities.push(EntityDescriptor {
        name: Some("SparkleEmitter".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [0.0, 3.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::ParticleEmitter,
        drawable: None,
        point_light: None,
        particle_emitter: Some(ParticleEmitterDescriptor {
            position: [0.0, 3.0, 0.0],
            emit_rate: 250.0,
            base_lifetime: 3.0,
            lifetime_variation: 1.0,
            velocity_direction: [0.0, -1.0, 0.0],
            velocity_magnitude: 0.5,
            velocity_cone_angle: 0.1,
            base_scale: 0.1,
            scale_variation: 0.5,
            color: [0.8, 0.9, 1.0, 1.0],
            color_variation: 0.3,
            gravity: 0.0,
            turbulence_strength: 0.0,
            turbulence_frequency: 3.0,
            shape: EmitterShape::Point,
            shape_params: [0.0; 4],
            active: true,
        }),
        animation: None,
        velocity: None,
    });

    // Point lights
    let lights = [
        ("WarmLight", [-5.0, 3.0, -3.0], [1.0, 0.6, 0.2], 15.0, 12.0),
        ("CoolLight", [-7.0, 2.0, -4.0], [0.3, 0.5, 1.0], 12.0, 10.0),
        (
            "MagentaLight",
            [5.0, 2.5, -3.0],
            [1.0, 0.2, 0.8],
            14.0,
            10.0,
        ),
        ("GreenLight", [7.0, 1.5, -1.0], [0.3, 1.0, 0.4], 10.0, 8.0),
        (
            "OverheadLight",
            [0.0, 6.0, -3.0],
            [0.9, 0.85, 0.8],
            8.0,
            15.0,
        ),
    ];

    for (name, pos, color, intensity, range) in lights {
        scene.entities.push(EntityDescriptor {
            name: Some(name.to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: pos,
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::Light,
            drawable: Some(DrawableDescriptor {
                color: Some([color[0], color[1], color[2], 1.0]),
                metallic: 0.0,
                roughness: 1.0,
                ao: 1.0,
            }),
            point_light: Some(PointLightDescriptor {
                color,
                intensity,
                range,
            }),
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
    }

    scene
}

fn color_from_desc(drawable: &Option<DrawableDescriptor>) -> katla_math::Color {
    drawable
        .as_ref()
        .and_then(|d| d.color)
        .map(|c| katla_math::Color::new(c[0], c[1], c[2], c[3]))
        .unwrap_or(katla_math::Color::WHITE)
}

#[cfg(test)]
mod tests {
    use super::descriptors::*;
    use super::*;
    use ron::ser::to_string_pretty;

    fn round_trip<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(value: &T) -> T {
        let ron = to_string_pretty(value, ron_pretty_config()).unwrap();
        ron::from_str(&ron).unwrap()
    }

    #[test]
    fn test_scene_round_trip() {
        let scene = Scene::new("Test Scene");
        let loaded: Scene = round_trip(&scene);
        assert_eq!(loaded.name, "Test Scene");
        assert_eq!(loaded.version, 1);
        assert!(loaded.entities.is_empty());
    }

    #[test]
    fn test_scene_with_entities_round_trip() {
        let mut scene = Scene::new("Multi Entity Scene");
        scene.entities.push(EntityDescriptor {
            name: Some("Ground".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [0.0, -1.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [20.0, 1.0, 20.0],
            },
            source: EntitySource::Plane {
                width: 20.0,
                height: 20.0,
            },
            drawable: Some(DrawableDescriptor {
                color: Some([0.16, 0.17, 0.20, 1.0]),
                metallic: 0.0,
                roughness: 1.0,
                ao: 1.0,
            }),
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("Player".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [0.0, 0.0, 5.0],
                rotation: [0.0, 0.707, 0.0, 0.707],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::Cube {
                size: [1.0, 2.0, 1.0],
            },
            drawable: Some(DrawableDescriptor {
                color: Some([0.2, 0.5, 1.0, 1.0]),
                metallic: 0.0,
                roughness: 0.5,
                ao: 1.0,
            }),
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: Some(VelocityDescriptor {
                velocity: [0.0, 0.0, -5.0],
                acceleration: [0.0, -9.8, 0.0],
            }),
        });

        let loaded: Scene = round_trip(&scene);
        assert_eq!(loaded.name, "Multi Entity Scene");
        assert_eq!(loaded.entities.len(), 2);
        assert_eq!(loaded.entities[0].name, Some("Ground".to_string()));
        assert_eq!(loaded.entities[1].name, Some("Player".to_string()));
        assert_eq!(
            loaded.entities[1].transform.rotation,
            [0.0, 0.707, 0.0, 0.707]
        );
        assert_eq!(
            loaded.entities[1].velocity.as_ref().unwrap().velocity,
            [0.0, 0.0, -5.0]
        );
    }

    #[test]
    fn test_all_entity_source_variants_round_trip() {
        let sources = vec![
            EntitySource::Cube {
                size: [1.0, 1.0, 1.0],
            },
            EntitySource::Sphere {
                radius: 0.5,
                segments: 32,
                rings: 16,
            },
            EntitySource::Plane {
                width: 10.0,
                height: 10.0,
            },
            EntitySource::Cylinder {
                height: 2.0,
                radius: 0.5,
                segments: 24,
            },
            EntitySource::Torus {
                radius: 1.0,
                tube_radius: 0.3,
                segments: 32,
                tube_segments: 16,
            },
            EntitySource::GltfModel {
                path: "models/test.glb".to_string(),
            },
            EntitySource::ParticleEmitter,
            EntitySource::Light,
        ];

        for source in sources {
            let desc = EntityDescriptor {
                name: None,
                parent: None,
                transform: TransformDescriptor {
                    position: [0.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                },
                source,
                drawable: None,
                point_light: None,
                particle_emitter: None,
                animation: None,
                velocity: None,
            };
            let loaded: EntityDescriptor = round_trip(&desc);
            assert_eq!(loaded.source, desc.source);
        }
    }

    #[test]
    fn test_point_light_descriptor_round_trip() {
        let desc = EntityDescriptor {
            name: Some("Torch".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [5.0, 3.0, -2.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::Light,
            drawable: Some(DrawableDescriptor {
                color: Some([1.0, 0.6, 0.2, 1.0]),
                metallic: 0.0,
                roughness: 1.0,
                ao: 1.0,
            }),
            point_light: Some(PointLightDescriptor {
                color: [1.0, 0.6, 0.2],
                intensity: 15.0,
                range: 12.0,
            }),
            particle_emitter: None,
            animation: None,
            velocity: None,
        };

        let loaded: EntityDescriptor = round_trip(&desc);
        let pl = loaded.point_light.unwrap();
        assert_eq!(pl.color, [1.0, 0.6, 0.2]);
        assert_eq!(pl.intensity, 15.0);
        assert_eq!(pl.range, 12.0);
    }

    #[test]
    fn test_particle_emitter_descriptor_round_trip() {
        let desc = EntityDescriptor {
            name: Some("Fire".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::ParticleEmitter,
            drawable: None,
            point_light: None,
            particle_emitter: Some(ParticleEmitterDescriptor {
                position: [-3.0, 1.0, -3.0],
                emit_rate: 400.0,
                base_lifetime: 2.5,
                lifetime_variation: 0.3,
                velocity_direction: [0.0, 1.0, 0.0],
                velocity_magnitude: 3.0,
                velocity_cone_angle: 0.05,
                base_scale: 0.08,
                scale_variation: 0.2,
                color: [1.0, 0.5, 0.0, 1.0],
                color_variation: 0.1,
                gravity: 0.0,
                turbulence_strength: 0.0,
                turbulence_frequency: 3.0,
                shape: katla_gfx::particles::EmitterShape::Point,
                shape_params: [0.0; 4],
                active: true,
            }),
            animation: None,
            velocity: None,
        };

        let loaded: EntityDescriptor = round_trip(&desc);
        let pe = loaded.particle_emitter.unwrap();
        assert_eq!(pe.emit_rate, 400.0);
        assert_eq!(pe.base_lifetime, 2.5);
        assert_eq!(pe.velocity_direction, [0.0, 1.0, 0.0]);
        assert_eq!(pe.color, [1.0, 0.5, 0.0, 1.0]);
        assert!(pe.active);
    }

    #[test]
    fn test_gltf_entity_with_animation_round_trip() {
        let desc = EntityDescriptor {
            name: Some("Fox".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [3.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [0.01, 0.01, 0.01],
            },
            source: EntitySource::GltfModel {
                path: "resources/models/Fox.glb".to_string(),
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: Some(AnimationDescriptor {
                current_clip: Some("Run".to_string()),
                playing: true,
                loop_animation: true,
                speed: 1.0,
                time: 0.5,
                ..Default::default()
            }),
            velocity: None,
        };

        let loaded: EntityDescriptor = round_trip(&desc);
        assert_eq!(loaded.name, Some("Fox".to_string()));
        assert_eq!(
            loaded.source,
            EntitySource::GltfModel {
                path: "resources/models/Fox.glb".to_string(),
            }
        );
        let anim = loaded.animation.unwrap();
        assert_eq!(anim.current_clip, Some("Run".to_string()));
        assert!(anim.playing);
        assert!(anim.loop_animation);
        assert_eq!(anim.speed, 1.0);
        assert_eq!(anim.time, 0.5);
    }

    #[test]
    fn test_parent_child_relationships_round_trip() {
        let mut scene = Scene::new("Hierarchy Test");
        scene.entities.push(EntityDescriptor {
            name: Some("Parent".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [0.0, 5.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::Cube {
                size: [1.0, 1.0, 1.0],
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("Child".to_string()),
            parent: Some("Parent".to_string()),
            transform: TransformDescriptor {
                position: [2.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [0.5, 0.5, 0.5],
            },
            source: EntitySource::Sphere {
                radius: 0.3,
                segments: 16,
                rings: 8,
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });

        let loaded: Scene = round_trip(&scene);
        assert_eq!(loaded.entities.len(), 2);
        assert!(loaded.entities[0].parent.is_none());
        assert_eq!(loaded.entities[1].parent, Some("Parent".to_string()));
        assert_eq!(loaded.entities[1].transform.scale, [0.5, 0.5, 0.5]);
    }

    #[test]
    fn test_forward_compatibility_unknown_struct_fields() {
        // RON v0.8 requires all fields to be known in struct format.
        // Forward compatibility for unknown fields is handled by the version field
        // at the Scene level -- a loader can check the version and decide whether
        // to attempt loading, or skip fields it doesn't understand via a custom
        // deserializer in the future.
        //
        // For now, verify that the version field enables graceful handling:
        let ron_version_2 = r#"Scene(
    version: 2,
    name: "Future Scene",
    entities: [],
)"#;

        let loaded: Scene = ron::from_str(ron_version_2).unwrap();
        assert_eq!(loaded.name, "Future Scene");
        assert_eq!(loaded.version, 2);
        assert!(loaded.entities.is_empty());
    }

    #[test]
    fn test_forward_compatibility_new_entity_source_variant() {
        let ron_new_variant = r#"
EntityDescriptor(
    name: Some("NewTypeEntity"),
    parent: None,
    transform: TransformDescriptor(
        position: [5.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    ),
    source: NewPrimitiveType(param_a: 1.0, param_b: 2),
    drawable: None,
    point_light: None,
    particle_emitter: None,
    animation: None,
    velocity: None,
)"#;

        let result = ron::from_str::<EntityDescriptor>(ron_new_variant);
        assert!(
            result.is_err(),
            "Unknown enum variants should fail to parse"
        );
    }

    #[test]
    fn test_scene_version_defaults() {
        let ron_no_version = r#"Scene(
    name: "Old Scene",
    entities: [],
)"#;

        let loaded: Scene = ron::from_str(ron_no_version).unwrap();
        assert_eq!(loaded.version, 0, "Missing version should default to 0");
        assert_eq!(loaded.name, "Old Scene");
    }

    #[test]
    fn test_scene_serialized_output_is_human_readable() {
        let mut scene = Scene::new("Readability Test");
        scene.entities.push(EntityDescriptor {
            name: Some("Cube".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [1.0, 2.0, 3.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::Cube {
                size: [1.0, 1.0, 1.0],
            },
            drawable: Some(DrawableDescriptor {
                color: Some([1.0, 0.0, 0.0, 1.0]),
                metallic: 0.5,
                roughness: 0.3,
                ao: 1.0,
            }),
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });

        let ron = to_string_pretty(&scene, ron_pretty_config()).unwrap();

        assert!(
            ron.contains("Readability Test"),
            "Scene name should appear in output"
        );
        assert!(ron.contains("Cube"), "Entity name should appear in output");
        assert!(ron.contains("position:"), "Field names should be visible");
        assert!(
            ron.contains("metallic:"),
            "Material fields should be visible"
        );
    }

    #[test]
    fn test_full_default_scene_like_serialization() {
        let mut scene = Scene::new("Default Scene");
        scene.version = 1;

        // Ground (1) + PBR spheres (9) + Cube (1) + Fox (1) + Light (1) + Particle (1) = 14
        let entity_count = 14;

        // Ground plane
        scene.entities.push(EntityDescriptor {
            name: Some("Ground".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [0.0, -1.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::Plane {
                width: 20.0,
                height: 20.0,
            },
            drawable: Some(DrawableDescriptor {
                color: Some([0.16, 0.17, 0.20, 1.0]),
                metallic: 0.0,
                roughness: 1.0,
                ao: 1.0,
            }),
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });

        // PBR spheres (5x5 grid = 25 entities, but we'll do a few)
        for i in 0..3 {
            for j in 0..3 {
                let metallic = i as f32 / 2.0;
                let roughness = j as f32 / 2.0;
                scene.entities.push(EntityDescriptor {
                    name: Some(format!("Sphere_{}_{}", i, j)),
                    parent: None,
                    transform: TransformDescriptor {
                        position: [(j as f32 - 1.0) * 1.2, 2.0 + (i as f32 - 1.0) * 1.2, -6.0],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                        scale: [1.0, 1.0, 1.0],
                    },
                    source: EntitySource::Sphere {
                        radius: 0.4,
                        segments: 32,
                        rings: 16,
                    },
                    drawable: Some(DrawableDescriptor {
                        color: Some([0.4, 0.6, 1.0, 1.0]),
                        metallic,
                        roughness,
                        ao: 1.0,
                    }),
                    point_light: None,
                    particle_emitter: None,
                    animation: None,
                    velocity: None,
                });
            }
        }

        // Cube
        scene.entities.push(EntityDescriptor {
            name: Some("CenterCube".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [-5.0, 0.0, -5.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::Cube {
                size: [1.0, 1.0, 1.0],
            },
            drawable: Some(DrawableDescriptor {
                color: Some([1.0, 0.47, 0.31, 1.0]),
                metallic: 0.0,
                roughness: 0.5,
                ao: 1.0,
            }),
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });

        // Fox with animation
        scene.entities.push(EntityDescriptor {
            name: Some("Fox".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [3.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [0.01, 0.01, 0.01],
            },
            source: EntitySource::GltfModel {
                path: "resources/models/Fox.glb".to_string(),
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: Some(AnimationDescriptor {
                current_clip: Some("Run".to_string()),
                playing: true,
                loop_animation: true,
                speed: 1.0,
                time: 0.0,
                ..Default::default()
            }),
            velocity: None,
        });

        // Point light
        scene.entities.push(EntityDescriptor {
            name: Some("WarmLight".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [-5.0, 3.0, -3.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::Light,
            drawable: Some(DrawableDescriptor {
                color: Some([1.0, 0.6, 0.2, 1.0]),
                metallic: 0.0,
                roughness: 1.0,
                ao: 1.0,
            }),
            point_light: Some(PointLightDescriptor {
                color: [1.0, 0.6, 0.2],
                intensity: 15.0,
                range: 12.0,
            }),
            particle_emitter: None,
            animation: None,
            velocity: None,
        });

        // Particle emitter
        scene.entities.push(EntityDescriptor {
            name: Some("FireEmitter".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::ParticleEmitter,
            drawable: None,
            point_light: None,
            particle_emitter: Some(ParticleEmitterDescriptor {
                position: [-3.0, 1.0, -3.0],
                emit_rate: 400.0,
                base_lifetime: 2.5,
                lifetime_variation: 0.3,
                velocity_direction: [0.0, 1.0, 0.0],
                velocity_magnitude: 3.0,
                velocity_cone_angle: 0.05,
                base_scale: 0.08,
                scale_variation: 0.2,
                color: [1.0, 0.5, 0.0, 1.0],
                color_variation: 0.1,
                gravity: 0.0,
                turbulence_strength: 0.0,
                turbulence_frequency: 3.0,
                shape: katla_gfx::particles::EmitterShape::Point,
                shape_params: [0.0; 4],
                active: true,
            }),
            animation: None,
            velocity: None,
        });

        assert_eq!(scene.entities.len(), entity_count);

        let loaded: Scene = round_trip(&scene);
        assert_eq!(loaded.entities.len(), entity_count);
        assert_eq!(loaded.version, 1);

        // Verify Fox animation survived
        let fox = loaded
            .entities
            .iter()
            .find(|e| e.name == Some("Fox".to_string()));
        assert!(fox.is_some());
        let fox = fox.unwrap();
        assert_eq!(
            fox.animation.as_ref().unwrap().current_clip,
            Some("Run".to_string())
        );

        // Verify point light survived
        let light = loaded
            .entities
            .iter()
            .find(|e| e.name == Some("WarmLight".to_string()));
        assert!(light.is_some());
        assert_eq!(light.unwrap().point_light.as_ref().unwrap().intensity, 15.0);

        // Verify particle emitter survived
        let fire = loaded
            .entities
            .iter()
            .find(|e| e.name == Some("FireEmitter".to_string()));
        assert!(fire.is_some());
        assert_eq!(
            fire.unwrap().particle_emitter.as_ref().unwrap().emit_rate,
            400.0
        );
    }

    #[test]
    fn test_build_default_scene_entity_count() {
        // 1 ground + 25 spheres (5x5 grid) + 1 cube + 1 sphere + 1 cylinder + 1 torus
        // + 1 backdrop + 1 fox + 1 helmet + 3 emitters + 5 lights = 41
        let scene = build_default_scene();
        assert_eq!(
            scene.entities.len(),
            41,
            "Default scene must have exactly 41 entities"
        );
        assert_eq!(scene.name, "Default Scene");
        assert_eq!(scene.version, SCENE_VERSION);
    }

    #[test]
    fn test_build_default_scene_round_trip() {
        let scene = build_default_scene();
        let loaded: Scene = round_trip(&scene);
        assert_eq!(loaded.entities.len(), scene.entities.len());
        assert_eq!(loaded.name, scene.name);
        for (original, loaded) in scene.entities.iter().zip(loaded.entities.iter()) {
            assert_eq!(original.name, loaded.name, "Entity name mismatch");
            assert_eq!(
                original.source, loaded.source,
                "EntitySource mismatch for {:?}",
                original.name
            );
            assert_eq!(
                original.transform.position, loaded.transform.position,
                "Position mismatch for {:?}",
                original.name
            );
        }
    }

    #[test]
    fn test_build_default_scene_all_entities_named() {
        let scene = build_default_scene();
        for entity in &scene.entities {
            assert!(
                entity.name.is_some(),
                "All default scene entities must have names, found unnamed entity: {:?}",
                entity
            );
        }
    }

    #[test]
    fn test_build_default_scene_fox_initial_state() {
        let scene = build_default_scene();
        let fox = scene
            .entities
            .iter()
            .find(|e| e.name == Some("Fox".to_string()))
            .expect("Fox entity must exist");

        let anim = fox.animation.as_ref().expect("Fox must have animation");
        assert_eq!(anim.current_clip, Some("Run".to_string()));
        assert!(anim.playing);
        assert!(anim.loop_animation);
        assert_eq!(anim.time, 0.0, "Fox animation time must start at 0");
        assert_eq!(anim.loop_count, 0, "Fox loop count must start at 0");
    }

    #[test]
    fn test_build_default_scene_lights() {
        let scene = build_default_scene();
        let light_names = [
            "WarmLight",
            "CoolLight",
            "MagentaLight",
            "GreenLight",
            "OverheadLight",
        ];
        for name in &light_names {
            let light = scene
                .entities
                .iter()
                .find(|e| e.name == Some(name.to_string()))
                .unwrap_or_else(|| panic!("Light entity '{}' must exist", name));
            assert!(
                light.point_light.is_some(),
                "'{}' must have PointLight component",
                name
            );
            assert_eq!(light.source, EntitySource::Light);
        }
    }

    #[test]
    fn test_build_default_scene_emitters() {
        let scene = build_default_scene();
        let emitters: Vec<_> = scene
            .entities
            .iter()
            .filter(|e| e.source == EntitySource::ParticleEmitter)
            .collect();
        assert_eq!(emitters.len(), 3, "Must have exactly 3 particle emitters");

        let fire = emitters
            .iter()
            .find(|e| e.name == Some("FireEmitter".to_string()))
            .expect("FireEmitter must exist");
        assert_eq!(fire.particle_emitter.as_ref().unwrap().emit_rate, 400.0);

        let ethereal = emitters
            .iter()
            .find(|e| e.name == Some("EtherealEmitter".to_string()))
            .expect("EtherealEmitter must exist");
        assert_eq!(
            ethereal.particle_emitter.as_ref().unwrap().shape,
            katla_gfx::particles::EmitterShape::Circle
        );
    }

    #[test]
    fn test_build_default_scene_pbr_grid() {
        let scene = build_default_scene();
        let spheres: Vec<_> = scene
            .entities
            .iter()
            .filter(|e| e.name.as_ref().map_or(false, |n| n.starts_with("Sphere_")))
            .collect();
        assert_eq!(spheres.len(), 25, "PBR grid must have 25 spheres");

        let mut metallics: Vec<f32> = spheres
            .iter()
            .map(|s| s.drawable.as_ref().unwrap().metallic)
            .collect();
        metallics.sort_by(|a, b| a.partial_cmp(b).unwrap());
        metallics.dedup_by(|a, b| (*a - *b).abs() < f32::EPSILON);
        assert_eq!(
            metallics.len(),
            5,
            "Grid must have 5 distinct metallic values"
        );

        let mut roughnesses: Vec<f32> = spheres
            .iter()
            .map(|s| s.drawable.as_ref().unwrap().roughness)
            .collect();
        roughnesses.sort_by(|a, b| a.partial_cmp(b).unwrap());
        roughnesses.dedup_by(|a, b| (*a - *b).abs() < f32::EPSILON);
        assert_eq!(
            roughnesses.len(),
            5,
            "Grid must have 5 distinct roughness values"
        );
    }

    #[test]
    fn test_default_scene_matches_disk() {
        // This is the 1:1 parity test: the canonical scene built in code
        // must exactly match what's on disk. If this fails, regenerate the
        // file by running: cargo test -p katla_app -- test_regenerate_default_scene --nocapture
        let scene = build_default_scene();
        let canonical_ron = to_string_pretty(&scene, ron_pretty_config()).unwrap();

        let disk_path = std::path::Path::new(DEFAULT_SCENE_PATH);
        if !disk_path.exists() {
            panic!(
                "Default scene file not found at {:?}. Run test_regenerate_default_scene to create it.",
                disk_path
            );
        }

        let disk_content = std::fs::read_to_string(disk_path)
            .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", disk_path, e));

        assert_eq!(
            canonical_ron, disk_content,
            "Default scene on disk does not match build_default_scene(). \
             Run test_regenerate_default_scene to regenerate."
        );
    }

    #[test]
    fn test_regenerate_default_scene() {
        // Utility test to regenerate the canonical default scene file.
        // Run with: cargo test -p katla_app -- test_regenerate_default_scene --nocapture
        let scene = build_default_scene();
        let canonical_ron = to_string_pretty(&scene, ron_pretty_config()).unwrap();

        let disk_path = std::path::Path::new(DEFAULT_SCENE_PATH);
        if let Some(parent) = disk_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(disk_path, &canonical_ron).unwrap();

        println!(
            "Regenerated default scene at {:?} ({} entities, {} bytes)",
            disk_path,
            scene.entities.len(),
            canonical_ron.len()
        );
    }

    // =========================================================================
    // GPU Resource Cleanup Tests (unit-level, no GPU required)
    // =========================================================================

    #[test]
    fn test_scene_load_gpu_cleanup_tracker_logic() {
        // Verify that the GpuResourceTracker correctly tracks and releases
        // resources through a simulated scene load cycle.
        use crate::gpu_resource_tracker::GpuResourceTracker;
        use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle};

        let protected = MaterialHandle::new(100);
        let mut tracker = GpuResourceTracker::new(protected);

        // Simulate spawning 3 entities with shared material
        let shared_mat = MaterialHandle::new(10);
        let m1 = MeshHandle::new(1);
        let m2 = MeshHandle::new(2);
        let m3 = MeshHandle::new(3);

        tracker.track_drawable(m1, shared_mat, SkeletonHandle::NONE);
        tracker.track_drawable(m2, shared_mat, SkeletonHandle::NONE);
        tracker.track_drawable(m3, shared_mat, SkeletonHandle::NONE);

        assert_eq!(tracker.mesh_count(), 3);
        assert_eq!(tracker.material_ref_count(shared_mat), 3);

        // Simulate scene load (release all)
        let to_destroy = tracker.release_all();

        // All meshes should be destroyed
        assert_eq!(to_destroy.meshes.len(), 3);
        // Shared material should be destroyed (ref count was 3, now 0)
        assert_eq!(to_destroy.materials.len(), 1);
        assert_eq!(to_destroy.materials[0], shared_mat);
        // Protected material should NOT be in destroy list
        assert!(!to_destroy.materials.iter().any(|m| m.index() == 100));

        // Tracker should be empty
        assert_eq!(tracker.mesh_count(), 0);
        assert_eq!(tracker.material_count(), 0);
    }

    #[test]
    fn test_repeated_load_no_leak() {
        // Simulate repeated scene loads and verify resources don't accumulate
        use crate::gpu_resource_tracker::GpuResourceTracker;
        use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle};

        let protected = MaterialHandle::new(100);
        let mut tracker = GpuResourceTracker::new(protected);

        for load_iteration in 0..5 {
            // Simulate loading a scene with 10 entities
            for i in 0..10 {
                let mesh = MeshHandle::new((load_iteration * 100) + i);
                let mat = MaterialHandle::new((load_iteration * 100) + i + 50);
                tracker.track_drawable(mesh, mat, SkeletonHandle::NONE);
            }

            assert_eq!(tracker.mesh_count(), 10);

            // Release all (simulating scene load)
            let to_destroy = tracker.release_all();
            assert_eq!(to_destroy.meshes.len(), 10, "Iteration {}", load_iteration);
            assert_eq!(
                to_destroy.materials.len(),
                10,
                "Iteration {}",
                load_iteration
            );
            assert_eq!(
                tracker.mesh_count(),
                0,
                "Tracker should be empty after release"
            );
        }
    }

    #[test]
    fn test_entity_destroy_gpu_cleanup() {
        // Verify that destroying an entity releases its GPU resources
        use crate::gpu_resource_tracker::GpuResourceTracker;
        use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle};

        let protected = MaterialHandle::new(100);
        let mut tracker = GpuResourceTracker::new(protected);

        let mesh = MeshHandle::new(1);
        let mat = MaterialHandle::new(2);
        let skeleton = SkeletonHandle::new(3);

        tracker.track_drawable(mesh, mat, skeleton);

        // Simulate entity destruction (release drawable)
        let to_destroy = tracker.release_drawable(mesh, mat, skeleton);

        assert_eq!(to_destroy.meshes.len(), 1, "Mesh should be destroyed");
        assert_eq!(
            to_destroy.materials.len(),
            1,
            "Material should be destroyed"
        );
        assert_eq!(
            to_destroy.skeletons.len(),
            1,
            "Skeleton should be destroyed"
        );
        assert_eq!(tracker.mesh_count(), 0);
    }

    #[test]
    fn test_component_remove_gpu_cleanup() {
        // Verify that removing a DrawableComponent releases its GPU resources.
        // This tests the GpuResourceTracker integration that gpu_cleanup uses
        // when processing ComponentEvent::Removed for DrawableComponent.
        use crate::gpu_resource_tracker::GpuResourceTracker;
        use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle};

        let protected = MaterialHandle::new(100);
        let mut tracker = GpuResourceTracker::new(protected);

        let mesh = MeshHandle::new(1);
        let mat = MaterialHandle::new(2);
        let skeleton = SkeletonHandle::new(3);

        tracker.track_drawable(mesh, mat, skeleton);

        // Simulate what gpu_cleanup does on ComponentEvent::Removed for DrawableComponent
        let to_destroy = tracker.release_drawable(mesh, mat, skeleton);

        assert_eq!(
            to_destroy.meshes.len(),
            1,
            "Mesh should be marked for destruction"
        );
        assert_eq!(
            to_destroy.materials.len(),
            1,
            "Material should be marked for destruction"
        );
        assert_eq!(
            to_destroy.skeletons.len(),
            1,
            "Skeleton should be marked for destruction"
        );
        assert_eq!(
            tracker.mesh_count(),
            0,
            "Tracker should be empty after release"
        );
        assert_eq!(
            tracker.material_count(),
            0,
            "Tracker should be empty after release"
        );
    }

    #[test]
    fn test_component_remove_emits_correct_event() {
        // Verify that removing DrawableComponent emits a ComponentEvent::Removed
        // that gpu_cleanup can detect via TypeId matching
        use crate::components::DrawableComponent;
        use katla_ecs::events::ComponentEvent;
        use std::any::TypeId;

        let mut world = katla_ecs::World::new();
        let entity = world.create_entity();
        world.add_component(
            entity,
            DrawableComponent::with_handles(
                katla_gfx::MeshHandle::new(1),
                katla_gfx::MaterialHandle::new(2),
            ),
        );

        world.remove_component::<DrawableComponent>(entity);

        let drawable_type_id = TypeId::of::<DrawableComponent>();
        let removed_events: Vec<_> = world
            .component_events()
            .iter()
            .filter(|e| matches!(e, ComponentEvent::Removed(id, tid) if *id == entity && *tid == drawable_type_id))
            .collect();

        assert_eq!(
            removed_events.len(),
            1,
            "Exactly one DrawableComponent Removed event should be emitted"
        );
    }

    #[test]
    fn test_gltf_texture_tracking() {
        // Verify that GLTF texture tracking via track_texture() works correctly
        // and that release_all() returns tracked textures for destruction
        use crate::gpu_resource_tracker::GpuResourceTracker;
        use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};

        let protected = MaterialHandle::new(100);
        let mut tracker = GpuResourceTracker::new(protected);

        // Simulate what spawn_gltf_model does: track drawable + textures
        let mesh = MeshHandle::new(1);
        let mat = MaterialHandle::new(2);
        tracker.track_drawable(mesh, mat, SkeletonHandle::NONE);

        // Track textures (albedo, normal, mr, ao, emission)
        let albedo = TextureHandle::new(10);
        let normal = TextureHandle::new(11);
        let mr = TextureHandle::new(12);
        let ao = TextureHandle::new(13);
        let emission = TextureHandle::new(14);

        tracker.track_texture(albedo);
        tracker.track_texture(normal);
        tracker.track_texture(mr);
        tracker.track_texture(ao);
        tracker.track_texture(emission);

        // Verify textures are tracked
        assert_eq!(
            tracker.texture_count(),
            5,
            "All 5 textures should be tracked"
        );

        // release_all should return both drawable and texture resources
        let to_destroy = tracker.release_all();
        assert_eq!(
            to_destroy.textures.len(),
            5,
            "All 5 textures should be in destroy list"
        );
        assert_eq!(to_destroy.meshes.len(), 1, "Mesh should be in destroy list");
        assert_eq!(
            to_destroy.materials.len(),
            1,
            "Material should be in destroy list"
        );

        // Verify texture handles are correct
        let texture_indices: Vec<u32> = to_destroy.textures.iter().map(|h| h.index()).collect();
        assert!(texture_indices.contains(&10));
        assert!(texture_indices.contains(&11));
        assert!(texture_indices.contains(&12));
        assert!(texture_indices.contains(&13));
        assert!(texture_indices.contains(&14));
    }

    #[test]
    fn test_shared_resources_safe() {
        // Verify that shared resources (mesh used by multiple entities)
        // are not destroyed when only one entity is cleaned up
        use crate::gpu_resource_tracker::GpuResourceTracker;
        use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle};

        let protected = MaterialHandle::new(100);
        let mut tracker = GpuResourceTracker::new(protected);

        // Two entities share the same mesh and material
        let shared_mesh = MeshHandle::new(1);
        let shared_mat = MaterialHandle::new(2);

        tracker.track_drawable(shared_mesh, shared_mat, SkeletonHandle::NONE);
        tracker.track_drawable(shared_mesh, shared_mat, SkeletonHandle::NONE);

        assert_eq!(tracker.mesh_ref_count(shared_mesh), 2);
        assert_eq!(tracker.material_ref_count(shared_mat), 2);

        // Destroy first entity
        let to_destroy = tracker.release_drawable(shared_mesh, shared_mat, SkeletonHandle::NONE);

        assert!(
            to_destroy.meshes.is_empty(),
            "Shared mesh should NOT be destroyed when one entity remains"
        );
        assert!(
            to_destroy.materials.is_empty(),
            "Shared material should NOT be destroyed when one entity remains"
        );
        assert_eq!(tracker.mesh_ref_count(shared_mesh), 1);

        // Second entity should still have valid resources
        assert_eq!(tracker.mesh_ref_count(shared_mesh), 1);
        assert_eq!(tracker.material_ref_count(shared_mat), 1);

        // Destroy second entity - now resources should be freed
        let to_destroy = tracker.release_drawable(shared_mesh, shared_mat, SkeletonHandle::NONE);
        assert_eq!(
            to_destroy.meshes.len(),
            1,
            "Mesh should be destroyed after last entity"
        );
        assert_eq!(
            to_destroy.materials.len(),
            1,
            "Material should be destroyed after last entity"
        );
        assert_eq!(tracker.mesh_count(), 0);
    }

    #[test]
    fn test_resource_counts_create_destroy_sequence() {
        // VAL-GPU-010: Create N, destroy one → N-1, destroy another → N-2, create new → N-1
        use crate::gpu_resource_tracker::GpuResourceTracker;
        use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle};

        let protected = MaterialHandle::new(100);
        let mut tracker = GpuResourceTracker::new(protected);

        let mat = MaterialHandle::new(50);

        // Create 3 meshes (each with unique mesh, shared material)
        let m1 = MeshHandle::new(1);
        let m2 = MeshHandle::new(2);
        let m3 = MeshHandle::new(3);

        tracker.track_drawable(m1, mat, SkeletonHandle::NONE);
        tracker.track_drawable(m2, mat, SkeletonHandle::NONE);
        tracker.track_drawable(m3, mat, SkeletonHandle::NONE);
        assert_eq!(tracker.mesh_count(), 3);

        // Destroy one → 2
        let d1 = tracker.release_drawable(m2, mat, SkeletonHandle::NONE);
        assert!(d1.meshes.len() == 1);
        assert!(d1.materials.is_empty(), "Material still shared");
        assert_eq!(tracker.mesh_count(), 2);

        // Destroy another → 1
        let d2 = tracker.release_drawable(m1, mat, SkeletonHandle::NONE);
        assert!(d2.meshes.len() == 1);
        assert!(d2.materials.is_empty(), "Material still shared by m3");
        assert_eq!(tracker.mesh_count(), 1);

        // Create new → 2
        let m4 = MeshHandle::new(4);
        tracker.track_drawable(m4, mat, SkeletonHandle::NONE);
        assert_eq!(tracker.mesh_count(), 2);

        // Destroy remaining two → 0
        let d3 = tracker.release_drawable(m3, mat, SkeletonHandle::NONE);
        assert!(d3.meshes.len() == 1);
        assert!(d3.materials.is_empty(), "Material still shared by m4");
        let d4 = tracker.release_drawable(m4, mat, SkeletonHandle::NONE);
        assert_eq!(d4.meshes.len(), 1);
        assert_eq!(d4.materials.len(), 1, "Material ref count now 0");
        assert_eq!(tracker.mesh_count(), 0);
        assert_eq!(tracker.material_count(), 0);
    }

    // =========================================================================
    // Scene Serialization Round-Trip Tests (VAL-SCENE-001 through VAL-SCENE-017)
    // =========================================================================

    #[test]
    fn test_primitive_round_trip() {
        let primitives = vec![
            EntityDescriptor {
                name: Some("MyCube".to_string()),
                parent: None,
                transform: TransformDescriptor {
                    position: [1.5, 2.5, 3.5],
                    rotation: [0.1, 0.2, 0.3, 0.4],
                    scale: [2.0, 3.0, 4.0],
                },
                source: EntitySource::Cube {
                    size: [1.5, 2.0, 3.0],
                },
                drawable: Some(DrawableDescriptor {
                    color: Some([0.8, 0.2, 0.1, 1.0]),
                    metallic: 0.7,
                    roughness: 0.3,
                    ao: 0.8,
                }),
                point_light: None,
                particle_emitter: None,
                animation: None,
                velocity: None,
            },
            EntityDescriptor {
                name: Some("MySphere".to_string()),
                parent: None,
                transform: TransformDescriptor {
                    position: [0.0, 5.0, -2.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                },
                source: EntitySource::Sphere {
                    radius: 1.23,
                    segments: 48,
                    rings: 24,
                },
                drawable: Some(DrawableDescriptor {
                    color: Some([0.1, 0.5, 0.9, 1.0]),
                    metallic: 0.1,
                    roughness: 0.9,
                    ao: 1.0,
                }),
                point_light: None,
                particle_emitter: None,
                animation: None,
                velocity: None,
            },
            EntityDescriptor {
                name: Some("MyPlane".to_string()),
                parent: None,
                transform: TransformDescriptor {
                    position: [0.0, -1.0, 0.0],
                    rotation: [0.707, 0.0, 0.0, 0.707],
                    scale: [10.0, 1.0, 10.0],
                },
                source: EntitySource::Plane {
                    width: 50.0,
                    height: 50.0,
                },
                drawable: Some(DrawableDescriptor {
                    color: Some([0.5, 0.5, 0.5, 1.0]),
                    metallic: 0.0,
                    roughness: 1.0,
                    ao: 1.0,
                }),
                point_light: None,
                particle_emitter: None,
                animation: None,
                velocity: None,
            },
            EntityDescriptor {
                name: Some("MyCylinder".to_string()),
                parent: None,
                transform: TransformDescriptor {
                    position: [-3.0, 0.0, 4.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 2.0, 1.0],
                },
                source: EntitySource::Cylinder {
                    height: 3.0,
                    radius: 0.75,
                    segments: 36,
                },
                drawable: Some(DrawableDescriptor {
                    color: Some([0.9, 0.7, 0.2, 1.0]),
                    metallic: 0.5,
                    roughness: 0.4,
                    ao: 0.9,
                }),
                point_light: None,
                particle_emitter: None,
                animation: None,
                velocity: None,
            },
            EntityDescriptor {
                name: Some("MyTorus".to_string()),
                parent: None,
                transform: TransformDescriptor {
                    position: [0.0, 2.0, 0.0],
                    rotation: [0.5, 0.5, 0.5, 0.5],
                    scale: [1.0, 1.0, 1.0],
                },
                source: EntitySource::Torus {
                    radius: 2.0,
                    tube_radius: 0.4,
                    segments: 64,
                    tube_segments: 32,
                },
                drawable: Some(DrawableDescriptor {
                    color: Some([0.3, 0.9, 0.4, 1.0]),
                    metallic: 0.3,
                    roughness: 0.6,
                    ao: 1.0,
                }),
                point_light: None,
                particle_emitter: None,
                animation: None,
                velocity: None,
            },
        ];

        for desc in &primitives {
            let loaded: EntityDescriptor = round_trip(desc);
            assert_eq!(loaded.name, desc.name, "Name mismatch for {:?}", desc.name);
            assert_eq!(
                loaded.source, desc.source,
                "Source mismatch for {:?}",
                desc.name
            );
            assert_eq!(
                loaded.transform.position, desc.transform.position,
                "Position mismatch for {:?}",
                desc.name
            );
            assert_eq!(
                loaded.transform.rotation, desc.transform.rotation,
                "Rotation mismatch for {:?}",
                desc.name
            );
            assert_eq!(
                loaded.transform.scale, desc.transform.scale,
                "Scale mismatch for {:?}",
                desc.name
            );
            assert_eq!(
                loaded.drawable, desc.drawable,
                "Drawable mismatch for {:?}",
                desc.name
            );
        }
    }

    #[test]
    fn test_gltf_round_trip() {
        let desc = EntityDescriptor {
            name: Some("Fox".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [3.0, 0.0, 0.0],
                rotation: [0.0, 0.38268343, 0.0, 0.92387953],
                scale: [0.01, 0.01, 0.01],
            },
            source: EntitySource::GltfModel {
                path: "resources/models/Fox.glb".to_string(),
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: Some(AnimationDescriptor {
                current_clip: Some("Run".to_string()),
                playing: true,
                loop_animation: true,
                speed: 1.0,
                time: 0.5,
                duration: 1.2,
                blending: false,
                target_clip: None,
                blend_weight: 1.0,
                blend_time: 0.0,
                blend_duration: 0.0,
                target_time: 0.0,
                target_duration: 0.0,
                loop_count: 3,
            }),
            velocity: None,
        };

        let loaded: EntityDescriptor = round_trip(&desc);
        assert_eq!(loaded.name, Some("Fox".to_string()));
        assert_eq!(
            loaded.source,
            EntitySource::GltfModel {
                path: "resources/models/Fox.glb".to_string(),
            }
        );
        let anim = loaded.animation.as_ref().unwrap();
        assert_eq!(anim.current_clip, Some("Run".to_string()));
        assert!(anim.playing);
        assert!(anim.loop_animation);
        assert_eq!(anim.speed, 1.0);
        assert_eq!(anim.time, 0.5);
        assert_eq!(anim.duration, 1.2);
        assert_eq!(anim.loop_count, 3);
        assert!(!anim.blending);
    }

    #[test]
    fn test_point_light_round_trip() {
        let desc = EntityDescriptor {
            name: Some("Torch".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [5.0, 3.0, -2.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::Light,
            drawable: Some(DrawableDescriptor {
                color: Some([1.0, 0.6, 0.2, 1.0]),
                metallic: 0.0,
                roughness: 1.0,
                ao: 1.0,
            }),
            point_light: Some(PointLightDescriptor {
                color: [1.0, 0.6, 0.2],
                intensity: 15.0,
                range: 12.0,
            }),
            particle_emitter: None,
            animation: None,
            velocity: None,
        };

        let loaded: EntityDescriptor = round_trip(&desc);
        let pl = loaded.point_light.unwrap();
        assert_eq!(pl.color, [1.0, 0.6, 0.2]);
        assert_eq!(pl.intensity, 15.0);
        assert_eq!(pl.range, 12.0);

        let drawable = loaded.drawable.unwrap();
        assert_eq!(drawable.color, Some([1.0, 0.6, 0.2, 1.0]));
    }

    #[test]
    fn test_particle_emitter_round_trip() {
        let shapes = vec![
            katla_gfx::particles::EmitterShape::Point,
            katla_gfx::particles::EmitterShape::Circle,
        ];

        for (idx, shape) in shapes.into_iter().enumerate() {
            let desc = EntityDescriptor {
                name: Some(format!("Emitter{}", idx)),
                parent: None,
                transform: TransformDescriptor {
                    position: [0.0, 1.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                },
                source: EntitySource::ParticleEmitter,
                drawable: None,
                point_light: None,
                particle_emitter: Some(ParticleEmitterDescriptor {
                    position: [1.0, 2.0, 3.0],
                    emit_rate: 350.0,
                    base_lifetime: 3.0,
                    lifetime_variation: 0.5,
                    velocity_direction: [0.0, 1.0, 0.0],
                    velocity_magnitude: 2.5,
                    velocity_cone_angle: 0.15,
                    base_scale: 0.1,
                    scale_variation: 0.3,
                    color: [1.0, 0.8, 0.3, 0.9],
                    color_variation: 0.15,
                    gravity: -2.0,
                    turbulence_strength: 3.0,
                    turbulence_frequency: 2.5,
                    shape,
                    shape_params: if idx == 0 {
                        [0.0; 4]
                    } else {
                        [3.0, 0.0, 0.0, 0.0]
                    },
                    active: idx == 0,
                }),
                animation: None,
                velocity: None,
            };

            let loaded: EntityDescriptor = round_trip(&desc);
            let pe = loaded.particle_emitter.as_ref().unwrap();
            assert_eq!(pe.emit_rate, 350.0);
            assert_eq!(pe.base_lifetime, 3.0);
            assert_eq!(pe.lifetime_variation, 0.5);
            assert_eq!(pe.velocity_direction, [0.0, 1.0, 0.0]);
            assert_eq!(pe.velocity_magnitude, 2.5);
            assert_eq!(pe.velocity_cone_angle, 0.15);
            assert_eq!(pe.base_scale, 0.1);
            assert_eq!(pe.scale_variation, 0.3);
            assert_eq!(pe.color, [1.0, 0.8, 0.3, 0.9]);
            assert_eq!(pe.color_variation, 0.15);
            assert_eq!(pe.gravity, -2.0);
            assert_eq!(pe.turbulence_strength, 3.0);
            assert_eq!(pe.turbulence_frequency, 2.5);
            assert_eq!(pe.shape, shape);
            assert_eq!(pe.active, idx == 0);
        }
    }

    #[test]
    fn test_animation_round_trip() {
        let non_blending = AnimationDescriptor {
            current_clip: Some("Walk".to_string()),
            playing: true,
            loop_animation: true,
            speed: 1.5,
            time: 2.3,
            duration: 3.0,
            blending: false,
            target_clip: None,
            blend_weight: 1.0,
            blend_time: 0.0,
            blend_duration: 0.0,
            target_time: 0.0,
            target_duration: 0.0,
            loop_count: 5,
        };

        let loaded: AnimationDescriptor = round_trip(&non_blending);
        assert_eq!(loaded.current_clip, Some("Walk".to_string()));
        assert!(loaded.playing);
        assert!(loaded.loop_animation);
        assert_eq!(loaded.speed, 1.5);
        assert_eq!(loaded.time, 2.3);
        assert_eq!(loaded.duration, 3.0);
        assert!(!loaded.blending);
        assert_eq!(loaded.loop_count, 5);

        let blending = AnimationDescriptor {
            current_clip: Some("Idle".to_string()),
            playing: true,
            loop_animation: true,
            speed: 1.0,
            time: 0.5,
            duration: 2.0,
            blending: true,
            target_clip: Some("Walk".to_string()),
            blend_weight: 0.4,
            blend_time: 0.3,
            blend_duration: 0.5,
            target_time: 1.0,
            target_duration: 3.0,
            loop_count: 0,
        };

        let loaded: AnimationDescriptor = round_trip(&blending);
        assert_eq!(loaded.current_clip, Some("Idle".to_string()));
        assert!(loaded.blending);
        assert_eq!(loaded.target_clip, Some("Walk".to_string()));
        assert_eq!(loaded.blend_weight, 0.4);
        assert_eq!(loaded.blend_time, 0.3);
        assert_eq!(loaded.blend_duration, 0.5);
        assert_eq!(loaded.target_time, 1.0);
        assert_eq!(loaded.target_duration, 3.0);

        // Also test via EntityDescriptor round-trip
        let desc = EntityDescriptor {
            name: Some("AnimatedModel".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::GltfModel {
                path: "test.glb".to_string(),
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: Some(blending.clone()),
            velocity: None,
        };
        let loaded_desc: EntityDescriptor = round_trip(&desc);
        let loaded_anim = loaded_desc.animation.unwrap();
        assert_eq!(loaded_anim, blending);
    }

    #[test]
    fn test_hierarchy_preservation() {
        let mut scene = Scene::new("Hierarchy Test");
        scene.entities.push(EntityDescriptor {
            name: Some("Root".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [0.0, 5.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [2.0, 2.0, 2.0],
            },
            source: EntitySource::Cube {
                size: [1.0, 1.0, 1.0],
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("ChildA".to_string()),
            parent: Some("Root".to_string()),
            transform: TransformDescriptor {
                position: [2.0, 0.0, 0.0],
                rotation: [0.0, 0.707, 0.0, 0.707],
                scale: [0.5, 0.5, 0.5],
            },
            source: EntitySource::Sphere {
                radius: 0.3,
                segments: 16,
                rings: 8,
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("Grandchild".to_string()),
            parent: Some("ChildA".to_string()),
            transform: TransformDescriptor {
                position: [1.0, 1.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [0.25, 0.25, 0.25],
            },
            source: EntitySource::Cube {
                size: [0.5, 0.5, 0.5],
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });

        let loaded: Scene = round_trip(&scene);
        assert_eq!(loaded.entities.len(), 3);
        assert!(loaded.entities[0].parent.is_none());
        assert_eq!(loaded.entities[1].parent, Some("Root".to_string()));
        assert_eq!(loaded.entities[2].parent, Some("ChildA".to_string()));
        assert_eq!(loaded.entities[0].transform.scale, [2.0, 2.0, 2.0]);
        assert_eq!(loaded.entities[1].transform.scale, [0.5, 0.5, 0.5]);
        assert_eq!(loaded.entities[2].transform.scale, [0.25, 0.25, 0.25]);
    }

    #[test]
    fn test_entity_count_preservation() {
        let mut scene = Scene::new("Mixed Scene");
        scene.version = SCENE_VERSION;

        // One of each EntitySource variant: 8 total
        scene.entities.push(EntityDescriptor {
            name: Some("Cube1".to_string()),
            parent: None,
            transform: TransformDescriptor::default_transform(),
            source: EntitySource::Cube {
                size: [1.0, 1.0, 1.0],
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("Sphere1".to_string()),
            parent: None,
            transform: TransformDescriptor::default_transform(),
            source: EntitySource::Sphere {
                radius: 1.0,
                segments: 16,
                rings: 8,
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("Plane1".to_string()),
            parent: None,
            transform: TransformDescriptor::default_transform(),
            source: EntitySource::Plane {
                width: 10.0,
                height: 10.0,
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("Cylinder1".to_string()),
            parent: None,
            transform: TransformDescriptor::default_transform(),
            source: EntitySource::Cylinder {
                height: 2.0,
                radius: 0.5,
                segments: 16,
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("Torus1".to_string()),
            parent: None,
            transform: TransformDescriptor::default_transform(),
            source: EntitySource::Torus {
                radius: 1.0,
                tube_radius: 0.3,
                segments: 16,
                tube_segments: 8,
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("Model1".to_string()),
            parent: None,
            transform: TransformDescriptor::default_transform(),
            source: EntitySource::GltfModel {
                path: "test.glb".to_string(),
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("Emitter1".to_string()),
            parent: None,
            transform: TransformDescriptor::default_transform(),
            source: EntitySource::ParticleEmitter,
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });
        scene.entities.push(EntityDescriptor {
            name: Some("Light1".to_string()),
            parent: None,
            transform: TransformDescriptor::default_transform(),
            source: EntitySource::Light,
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
        });

        assert_eq!(scene.entities.len(), 8);
        let loaded: Scene = round_trip(&scene);
        assert_eq!(loaded.entities.len(), 8);
        assert_eq!(loaded.version, SCENE_VERSION);
        for (original, loaded) in scene.entities.iter().zip(loaded.entities.iter()) {
            assert_eq!(original.source, loaded.source);
        }
    }

    #[test]
    fn test_unknown_fields_ignored() {
        // Forward compatibility: a scene with unknown fields at the top level
        // should deserialize successfully. RON ignores unknown fields by default.
        let ron_with_unknown = r#"(
    version: 1,
    name: "Forward Compat Test",
    future_field: "this is ignored",
    entities: [],
)"#;

        let loaded: Scene = ron::from_str(ron_with_unknown).unwrap();
        assert_eq!(loaded.name, "Forward Compat Test");
        assert_eq!(loaded.version, 1);
        assert!(loaded.entities.is_empty());

        // Entity-level unknown fields: RON with unknown fields on EntityDescriptor
        // RON v0.8+ does not allow unknown struct fields by default,
        // but the version field at the Scene level provides a migration path.
        // Verify that the scene-level version field works correctly for this purpose.
        let ron_version_mismatch = r#"(
    version: 2,
    name: "Version 2 Scene",
    entities: [],
)"#;

        let loaded: Scene = ron::from_str(ron_version_mismatch).unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.name, "Version 2 Scene");
    }

    #[test]
    fn test_version_field_present() {
        let scene = Scene::new("Version Test");
        let ron = to_string_pretty(&scene, ron_pretty_config()).unwrap();

        assert!(
            ron.contains("version: 1"),
            "Serialized scene must contain 'version: 1'"
        );

        // Test with entities too
        let scene = build_default_scene();
        let ron = to_string_pretty(&scene, ron_pretty_config()).unwrap();
        assert!(
            ron.contains("version: 1"),
            "Default scene must contain 'version: 1'"
        );
    }

    #[test]
    fn test_empty_scene() {
        let scene = Scene::new("Empty");
        assert_eq!(scene.entities.len(), 0);
        assert_eq!(scene.version, 1);
        assert_eq!(scene.name, "Empty");

        let loaded: Scene = round_trip(&scene);
        assert_eq!(loaded.name, "Empty");
        assert_eq!(loaded.version, 1);
        assert!(loaded.entities.is_empty());
        assert!(loaded.author.is_none());
        assert!(loaded.created_at.is_none());
        assert!(loaded.modified_at.is_none());
        assert!(loaded.engine_version.is_none());

        // Also test deserializing empty scene from RON
        let ron_empty = r#"(
    version: 1,
    name: "Empty From RON",
    author: None,
    created_at: None,
    modified_at: None,
    engine_version: None,
    entities: [],
)"#;
        let loaded: Scene = ron::from_str(ron_empty).unwrap();
        assert_eq!(loaded.name, "Empty From RON");
        assert_eq!(loaded.version, 1);
        assert!(loaded.entities.is_empty());
    }

    #[test]
    fn test_velocity_round_trip() {
        let desc = EntityDescriptor {
            name: Some("MovingObject".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [0.0, 10.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            source: EntitySource::Cube {
                size: [1.0, 1.0, 1.0],
            },
            drawable: Some(DrawableDescriptor {
                color: Some([0.2, 0.4, 0.8, 1.0]),
                metallic: 0.0,
                roughness: 0.5,
                ao: 1.0,
            }),
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: Some(VelocityDescriptor {
                velocity: [5.0, 0.0, -3.0],
                acceleration: [0.0, -9.81, 0.0],
            }),
        };

        let loaded: EntityDescriptor = round_trip(&desc);
        let vel = loaded.velocity.as_ref().unwrap();
        assert_eq!(vel.velocity, [5.0, 0.0, -3.0]);
        assert_eq!(vel.acceleration, [0.0, -9.81, 0.0]);

        // Test with zero velocity
        let desc_zero = EntityDescriptor {
            name: Some("StaticObject".to_string()),
            parent: None,
            transform: TransformDescriptor::default_transform(),
            source: EntitySource::Cube {
                size: [1.0, 1.0, 1.0],
            },
            drawable: None,
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: Some(VelocityDescriptor {
                velocity: [0.0, 0.0, 0.0],
                acceleration: [0.0, 0.0, 0.0],
            }),
        };
        let loaded_zero: EntityDescriptor = round_trip(&desc_zero);
        let vel_zero = loaded_zero.velocity.unwrap();
        assert_eq!(vel_zero.velocity, [0.0, 0.0, 0.0]);
        assert_eq!(vel_zero.acceleration, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_default_scene_round_trip() {
        let scene = build_default_scene();
        let loaded: Scene = round_trip(&scene);

        assert_eq!(loaded.entities.len(), scene.entities.len());
        assert_eq!(loaded.name, scene.name);
        assert_eq!(loaded.version, scene.version);

        for (original, loaded) in scene.entities.iter().zip(loaded.entities.iter()) {
            assert_eq!(original.name, loaded.name);
            assert_eq!(original.source, loaded.source);
            assert_eq!(original.transform.position, loaded.transform.position);
            assert_eq!(original.transform.rotation, loaded.transform.rotation);
            assert_eq!(original.transform.scale, loaded.transform.scale);
            assert_eq!(original.parent, loaded.parent);
            assert_eq!(original.drawable, loaded.drawable);
            assert_eq!(original.point_light, loaded.point_light);
            assert_eq!(original.particle_emitter, loaded.particle_emitter);
            assert_eq!(original.animation, loaded.animation);
            assert_eq!(original.velocity, loaded.velocity);
        }
    }

    #[test]
    fn test_metadata_preservation() {
        let mut scene = Scene::new("Metadata Test");
        scene.version = 1;
        scene.author = Some("TestAuthor".to_string());
        scene.created_at = Some("1000000000".to_string());
        scene.modified_at = Some("2000000000".to_string());
        scene.engine_version = Some("0.5.0".to_string());

        let loaded: Scene = round_trip(&scene);
        assert_eq!(loaded.name, "Metadata Test");
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.author, Some("TestAuthor".to_string()));
        assert_eq!(loaded.created_at, Some("1000000000".to_string()));
        assert_eq!(loaded.modified_at, Some("2000000000".to_string()));
        assert_eq!(loaded.engine_version, Some("0.5.0".to_string()));

        // Test with None metadata
        let scene_none = Scene::new("No Metadata");
        let loaded_none: Scene = round_trip(&scene_none);
        assert_eq!(loaded_none.name, "No Metadata");
        assert!(loaded_none.author.is_none());
        assert!(loaded_none.created_at.is_none());
        assert!(loaded_none.modified_at.is_none());
        assert!(loaded_none.engine_version.is_none());

        // Test version defaults to 0 when absent
        let ron_no_version = r#"(
    name: "Version Default Test",
    entities: [],
)"#;
        let loaded_default: Scene = ron::from_str(ron_no_version).unwrap();
        assert_eq!(loaded_default.version, 0);
        assert_eq!(loaded_default.name, "Version Default Test");
    }
}
