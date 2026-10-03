//! Capture authored ECS state with persistent document keys.
use super::component_registry::SceneWriteContext;
use super::descriptors::*;
use super::identity::SceneIdentity;
use super::serialization::{SCENE_VERSION, asset_context, entity_assets_mut};
use super::{AssetRef, EntitySource, SceneAssetContext, SceneEntityId, SceneError};
use crate::animation::AnimationPlayer;
use crate::application::Application;
use crate::components::{
    DirectionalLight, DrawableComponent, NameComponent, ParticleEmitterComponent,
    PerspectiveComponent, PointLight, TransformComponent, VelocityComponent,
};
use katla_physics::{ColliderShape, CollisionFilter, PhysicsMaterial, RigidBody, TriggerVolume};
use katla_script::ScriptComponent;
use log::debug;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

pub(super) fn capture_scene(app: &mut Application) -> Result<Scene, SceneError> {
    let entities: Vec<_> = app
        .world
        .entity_ids()
        .filter(|id| {
            app.world
                .get_component::<crate::components::EditorHidden>(*id)
                .is_none()
        })
        .collect();
    capture_selection(app, &entities, None)
}

pub(crate) fn capture_selection(
    app: &mut Application,
    entities: &[katla_ecs::EntityId],
    root: Option<katla_ecs::EntityId>,
) -> Result<Scene, SceneError> {
    for entity in entities {
        if app
            .world
            .get_component::<TransformComponent>(*entity)
            .is_none()
        {
            return Err(SceneError::Capture(format!(
                "Visible entity {entity} has no TransformComponent; add a transform or mark runtime-only entities EditorHidden"
            )));
        }
    }
    let mut used = HashSet::new();
    let mut next = app.scene_document.next_entity_id;
    for id in entities {
        if let Some(identity) = app.world.get_component::<SceneIdentity>(*id) {
            if identity.id.0 == 0 || !used.insert(identity.id) {
                return Err(SceneError::entity(
                    identity.id,
                    "id",
                    "duplicate or zero scene identity",
                ));
            }
            next = next.max(identity.id.0.checked_add(1).ok_or_else(|| {
                SceneError::entity(identity.id, "id", "scene identity space exhausted")
            })?);
        }
    }
    for id in entities {
        if app.world.get_component::<SceneIdentity>(*id).is_none() {
            let key = SceneEntityId(next);
            next = next
                .checked_add(1)
                .ok_or_else(|| SceneError::entity(key, "id", "scene identity space exhausted"))?;
            app.world.add_component(*id, SceneIdentity { id: key });
        }
    }
    app.scene_document.next_entity_id = next;
    let context = match &app.scene_document.assets {
        Some(context) => context.clone(),
        None => asset_context(app, app.scene_document.path.as_deref())?,
    };
    app.scene_document.assets = Some(context.clone());
    let mut scene = app.scene_document.saved.clone();
    scene.version = SCENE_VERSION;
    scene.next_entity_id = next;
    capture_entities_scoped(app, entities, scene, &context, root)
}

pub(super) fn capture_entities(
    app: &Application,
    entities: &[katla_ecs::EntityId],
    scene: Scene,
    assets: &SceneAssetContext,
) -> Result<Scene, SceneError> {
    capture_entities_scoped(app, entities, scene, assets, None)
}

