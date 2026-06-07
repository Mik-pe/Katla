use super::descriptors::{
    AnimationDescriptor, ColliderShapeDescriptor, DrawableDescriptor, EntityDescriptor,
    ParticleEmitterDescriptor, PhysicsMaterialDescriptor, PointLightDescriptor,
    RigidBodyDescriptor, Scene, TransformDescriptor,
};
use super::entity_source::EntitySource;

use super::serialization::SCENE_VERSION;

/// Path to the default scene file, relative to the workspace root.
pub const DEFAULT_SCENE_PATH: &str = "assets/scenes/default.katla";

/// Resolve the default scene path to an absolute path using `CARGO_MANIFEST_DIR`.
///
/// `CARGO_MANIFEST_DIR` points to `katla_app/`. Going up one level gives the
/// workspace root, where `assets/scenes/default.katla` actually lives. This
/// works regardless of cwd — both `cargo run` and `cargo test` get the same
/// canonical file.
pub fn default_scene_path() -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // up to workspace root
    path.push(DEFAULT_SCENE_PATH);
    path
}

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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: Some(RigidBodyDescriptor::Static),
        collider_shape: Some(ColliderShapeDescriptor::Box([10.0, 0.05, 10.0])),
        physics_material: Some(PhysicsMaterialDescriptor {
            friction: 0.7,
            restitution: 0.1,
            density: 1.0,
        }),
        trigger_volume: None,
        collision_filter: None,
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
            // Top two rows of the grid fall and stack on the ground.
            let is_dynamic = y >= grid_size - 2;
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
                script: None,
                perspective: None,
                directional_light: None,
                audio_emitter: None,
                rigid_body: is_dynamic.then_some(RigidBodyDescriptor::Dynamic),
                collider_shape: is_dynamic.then_some(ColliderShapeDescriptor::Sphere(0.4)),
                physics_material: is_dynamic.then_some(PhysicsMaterialDescriptor {
                    friction: 0.5,
                    restitution: 0.3,
                    density: 1.0,
                }),
                trigger_volume: None,
                collision_filter: None,
            });
        }
    }

    // Center cube
    scene.entities.push(EntityDescriptor {
        name: Some("CenterCube".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [-5.0, 2.0, -5.0],
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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: Some(RigidBodyDescriptor::Dynamic),
        collider_shape: Some(ColliderShapeDescriptor::Box([0.5, 0.5, 0.5])),
        physics_material: Some(PhysicsMaterialDescriptor {
            friction: 0.6,
            restitution: 0.2,
            density: 1.0,
        }),
        trigger_volume: None,
        collision_filter: None,
    });

    // Cyan sphere
    scene.entities.push(EntityDescriptor {
        name: Some("CyanSphere".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [-7.0, 3.0, -5.0],
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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: Some(RigidBodyDescriptor::Dynamic),
        collider_shape: Some(ColliderShapeDescriptor::Sphere(0.7)),
        physics_material: Some(PhysicsMaterialDescriptor {
            friction: 0.4,
            restitution: 0.4,
            density: 1.0,
        }),
        trigger_volume: None,
        collision_filter: None,
    });

    // Magenta cylinder
    scene.entities.push(EntityDescriptor {
        name: Some("MagentaCylinder".to_string()),
        parent: None,
        transform: TransformDescriptor {
            position: [5.0, 3.0, -5.0],
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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: Some(RigidBodyDescriptor::Dynamic),
        collider_shape: Some(ColliderShapeDescriptor::Capsule {
            half_height: 0.375,
            radius: 0.5,
        }),
        physics_material: Some(PhysicsMaterialDescriptor {
            friction: 0.5,
            restitution: 0.2,
            density: 1.0,
        }),
        trigger_volume: None,
        collision_filter: None,
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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: None,
        collider_shape: None,
        physics_material: None,
        trigger_volume: None,
        collision_filter: None,
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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: None,
        collider_shape: None,
        physics_material: None,
        trigger_volume: None,
        collision_filter: None,
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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: None,
        collider_shape: None,
        physics_material: None,
        trigger_volume: None,
        collision_filter: None,
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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: None,
        collider_shape: None,
        physics_material: None,
        trigger_volume: None,
        collision_filter: None,
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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: None,
        collider_shape: None,
        physics_material: None,
        trigger_volume: None,
        collision_filter: None,
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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: None,
        collider_shape: None,
        physics_material: None,
        trigger_volume: None,
        collision_filter: None,
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
        script: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        rigid_body: None,
        collider_shape: None,
        physics_material: None,
        trigger_volume: None,
        collision_filter: None,
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
            script: None,
            perspective: None,
            directional_light: None,
            audio_emitter: None,
            rigid_body: None,
            collider_shape: None,
            physics_material: None,
            trigger_volume: None,
            collision_filter: None,
        });
    }

    scene
}
