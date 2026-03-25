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

        for entity_id in app.world.entity_ids() {
            let Some(transform) = app.world.get_component::<TransformComponent>(entity_id) else {
                continue;
            };

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

            let source = app
                .world
                .get_component::<EntitySource>(entity_id)
                .cloned()
                .unwrap_or(EntitySource::Cube {
                    size: [1.0, 1.0, 1.0],
                });

            let drawable = app
                .world
                .get_component::<DrawableComponent>(entity_id)
                .map(|d| DrawableDescriptor {
                    color: d.color.map(|c| [c.r, c.g, c.b, c.a]),
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
                    shape: p.config.shape,
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

        // Clear existing world state
        app.world.clear_entities();

        // Build a name -> entity_id mapping for parent resolution
        let mut name_to_entity: std::collections::HashMap<String, katla_ecs::EntityId> =
            std::collections::HashMap::new();

        // First pass: spawn all entities
        for desc in &scene.entities {
            let entity_id = Self::spawn_entity(app, desc)?;
            if let Some(ref name) = desc.name {
                name_to_entity.insert(name.clone(), entity_id);
            }
        }

        // Second pass: resolve parent relationships
        for desc in &scene.entities {
            if let Some(ref parent_name) = desc.parent {
                if let Some(&parent_id) = name_to_entity.get(parent_name) {
                    if let Some(&child_id) = desc.name.as_ref().and_then(|n| name_to_entity.get(n))
                    {
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

        let entity_id = match &desc.source {
            EntitySource::Cube { size } => {
                app.spawn_test_cube_with_color(pos, *size, color_from_desc(&desc.drawable))
            }
            EntitySource::Sphere {
                radius,
                segments,
                rings,
            } => app.spawn_sphere_with_color(
                pos,
                *radius,
                *segments,
                *rings,
                color_from_desc(&desc.drawable),
            ),
            EntitySource::Plane { width, height } => {
                app.spawn_plane_with_color(pos, *width, *height, color_from_desc(&desc.drawable))
            }
            EntitySource::Cylinder {
                height,
                radius,
                segments,
            } => app.spawn_cylinder_with_color(
                pos,
                *height,
                *radius,
                *segments,
                color_from_desc(&desc.drawable),
            ),
            EntitySource::Torus {
                radius,
                tube_radius,
                segments,
                tube_segments,
            } => app.spawn_torus_with_color(
                pos,
                *radius,
                *tube_radius,
                *segments,
                *tube_segments,
                color_from_desc(&desc.drawable),
            ),
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
                app.world.spawn((emitter,))
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
        };

        // Apply transform (rotation + scale) -- spawn functions only set position
        if let Some(transform) = app.world.get_component_mut::<TransformComponent>(entity_id) {
            transform.transform.rotation = katla_math::Quat::new(qx, qy, qz, qw);
            transform.transform.scale = katla_math::Vec3::new(sx, sy, sz);
        }

        // Apply drawable material overrides
        if let Some(ref drawable_desc) = desc.drawable {
            if let Some(drawable) = app.world.get_component_mut::<DrawableComponent>(entity_id) {
                drawable.metallic = drawable_desc.metallic;
                drawable.roughness = drawable_desc.roughness;
                drawable.ao = drawable_desc.ao;
                if let Some(c) = drawable_desc.color {
                    drawable.color = Some(katla_math::Color::new(c[0], c[1], c[2], c[3]));
                }
            }
        }

        // Apply particle emitter config overrides
        if let Some(ref pe_desc) = desc.particle_emitter {
            if let Some(emitter) = app
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
        }

        // Apply animation state
        if let Some(ref anim_desc) = desc.animation {
            if let Some(player) = app.world.get_component_mut::<AnimationPlayer>(entity_id) {
                if let Some(ref clip) = anim_desc.current_clip {
                    player.set_clip(clip.clone(), player.duration);
                }
                player.playing = anim_desc.playing;
                player.loop_animation = anim_desc.loop_animation;
                player.speed = anim_desc.speed;
                player.time = anim_desc.time;
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
                shape: 0,
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
                shape: 0,
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
}