fn capture_entities_scoped(
    app: &Application,
    entities: &[katla_ecs::EntityId],
    mut scene: Scene,
    assets: &SceneAssetContext,
    root: Option<katla_ecs::EntityId>,
) -> Result<Scene, SceneError> {
    let mut mapping = HashMap::new();
    for entity in entities {
        let identity = app
            .world
            .get_component::<SceneIdentity>(*entity)
            .ok_or_else(|| SceneError::Capture("Entity has no assigned scene key".into()))?;
        mapping.insert(*entity, identity.id);
    }
    let references = SceneWriteContext { entities: mapping };
    let raw_ids: HashMap<_, _> = references
        .entities
        .iter()
        .map(|(entity, key)| (entity.id(), *key))
        .collect();
    scene.entities.clear();
    for entity in entities {
        let id = references.id(*entity).map_err(SceneError::Capture)?;
        let world = &app.world;
        let mut desc = EntityDescriptor::new(
            id,
            world
                .get_component::<EntitySource>(*entity)
                .cloned()
                .unwrap_or_default(),
        );
        desc.name = world
            .get_component::<NameComponent>(*entity)
            .map(|value| value.name.clone());
        if Some(*entity) != root {
            desc.parent = world
                .get_component::<crate::components::Parent>(*entity)
                .map(|parent| {
                    references
                        .id(parent.parent)
                        .map_err(|error| SceneError::entity(id, "parent", error))
                })
                .transpose()?;
        }
        let transform = world
            .get_component::<TransformComponent>(*entity)
            .ok_or_else(|| SceneError::entity(id, "transform", "transform missing"))?;
        let t = &transform.transform;
        let (x, y, z, w) = t.rotation.xyzw();
        desc.transform = TransformDescriptor {
            position: [t.position.x(), t.position.y(), t.position.z()],
            rotation: [x, y, z, w],
            scale: [t.scale.x(), t.scale.y(), t.scale.z()],
        };
        if world
            .get_component::<crate::components::BillboardComponent>(*entity)
            .is_none()
        {
            desc.drawable =
                world
                    .get_component::<DrawableComponent>(*entity)
                    .map(|d| DrawableDescriptor {
                        surface: Some(d.surface),
                        sampling: Some(d.sampling),
                        color: d.color.map(|c| {
                            let s = c.to_srgb();
                            [s.r, s.g, s.b, s.a]
                        }),
                        metallic: d.metallic,
                        roughness: d.roughness,
                        ao: d.ao,
                    });
        }
        desc.point_light =
            world
                .get_component::<PointLight>(*entity)
                .map(|v| PointLightDescriptor {
                    color: v.color,
                    intensity: v.intensity,
                    range: v.range,
                });
        desc.particle_emitter = world
            .get_component::<ParticleEmitterComponent>(*entity)
            .map(ParticleEmitterDescriptor::from_component);
        desc.animation = world
            .get_component::<AnimationPlayer>(*entity)
            .map(AnimationDescriptor::from);
        desc.velocity =
            world
                .get_component::<VelocityComponent>(*entity)
                .map(|v| VelocityDescriptor {
                    velocity: [v.velocity.x(), v.velocity.y(), v.velocity.z()],
                    acceleration: [v.acceleration.x(), v.acceleration.y(), v.acceleration.z()],
                });
        desc.perspective = world
            .get_component::<PerspectiveComponent>(*entity)
            .map(|v| PerspectiveDescriptor {
                fov: v.fov,
                near: v.near,
                aspect_ratio: v.aspect_ratio,
            });
        desc.directional_light =
            world
                .get_component::<DirectionalLight>(*entity)
                .map(|v| DirectionalLightDescriptor {
                    direction: [v.direction.x(), v.direction.y(), v.direction.z()],
                    color: v.color,
                    intensity: v.intensity,
                });
        desc.script = world
            .get_component::<ScriptComponent>(*entity)
            .map(|v| {
                let path = Path::new(&v.script_path);
                let path = if path.is_absolute() {
                    assets.identify(path)
                } else {
                    Ok(AssetRef::Resource(format!(
                        "scripts/{}",
                        path.with_extension("luau").display()
                    )))
                };
                path.map(|path| ScriptDescriptor { path })
                    .map_err(|error| SceneError::entity(id, "script.path", error))
            })
            .transpose()?;
        desc.audio_emitter = world
            .get_component::<crate::components::AudioEmitter>(*entity)
            .map(|v| {
                let path = Path::new(&v.source_path);
                let path = if path.is_absolute() {
                    assets.identify(path)
                } else if let Some(path) = v.source_path.strip_prefix("resources/") {
                    Ok(AssetRef::Resource(path.into()))
                } else {
                    Ok(AssetRef::Scene(v.source_path.clone()))
                };
                path.map(|path| AudioEmitterDescriptor {
                    path,
                    volume: v.volume,
                    looping: v.looping,
                    playing: v.playing,
                    spatial: v.spatial,
                    min_distance: v.min_distance,
                    max_distance: v.max_distance,
                    rolloff_factor: v.rolloff_factor,
                    distance_model: v.distance_model,
                })
                .map_err(|error| SceneError::entity(id, "audio_emitter.path", error))
            })
            .transpose()?;
        desc.rigid_body = world
            .get_component::<RigidBody>(*entity)
            .map(|v| RigidBodyDescriptor {
                kind: v.body_type,
                gravity_scale: v.gravity_scale,
                ccd_enabled: v.ccd_enabled,
                linear_velocity: [
                    v.linear_velocity.x(),
                    v.linear_velocity.y(),
                    v.linear_velocity.z(),
                ],
            });
        desc.reverb_zone = world
            .get_component::<crate::components::ReverbZone>(*entity)
            .cloned();
        desc.collider_shape = world
            .get_component::<ColliderShape>(*entity)
            .map(|v| match v {
                ColliderShape::Sphere(v) => ColliderShapeDescriptor::Sphere(v.radius),
                ColliderShape::Box(v) => ColliderShapeDescriptor::Box(v.half_extents),
                ColliderShape::Capsule(v) => ColliderShapeDescriptor::Capsule {
                    half_height: v.half_height,
                    radius: v.radius,
                },
                ColliderShape::Trimesh(_) => ColliderShapeDescriptor::Trimesh,
                ColliderShape::ConvexHull(_) => ColliderShapeDescriptor::ConvexHull,
                ColliderShape::Heightfield(v) => ColliderShapeDescriptor::Heightfield {
                    rows: v.rows,
                    cols: v.cols,
                    heights: v.heights.clone(),
                },
            });
        desc.physics_material =
            world
                .get_component::<PhysicsMaterial>(*entity)
                .map(|v| PhysicsMaterialDescriptor {
                    friction: v.friction,
                    restitution: v.restitution,
                    density: v.density,
                });
        desc.trigger_volume = world
            .get_component::<TriggerVolume>(*entity)
            .map(|_| TriggerVolumeDescriptor);
        desc.collision_filter =
            world
                .get_component::<CollisionFilter>(*entity)
                .map(|v| CollisionFilterDescriptor {
                    layers: v.layers,
                    mask: v.mask,
                });
        desc.trigger_rules = world
            .get_component::<crate::events::TriggerRules>(*entity)
            .map(|rules| {
                rules
                    .rules()
                    .iter()
                    .map(|rule| {
                        rule.map_entities(|raw| {
                            raw_ids.get(raw).copied().ok_or_else(|| {
                                SceneError::entity(
                                    id,
                                    "trigger_rules",
                                    "trigger target is outside this scene",
                                )
                            })
                        })
                    })
                    .collect::<Result<Vec<_>, SceneError>>()
            })
            .transpose()?
            .unwrap_or_default();
        desc.joint = world
            .get_component::<katla_physics::Joint>(*entity)
            .map(|v| {
                let endpoint = |raw, field| {
                    raw_ids.get(&raw).copied().ok_or_else(|| {
                        SceneError::entity(id, field, "joint endpoint is outside this scene")
                    })
                };
                Ok(JointDescriptor {
                    kind: v.joint_type,
                    a: endpoint(v.entity_a, "joint.a")?,
                    b: endpoint(v.entity_b, "joint.b")?,
                    anchor_a: v.anchor_a,
                    anchor_b: v.anchor_b,
                    limits: v.limits.map(|v| [v.min, v.max]),
                })
            })
            .transpose()?;
        desc.components = app.scene_components.capture(world, *entity, &references)?;
        for (field, reference) in entity_assets_mut(&mut desc) {
            let absolute = assets
                .resolve(reference)
                .map_err(|error| SceneError::entity(id, field, error))?;
            *reference = assets
                .identify(&absolute)
                .map_err(|error| SceneError::entity(id, field, error))?;
        }
        scene.entities.push(desc);
    }
    scene.entities.sort_by_key(|entity| entity.id);
    scene.validate()?;
    debug!(
        "Captured scene '{}' with {} entities",
        scene.name,
        scene.entities.len()
    );
    Ok(scene)
}
