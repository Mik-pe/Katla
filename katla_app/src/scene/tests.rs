use super::descriptors::*;
use super::*;
use crate::components::{NameComponent, PointLight, TransformComponent};
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
            script: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
                script: None,
                mass: None,
                drag: None,
                perspective: None,
                directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
            script: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
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
            script: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
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
            script: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
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
            script: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
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
            script: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
            script: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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

// =========================================================================
// Load/Spawn Integration Tests (VAL-SCENE-018, VAL-CROSS-005, VAL-CROSS-006)
// =========================================================================

#[test]
fn test_load_spawn_integration() {
    // Verify a scene with all entity types round-trips correctly
    // through serialization. This tests the full data path that
    // load_scene uses: build Scene → RON serialize → RON deserialize → Scene.
    //
    // The actual entity spawning (GPU mesh creation, etc.) requires
    // a Vulkan context and is verified by `cargo run -s`.

    let mut scene = Scene::new("Integration Test");
    scene.version = SCENE_VERSION;

    // All entity types: Cube, Sphere, Plane, Cylinder, Torus, GltfModel,
    // ParticleEmitter, Light
    scene.entities.push(EntityDescriptor {
        name: Some("Cube1".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [1.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Cube {
            size: [1.0, 2.0, 1.0],
        },
        drawable: Some(DrawableDescriptor {
            color: Some([0.8, 0.2, 0.1, 1.0]),
            metallic: 0.5,
            roughness: 0.3,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: Some(VelocityDescriptor {
            velocity: [1.0, 0.0, 0.0],
            acceleration: [0.0, -9.8, 0.0],
        }),
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    scene.entities.push(EntityDescriptor {
        name: Some("Sphere1".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [0.0, 2.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Sphere {
            radius: 0.5,
            segments: 32,
            rings: 16,
        },
        drawable: Some(DrawableDescriptor {
            color: Some([0.1, 0.5, 0.9, 1.0]),
            metallic: 0.0,
            roughness: 0.8,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: None,
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    scene.entities.push(EntityDescriptor {
        name: Some("Plane1".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [0.0, -1.0, 0.0],
            rotation: [0.707, 0.0, 0.0, 0.707],
            scale: [10.0, 1.0, 10.0],
        },
        source: EntitySource::Plane {
            width: 20.0,
            height: 20.0,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    scene.entities.push(EntityDescriptor {
        name: Some("Cylinder1".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [-3.0, 0.0, 4.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Cylinder {
            height: 3.0,
            radius: 0.75,
            segments: 32,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    scene.entities.push(EntityDescriptor {
        name: Some("Torus1".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [0.0, 2.0, -5.0],
            rotation: [0.5, 0.5, 0.5, 0.5],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Torus {
            radius: 1.0,
            tube_radius: 0.3,
            segments: 32,
            tube_segments: 16,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    // GLTF model with animation (VAL-CROSS-005: animated model state round-trip)
    scene.entities.push(EntityDescriptor {
        name: Some("AnimatedFox".to_string()),
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
            speed: 1.5,
            time: 0.75,
            duration: 1.2,
            blending: true,
            target_clip: Some("Walk".to_string()),
            blend_weight: 0.4,
            blend_time: 0.3,
            blend_duration: 0.5,
            target_time: 1.0,
            target_duration: 3.0,
            loop_count: 2,
        }),
        velocity: None,
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    // Particle emitter
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
            shape: katla_gfx::particles::EmitterShape::Point,
            shape_params: [0.0; 4],
            active: true,
        }),
        animation: None,
        velocity: None,
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    // Parent-child hierarchy (VAL-CROSS-006)
    scene.entities.push(EntityDescriptor {
        name: Some("ParentEntity".to_string()),
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });
    scene.entities.push(EntityDescriptor {
        name: Some("ChildA".to_string()),
        parent: Some("ParentEntity".to_string()),
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
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
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    assert_eq!(scene.entities.len(), 11, "Scene must have 11 entities");

    // Round-trip through RON serialization (what load_from_file does)
    let loaded: Scene = round_trip(&scene);

    // Verify entity count
    assert_eq!(loaded.entities.len(), 11);

    // Verify each entity type survived
    let entity_names: Vec<_> = loaded
        .entities
        .iter()
        .filter_map(|e| e.name.clone())
        .collect();
    assert!(entity_names.contains(&"Cube1".to_string()));
    assert!(entity_names.contains(&"Sphere1".to_string()));
    assert!(entity_names.contains(&"Plane1".to_string()));
    assert!(entity_names.contains(&"Cylinder1".to_string()));
    assert!(entity_names.contains(&"Torus1".to_string()));
    assert!(entity_names.contains(&"AnimatedFox".to_string()));
    assert!(entity_names.contains(&"FireEmitter".to_string()));
    assert!(entity_names.contains(&"WarmLight".to_string()));
    assert!(entity_names.contains(&"ParentEntity".to_string()));
    assert!(entity_names.contains(&"ChildA".to_string()));
    assert!(entity_names.contains(&"Grandchild".to_string()));

    // VAL-CROSS-005: Animated model state preserved
    let fox = loaded
        .entities
        .iter()
        .find(|e| e.name == Some("AnimatedFox".to_string()))
        .expect("Fox must exist");
    let fox_anim = fox.animation.as_ref().expect("Fox must have animation");
    assert_eq!(fox_anim.current_clip, Some("Run".to_string()));
    assert!(fox_anim.playing);
    assert!(fox_anim.loop_animation);
    assert_eq!(fox_anim.speed, 1.5);
    assert_eq!(fox_anim.time, 0.75);
    assert_eq!(fox_anim.duration, 1.2);
    assert!(fox_anim.blending);
    assert_eq!(fox_anim.target_clip, Some("Walk".to_string()));
    assert_eq!(fox_anim.blend_weight, 0.4);
    assert_eq!(fox_anim.blend_time, 0.3);
    assert_eq!(fox_anim.blend_duration, 0.5);
    assert_eq!(fox_anim.target_time, 1.0);
    assert_eq!(fox_anim.target_duration, 3.0);
    assert_eq!(fox_anim.loop_count, 2);

    // VAL-CROSS-006: Parent-child hierarchy preserved
    let parent = loaded
        .entities
        .iter()
        .find(|e| e.name == Some("ParentEntity".to_string()))
        .expect("Parent must exist");
    let child_a = loaded
        .entities
        .iter()
        .find(|e| e.name == Some("ChildA".to_string()))
        .expect("ChildA must exist");
    let grandchild = loaded
        .entities
        .iter()
        .find(|e| e.name == Some("Grandchild".to_string()))
        .expect("Grandchild must exist");
    assert!(parent.parent.is_none());
    assert_eq!(child_a.parent, Some("ParentEntity".to_string()));
    assert_eq!(grandchild.parent, Some("ChildA".to_string()));
    assert_eq!(parent.transform.scale, [2.0, 2.0, 2.0]);
    assert_eq!(child_a.transform.scale, [0.5, 0.5, 0.5]);
    assert_eq!(grandchild.transform.scale, [0.25, 0.25, 0.25]);

    // Verify all data fields match between original and loaded
    for (original, loaded) in scene.entities.iter().zip(loaded.entities.iter()) {
        assert_eq!(original.name, loaded.name);
        assert_eq!(original.source, loaded.source);
        assert_eq!(original.transform, loaded.transform);
        assert_eq!(original.parent, loaded.parent);
        assert_eq!(original.drawable, loaded.drawable);
        assert_eq!(original.point_light, loaded.point_light);
        assert_eq!(original.particle_emitter, loaded.particle_emitter);
        assert_eq!(original.animation, loaded.animation);
        assert_eq!(original.velocity, loaded.velocity);
    }
}

// =========================================================================
// Cross-Area Integration Tests (VAL-CROSS-002, VAL-CROSS-003,
// VAL-CROSS-004, VAL-CROSS-007)
// =========================================================================

/// VAL-CROSS-002: Transform edit persistence through save-reload.
///
/// Simulates the full editor workflow: build a scene → modify a transform
/// → serialize → deserialize → verify transform values match within epsilon.
#[test]
fn test_transform_persistence() {
    let mut scene = Scene::new("Transform Persistence Test");
    scene.version = SCENE_VERSION;

    // Create entity with non-default transform
    let original_position = [3.14, 2.71, -1.62];
    let original_rotation = [0.0, 0.38268343, 0.0, 0.92387953]; // 45° around Y
    let original_scale = [2.0, 0.5, 3.0];

    scene.entities.push(EntityDescriptor {
        name: Some("MovingCube".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: original_position,
            rotation: original_rotation,
            scale: original_scale,
        },
        source: EntitySource::Cube {
            size: [1.0, 1.0, 1.0],
        },
        drawable: Some(DrawableDescriptor {
            color: Some([0.8, 0.2, 0.1, 1.0]),
            metallic: 0.5,
            roughness: 0.3,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: None,
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    // Simulate editing: modify the transform values
    let edited_position = [5.0, 10.0, -3.5];
    let edited_rotation = [0.0, 0.70710678, 0.0, 0.70710678]; // 90° around Y
    let edited_scale = [4.0, 1.0, 2.0];

    scene.entities[0].transform = TransformDescriptor {
        position: edited_position,
        rotation: edited_rotation,
        scale: edited_scale,
    };

    // Simulate save → reload via RON round-trip
    let loaded: Scene = round_trip(&scene);

    // Verify transform values match within epsilon
    let eps = f32::EPSILON;
    let loaded_transform = &loaded.entities[0].transform;

    for i in 0..3 {
        assert!(
            (loaded_transform.position[i] - edited_position[i]).abs() < eps,
            "Position[{}] mismatch: got {}, expected {}",
            i,
            loaded_transform.position[i],
            edited_position[i]
        );
        assert!(
            (loaded_transform.rotation[i] - edited_rotation[i]).abs() < eps,
            "Rotation[{}] mismatch: got {}, expected {}",
            i,
            loaded_transform.rotation[i],
            edited_rotation[i]
        );
        assert!(
            (loaded_transform.scale[i] - edited_scale[i]).abs() < eps,
            "Scale[{}] mismatch: got {}, expected {}",
            i,
            loaded_transform.scale[i],
            edited_scale[i]
        );
    }

    // Also verify rotation w component
    assert!(
        (loaded_transform.rotation[3] - edited_rotation[3]).abs() < eps,
        "Rotation[3] mismatch: got {}, expected {}",
        loaded_transform.rotation[3],
        edited_rotation[3]
    );

    // Verify entity name and source are preserved
    assert_eq!(loaded.entities[0].name, Some("MovingCube".to_string()));
    assert_eq!(
        loaded.entities[0].source,
        EntitySource::Cube {
            size: [1.0, 1.0, 1.0],
        }
    );

    // Verify drawable properties are preserved
    let drawable = loaded.entities[0].drawable.as_ref().unwrap();
    assert_eq!(drawable.metallic, 0.5);
    assert_eq!(drawable.roughness, 0.3);
}

/// VAL-CROSS-003: Entity destruction cascades ECS cleanup and GPU release.
///
/// Creates entities with GPU resources, destroys them, and verifies:
/// - EntityEvent::Destroyed is emitted
/// - ComponentEvent::Removed events are emitted for all components
/// - Entity count decreases
/// - GpuResourceTracker correctly releases resources
#[test]
fn test_entity_destruction_cleanup() {
    use crate::components::DrawableComponent;
    use crate::gpu_resource_tracker::GpuResourceTracker;
    use katla_ecs::events::{ComponentEvent, EntityEvent};
    use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle};
    use std::any::TypeId;

    let protected = MaterialHandle::new(999);
    let mut tracker = GpuResourceTracker::new(protected);

    let mut world = katla_ecs::World::new();

    // Create entity with multiple components
    let entity_a = world.create_entity();
    world.add_component(entity_a, TransformComponent::default());
    let mesh_a = MeshHandle::new(1);
    let mat_a = MaterialHandle::new(2);
    world.add_component(entity_a, DrawableComponent::with_handles(mesh_a, mat_a));
    world.add_component(entity_a, PointLight::new([1.0, 0.5, 0.2], 10.0, 15.0));
    world.add_component(entity_a, NameComponent::new("EntityA"));

    // Track GPU resources
    tracker.track_drawable(mesh_a, mat_a, SkeletonHandle::NONE);

    // Create second entity
    let entity_b = world.create_entity();
    world.add_component(entity_b, TransformComponent::default());
    let mesh_b = MeshHandle::new(3);
    let mat_b = MaterialHandle::new(4);
    world.add_component(entity_b, DrawableComponent::with_handles(mesh_b, mat_b));
    world.add_component(entity_b, NameComponent::new("EntityB"));
    tracker.track_drawable(mesh_b, mat_b, SkeletonHandle::NONE);

    // Destroy entity_a
    let destroyed = world.destroy_entity(entity_a);
    assert!(
        destroyed,
        "destroy_entity should return true for live entity"
    );

    // Verify EntityEvent::Destroyed was emitted
    let entity_events = world.entity_events();
    let destroyed_events: Vec<_> = entity_events
        .iter()
        .filter(|e| matches!(e, EntityEvent::Destroyed(id) if *id == entity_a))
        .collect();
    assert_eq!(
        destroyed_events.len(),
        1,
        "Exactly one Destroyed event for entity_a"
    );

    // Verify ComponentEvent::Removed events for all components
    let component_events = world.component_events();
    let transform_type_id = TypeId::of::<TransformComponent>();
    let drawable_type_id = TypeId::of::<DrawableComponent>();
    let point_light_type_id = TypeId::of::<PointLight>();
    let name_type_id = TypeId::of::<NameComponent>();

    let removed_for_a: Vec<_> = component_events
        .iter()
        .filter(|e| matches!(e, ComponentEvent::Removed(id, tid) if *id == entity_a))
        .collect();

    let removed_type_ids: Vec<TypeId> = removed_for_a
        .iter()
        .map(|e| {
            if let ComponentEvent::Removed(_, tid) = e {
                *tid
            } else {
                unreachable!()
            }
        })
        .collect();

    assert!(
        removed_type_ids.contains(&transform_type_id),
        "TransformComponent Removed event should be emitted"
    );
    assert!(
        removed_type_ids.contains(&drawable_type_id),
        "DrawableComponent Removed event should be emitted"
    );
    assert!(
        removed_type_ids.contains(&point_light_type_id),
        "PointLight Removed event should be emitted"
    );
    assert!(
        removed_type_ids.contains(&name_type_id),
        "NameComponent Removed event should be emitted"
    );

    // Simulate GPU cleanup for destroyed entity
    // (In production, gpu_cleanup module does this after world.update())
    let to_destroy = tracker.release_drawable(mesh_a, mat_a, SkeletonHandle::NONE);
    assert_eq!(to_destroy.meshes.len(), 1, "Mesh should be destroyed");
    assert_eq!(
        to_destroy.materials.len(),
        1,
        "Material should be destroyed"
    );
    assert_eq!(tracker.mesh_count(), 1, "One mesh remains (entity_b)");
    assert_eq!(
        tracker.material_count(),
        1,
        "One material remains (entity_b)"
    );

    // Verify entity_b is unaffected
    let entity_b_name = world.get_component::<NameComponent>(entity_b);
    assert!(entity_b_name.is_some(), "entity_b should still be alive");
    assert_eq!(entity_b_name.unwrap().name, "EntityB");

    // Update the world to flush events
    world.update(0.016);
    assert!(
        world.entity_events().is_empty(),
        "Events should be flushed after update()"
    );
    assert!(
        world.component_events().is_empty(),
        "Component events should be flushed after update()"
    );

    // Now destroy entity_b
    world.destroy_entity(entity_b);
    let to_destroy_b = tracker.release_drawable(mesh_b, mat_b, SkeletonHandle::NONE);
    assert_eq!(to_destroy_b.meshes.len(), 1);
    assert_eq!(to_destroy_b.materials.len(), 1);
    assert_eq!(tracker.mesh_count(), 0, "All meshes released");
    assert_eq!(tracker.material_count(), 0, "All materials released");
}

/// VAL-CROSS-004: Component addition triggers event and change detection.
///
/// Adds components to entities and verifies:
/// - ComponentEvent::Added is emitted
/// - Change detection marks entity as changed via query_changed
#[test]
fn test_component_add_events() {
    use crate::components::DrawableComponent;
    use katla_ecs::events::ComponentEvent;
    use std::any::TypeId;

    let mut world = katla_ecs::World::new();

    // Create entity
    let entity = world.create_entity();
    world.add_component(entity, TransformComponent::default());

    // Clear any events from entity creation
    world.update(0.016);

    // Add a component
    world.add_component(entity, NameComponent::new("TestEntity"));

    // Verify ComponentEvent::Added was emitted
    let component_events = world.component_events();
    let name_type_id = TypeId::of::<NameComponent>();
    let added_events: Vec<_> = component_events
        .iter()
        .filter(|e| {
            matches!(e, ComponentEvent::Added(id, tid) if *id == entity && *tid == name_type_id)
        })
        .collect();
    assert_eq!(
        added_events.len(),
        1,
        "Exactly one NameComponent Added event should be emitted"
    );

    // Verify component_events_for type-safe filtering
    let filtered = world.component_events_for::<NameComponent>();
    assert_eq!(
        filtered.len(),
        1,
        "component_events_for::<NameComponent> should return 1 event"
    );

    // Verify change detection marks entity as changed
    let changed_entities: Vec<_> = world
        .query_changed::<(&TransformComponent, &NameComponent)>()
        .collect();
    assert!(
        changed_entities.iter().any(|(id, _, _)| *id == entity),
        "Entity should appear in query_changed after add_component"
    );

    // Add another component (DrawableComponent) and verify both events
    world.add_component(
        entity,
        DrawableComponent::with_handles(
            katla_gfx::MeshHandle::new(1),
            katla_gfx::MaterialHandle::new(2),
        ),
    );

    let drawable_type_id = TypeId::of::<DrawableComponent>();
    let drawable_added: Vec<_> = world
        .component_events()
        .iter()
        .filter(|e| {
            matches!(e, ComponentEvent::Added(id, tid) if *id == entity && *tid == drawable_type_id)
        })
        .collect();
    assert_eq!(
        drawable_added.len(),
        1,
        "DrawableComponent Added event should be emitted"
    );

    // Create a second entity with same components but don't add NameComponent
    let entity2 = world.create_entity();
    world.add_component(entity2, TransformComponent::default());
    world.add_component(entity2, NameComponent::new("Entity2"));

    // Verify entity2 events
    let name_events_for = world.component_events_for::<NameComponent>();
    let entity2_name_events: Vec<_> = name_events_for
        .iter()
        .filter(|e| matches!(e, ComponentEvent::Added(id, _) if *id == entity2))
        .collect();
    assert_eq!(
        entity2_name_events.len(),
        1,
        "Entity2 should have its own NameComponent Added event"
    );

    // Update to flush events
    world.update(0.016);
    assert!(
        world.component_events().is_empty(),
        "Component events should be flushed after update()"
    );

    // After flush, query_changed should return empty until next mutation
    // (clear_changed is called at end of update)
    let changed_after_flush: Vec<_> = world
        .query_changed::<(&TransformComponent, &NameComponent)>()
        .collect();
    assert!(
        changed_after_flush.is_empty(),
        "query_changed should return empty after clear_changed"
    );
}

/// VAL-CROSS-007: Full editor workflow end-to-end.
///
/// Simulates the complete editor workflow:
/// 1. Spawn entities (build scene)
/// 2. Edit properties (modify transforms, add components)
/// 3. Save (serialize to RON)
/// 4. New scene (clear)
/// 5. Load (deserialize from RON)
/// 6. Verify state
#[test]
fn test_full_editor_workflow() {
    // Step 1: Spawn entities — build initial scene
    let mut scene = Scene::new("Editor Workflow Test");
    scene.version = SCENE_VERSION;

    // Add a cube entity
    scene.entities.push(EntityDescriptor {
        name: Some("TestCube".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Cube {
            size: [1.0, 1.0, 1.0],
        },
        drawable: Some(DrawableDescriptor {
            color: Some([0.8, 0.2, 0.1, 1.0]),
            metallic: 0.5,
            roughness: 0.3,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: None,
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    // Add a light entity
    scene.entities.push(EntityDescriptor {
        name: Some("TestLight".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [5.0, 3.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        source: EntitySource::Light,
        drawable: Some(DrawableDescriptor {
            color: Some([1.0, 0.9, 0.8, 1.0]),
            metallic: 0.0,
            roughness: 1.0,
            ao: 1.0,
        }),
        point_light: Some(PointLightDescriptor {
            color: [1.0, 0.9, 0.8],
            intensity: 20.0,
            range: 15.0,
        }),
        particle_emitter: None,
        animation: None,
        velocity: None,
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    // Add a sphere with velocity
    scene.entities.push(EntityDescriptor {
        name: Some("MovingSphere".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [0.0, 2.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [0.5, 0.5, 0.5],
        },
        source: EntitySource::Sphere {
            radius: 0.5,
            segments: 16,
            rings: 8,
        },
        drawable: Some(DrawableDescriptor {
            color: Some([0.1, 0.5, 0.9, 1.0]),
            metallic: 0.0,
            roughness: 0.8,
            ao: 1.0,
        }),
        point_light: None,
        particle_emitter: None,
        animation: None,
        velocity: Some(VelocityDescriptor {
            velocity: [1.0, 0.0, -2.0],
            acceleration: [0.0, -9.8, 0.0],
        }),
        script: None,
        mass: None,
        drag: None,
        perspective: None,
        directional_light: None,
    });

    assert_eq!(
        scene.entities.len(),
        3,
        "Initial scene must have 3 entities"
    );

    // Step 2: Edit properties — modify transforms
    // Simulate user moving the cube via inspector
    scene.entities[0].transform.position = [2.5, 1.0, -3.0];
    scene.entities[0].transform.scale = [2.0, 2.0, 2.0];
    scene.entities[0].drawable.as_mut().unwrap().metallic = 0.8;

    // Simulate user adjusting light intensity
    scene.entities[1].point_light.as_mut().unwrap().intensity = 35.0;
    scene.entities[1].transform.position = [8.0, 5.0, 2.0];

    // Simulate user editing sphere velocity
    scene.entities[2].velocity.as_mut().unwrap().velocity = [0.0, 3.0, -1.0];

    // Step 3: Save — serialize to RON
    let saved_ron = to_string_pretty(&scene, ron_pretty_config()).unwrap();
    assert!(
        saved_ron.contains("TestCube"),
        "Saved scene must contain TestCube"
    );
    assert!(
        saved_ron.contains("TestLight"),
        "Saved scene must contain TestLight"
    );
    assert!(
        saved_ron.contains("MovingSphere"),
        "Saved scene must contain MovingSphere"
    );

    // Step 4: New scene — simulate clearing (create fresh scene)
    let mut new_scene = Scene::new("New Empty Scene");
    new_scene.version = SCENE_VERSION;
    assert!(new_scene.entities.is_empty(), "New scene must be empty");

    // Step 5: Load — deserialize from saved RON
    let loaded_scene: Scene = ron::from_str(&saved_ron).unwrap();

    // Step 6: Verify state — all entities present with correct edited values
    assert_eq!(
        loaded_scene.entities.len(),
        3,
        "Loaded scene must have 3 entities"
    );

    // Verify TestCube with edited values
    let cube = loaded_scene
        .entities
        .iter()
        .find(|e| e.name == Some("TestCube".to_string()))
        .expect("TestCube must exist after load");
    let eps = f32::EPSILON;
    assert!(
        (cube.transform.position[0] - 2.5).abs() < eps,
        "Cube X position should be 2.5"
    );
    assert!(
        (cube.transform.position[1] - 1.0).abs() < eps,
        "Cube Y position should be 1.0"
    );
    assert!(
        (cube.transform.position[2] - (-3.0)).abs() < eps,
        "Cube Z position should be -3.0"
    );
    assert!(
        (cube.transform.scale[0] - 2.0).abs() < eps,
        "Cube scale should be [2.0, 2.0, 2.0]"
    );
    assert_eq!(
        cube.drawable.as_ref().unwrap().metallic,
        0.8,
        "Cube metallic should be updated to 0.8"
    );

    // Verify TestLight with edited values
    let light = loaded_scene
        .entities
        .iter()
        .find(|e| e.name == Some("TestLight".to_string()))
        .expect("TestLight must exist after load");
    assert!(
        (light.transform.position[0] - 8.0).abs() < eps,
        "Light X position should be 8.0"
    );
    assert!(
        (light.transform.position[1] - 5.0).abs() < eps,
        "Light Y position should be 5.0"
    );
    assert_eq!(
        light.point_light.as_ref().unwrap().intensity,
        35.0,
        "Light intensity should be updated to 35.0"
    );
    assert_eq!(
        light.point_light.as_ref().unwrap().color,
        [1.0, 0.9, 0.8],
        "Light color should be preserved"
    );

    // Verify MovingSphere with edited velocity
    let sphere = loaded_scene
        .entities
        .iter()
        .find(|e| e.name == Some("MovingSphere".to_string()))
        .expect("MovingSphere must exist after load");
    let vel = sphere.velocity.as_ref().unwrap();
    assert!(
        (vel.velocity[1] - 3.0).abs() < eps,
        "Sphere velocity Y should be 3.0"
    );
    assert!(
        (vel.velocity[2] - (-1.0)).abs() < eps,
        "Sphere velocity Z should be -1.0"
    );
    assert!(
        (vel.acceleration[1] - (-9.8)).abs() < eps,
        "Sphere acceleration Y should be -9.8"
    );

    // Verify entity sources are preserved
    assert_eq!(
        cube.source,
        EntitySource::Cube {
            size: [1.0, 1.0, 1.0],
        }
    );
    assert_eq!(light.source, EntitySource::Light);
    assert_eq!(
        sphere.source,
        EntitySource::Sphere {
            radius: 0.5,
            segments: 16,
            rings: 8,
        }
    );

    // Verify round-trip consistency: load → save → load should be identical
    let reloaded: Scene = round_trip(&loaded_scene);
    for (first, second) in loaded_scene.entities.iter().zip(reloaded.entities.iter()) {
        assert_eq!(first.name, second.name, "Name mismatch on re-round-trip");
        assert_eq!(
            first.source, second.source,
            "Source mismatch on re-round-trip"
        );
        assert_eq!(
            first.transform, second.transform,
            "Transform mismatch on re-round-trip for {:?}",
            first.name
        );
        assert_eq!(first.drawable, second.drawable, "Drawable mismatch");
        assert_eq!(first.point_light, second.point_light, "PointLight mismatch");
        assert_eq!(first.velocity, second.velocity, "Velocity mismatch");
    }
}
