//! Allocate entity sources and restore built-in components without runtime handles.
use super::component_registry::SceneCustomData;
use super::descriptors::{ColliderShapeDescriptor, DrawableDescriptor};
use super::identity::SceneIdentity;
use super::{EntityDescriptor, EntitySource, SceneAssetContext};
use crate::animation::AnimationPlayer;
use crate::application::Application;
use crate::components::{
    DirectionalLight, DrawableComponent, NameComponent, PerspectiveComponent, PointLight,
    TransformComponent, VelocityComponent,
};
use katla_gfx::primitives;
use katla_physics::{
    BodyType, BoxShape, CapsuleShape, ColliderShape, CollisionFilter, HeightfieldShape,
    PhysicsMaterial, RigidBody, SphereShape, TriggerVolume,
};
use katla_script::ScriptComponent;

/// Spawn a single entity from its descriptor.
pub(super) fn spawn_entity(
    app: &mut Application,
    desc: &EntityDescriptor,
    context: &SceneAssetContext,
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

        let drawable =
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, linear_color)
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
            EntitySource::MeshAsset { path } => {
                let (mesh, bounds) = crate::mesh_asset::upload(app, &context.resolve(path)?)?;
                let material = app.default_material();
                let drawable = DrawableComponent::with_handles_and_color(
                    mesh,
                    material,
                    color_from_desc(&desc.drawable).to_linear(),
                )
                .with_bounds(bounds);
                app.gpu_resource_tracker
                    .track_drawable(mesh, material, drawable.skeleton_handle);
                app.world.spawn((
                    TransformComponent::from_position(katla_math::Vec3::new(
                        pos[0], pos[1], pos[2],
                    )),
                    drawable,
                ))
            }
            EntitySource::GltfModel { path } => app
                .spawn_gltf_model(context.resolve(path)?, pos, None)
                .map_err(|e| format!("{e}"))?,
            EntitySource::StlModel { path } => app
                .spawn_stl_model(context.resolve(path)?, pos)
                .map_err(|e| format!("{e}"))?,
            EntitySource::Empty
            | EntitySource::ParticleEmitter
            | EntitySource::Light
            | EntitySource::Trigger => {
                let entity =
                    app.world
                        .spawn((TransformComponent::from_position(katla_math::Vec3::new(
                            pos[0], pos[1], pos[2],
                        )),));
                match desc.source {
                    EntitySource::Light => app.attach_billboard_icon(
                        entity,
                        crate::components::billboard::BillboardIcon::Lightbulb,
                    ),
                    EntitySource::ParticleEmitter => app.attach_billboard_icon(
                        entity,
                        crate::components::billboard::BillboardIcon::Fire,
                    ),
                    _ => {}
                }
                entity
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

    if let Some(particles) = &desc.particle_emitter {
        app.world
            .add_component(entity_id, particles.to_component(pos));
    }
    if let Some(light) = &desc.point_light {
        app.world.add_component(
            entity_id,
            PointLight::new(light.color, light.intensity, light.range),
        );
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
        app.world.add_component(
            entity_id,
            ScriptComponent::new(
                context
                    .resolve(&script_desc.path)?
                    .to_string_lossy()
                    .as_ref(),
            ),
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

    // Attach audio emitter
    if let Some(ref audio_desc) = desc.audio_emitter {
        app.world.add_component(
            entity_id,
            crate::components::AudioEmitter {
                source_path: context
                    .resolve(&audio_desc.path)?
                    .to_string_lossy()
                    .into_owned(),
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
        let mut rb = match rb_desc.kind {
            BodyType::Static => RigidBody::static_body(),
            BodyType::Dynamic => RigidBody::dynamic(),
            BodyType::Kinematic => RigidBody::kinematic(),
        };
        rb.gravity_scale = rb_desc.gravity_scale;
        rb.ccd_enabled = rb_desc.ccd_enabled;
        rb.linear_velocity = katla_math::Vec3::new(
            rb_desc.linear_velocity[0],
            rb_desc.linear_velocity[1],
            rb_desc.linear_velocity[2],
        );
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
            ColliderShapeDescriptor::Trimesh => ColliderShape::Trimesh(
                app.world
                    .get_component::<DrawableComponent>(entity_id)
                    .ok_or("Mesh collider requires a drawable")?
                    .mesh_handle,
            ),
            ColliderShapeDescriptor::ConvexHull => ColliderShape::ConvexHull(
                app.world
                    .get_component::<DrawableComponent>(entity_id)
                    .ok_or("Convex collider requires a drawable")?
                    .mesh_handle,
            ),
            ColliderShapeDescriptor::Heightfield {
                rows,
                cols,
                heights,
            } => ColliderShape::Heightfield(HeightfieldShape::new(*rows, *cols, heights.clone())),
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

    app.world
        .add_component(entity_id, SceneIdentity { id: desc.id });
    app.world.add_component(
        entity_id,
        SceneCustomData {
            components: desc.components.clone(),
        },
    );
    if let Some(name) = &desc.name {
        app.world.add_component(entity_id, NameComponent::new(name));
    } else {
        app.world.remove_component::<NameComponent>(entity_id);
    }

    Ok(entity_id)
}
fn color_from_desc(drawable: &Option<DrawableDescriptor>) -> katla_math::Color {
    drawable
        .as_ref()
        .and_then(|d| d.color)
        .map(|c| katla_math::Color::new(c[0], c[1], c[2], c[3]))
        .unwrap_or(katla_math::Color::WHITE)
}
