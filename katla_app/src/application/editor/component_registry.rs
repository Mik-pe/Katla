use katla_ecs::EntityId;
use katla_ecs::World;
use katla_ecs::inspect::{FieldMut, Inspect};
use katla_ecs::scene_tool::SceneToolError;
use katla_ecs::scene_tool::registry::{ComponentRegistry, ComponentRegistryEntry, FieldValue};

use crate::components::ParticleEmitterComponent;
use crate::components::{
    AudioEmitter, DirectionalLight, DragComponent, MassComponent, NameComponent,
    PerspectiveComponent, PointLight, VelocityComponent,
};
use katla_physics::{ColliderShape, CollisionFilter};

fn field_type_mismatch(field_name: &str, expected: &str, value: FieldValue) -> SceneToolError {
    SceneToolError::InvalidFieldValue {
        field: field_name.to_string(),
        expected_type: expected.to_string(),
        got: value.type_name().to_string(),
    }
}

fn register_name_component(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "NameComponent",
        has_component: |world: &World, entity: EntityId| {
            world.get_component::<NameComponent>(entity).is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, NameComponent::new("Unnamed"));
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<NameComponent>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| NameComponent::fields(),
        get_field_value: |world: &mut World, entity: EntityId, field_name: &str| {
            let comp = world.get_component_mut::<NameComponent>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::String(v) => FieldValue::String(v.clone()),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
            let comp = world
                .get_component_mut::<NameComponent>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "NameComponent".to_string(),
                })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "NameComponent".to_string(),
                        field: field_name.to_string(),
                    })?;
            match (field_mut, value) {
                (FieldMut::String(ref mut target), FieldValue::String(v)) => {
                    **target = v;
                    Ok(())
                }
                (_, v) => Err(field_type_mismatch(field_name, "String", v)),
            }
        },
    });
}

fn register_point_light(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "PointLight",
        has_component: |world: &World, entity: EntityId| {
            world.get_component::<PointLight>(entity).is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, PointLight::default());
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<PointLight>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| PointLight::fields(),
        get_field_value: |world: &mut World, entity: EntityId, field_name: &str| {
            let comp = world.get_component_mut::<PointLight>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::F32(v) => FieldValue::F32(*v),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
            let comp = world
                .get_component_mut::<PointLight>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "PointLight".to_string(),
                })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "PointLight".to_string(),
                        field: field_name.to_string(),
                    })?;
            match (field_mut, value) {
                (FieldMut::F32(ref mut target), FieldValue::F32(v)) => {
                    **target = v;
                    Ok(())
                }
                (_, v) => Err(field_type_mismatch(field_name, "f32", v)),
            }
        },
    });
}

fn register_mass_component(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "MassComponent",
        has_component: |world: &World, entity: EntityId| {
            world.get_component::<MassComponent>(entity).is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, MassComponent::default());
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<MassComponent>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| MassComponent::fields(),
        get_field_value: |world: &mut World, entity: EntityId, field_name: &str| {
            let comp = world.get_component_mut::<MassComponent>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::F32(v) => FieldValue::F32(*v),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
            let comp = world
                .get_component_mut::<MassComponent>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "MassComponent".to_string(),
                })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "MassComponent".to_string(),
                        field: field_name.to_string(),
                    })?;
            match (field_mut, value) {
                (FieldMut::F32(ref mut target), FieldValue::F32(v)) => {
                    **target = v;
                    Ok(())
                }
                (_, v) => Err(field_type_mismatch(field_name, "f32", v)),
            }
        },
    });
}

fn register_drag_component(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "DragComponent",
        has_component: |world: &World, entity: EntityId| {
            world.get_component::<DragComponent>(entity).is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, DragComponent::default());
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<DragComponent>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| DragComponent::fields(),
        get_field_value: |world: &mut World, entity: EntityId, field_name: &str| {
            let comp = world.get_component_mut::<DragComponent>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::F32(v) => FieldValue::F32(*v),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
            let comp = world
                .get_component_mut::<DragComponent>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "DragComponent".to_string(),
                })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "DragComponent".to_string(),
                        field: field_name.to_string(),
                    })?;
            match (field_mut, value) {
                (FieldMut::F32(ref mut target), FieldValue::F32(v)) => {
                    **target = v;
                    Ok(())
                }
                (_, v) => Err(field_type_mismatch(field_name, "f32", v)),
            }
        },
    });
}

fn register_perspective_component(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "PerspectiveComponent",
        has_component: |world: &World, entity: EntityId| {
            world
                .get_component::<PerspectiveComponent>(entity)
                .is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, PerspectiveComponent::default());
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<PerspectiveComponent>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| PerspectiveComponent::fields(),
        get_field_value: |world: &mut World, entity: EntityId, field_name: &str| {
            let comp = world.get_component_mut::<PerspectiveComponent>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::F32(v) => FieldValue::F32(*v),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
            let comp = world
                .get_component_mut::<PerspectiveComponent>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "PerspectiveComponent".to_string(),
                })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "PerspectiveComponent".to_string(),
                        field: field_name.to_string(),
                    })?;
            match (field_mut, value) {
                (FieldMut::F32(ref mut target), FieldValue::F32(v)) => {
                    **target = v;
                    Ok(())
                }
                (_, v) => Err(field_type_mismatch(field_name, "f32", v)),
            }
        },
    });
}

fn register_directional_light(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "DirectionalLight",
        has_component: |world: &World, entity: EntityId| {
            world.get_component::<DirectionalLight>(entity).is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, DirectionalLight::default());
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<DirectionalLight>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| DirectionalLight::fields(),
        get_field_value: |world: &mut World, entity: EntityId, field_name: &str| {
            let comp = world.get_component_mut::<DirectionalLight>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::F32(v) => FieldValue::F32(*v),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
            let comp = world
                .get_component_mut::<DirectionalLight>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "DirectionalLight".to_string(),
                })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "DirectionalLight".to_string(),
                        field: field_name.to_string(),
                    })?;
            match (field_mut, value) {
                (FieldMut::F32(ref mut target), FieldValue::F32(v)) => {
                    **target = v;
                    Ok(())
                }
                (_, v) => Err(field_type_mismatch(field_name, "f32", v)),
            }
        },
    });
}

fn register_script_component(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "ScriptComponent",
        has_component: |world: &World, entity: EntityId| {
            world
                .get_component::<katla_script::ScriptComponent>(entity)
                .is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, katla_script::ScriptComponent::new(""));
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<katla_script::ScriptComponent>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| katla_script::ScriptComponent::fields(),
        get_field_value: |world: &mut World, entity: EntityId, field_name: &str| {
            let comp = world.get_component_mut::<katla_script::ScriptComponent>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::String(v) => FieldValue::String(v.clone()),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
            let comp = world
                .get_component_mut::<katla_script::ScriptComponent>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "ScriptComponent".to_string(),
                })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "ScriptComponent".to_string(),
                        field: field_name.to_string(),
                    })?;
            match (field_mut, value) {
                (FieldMut::String(ref mut target), FieldValue::String(v)) => {
                    **target = v;
                    Ok(())
                }
                (_, v) => Err(field_type_mismatch(field_name, "String", v)),
            }
        },
    });
}

fn register_velocity_component(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "VelocityComponent",
        has_component: |world: &World, entity: EntityId| {
            world.get_component::<VelocityComponent>(entity).is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, VelocityComponent::default());
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<VelocityComponent>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| VelocityComponent::fields(),
        get_field_value: |_world: &mut World, _entity: EntityId, _field_name: &str| {
            // Vec3 fields produce FieldMut::Unknown, so no field-level access
            None
        },
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          _value: FieldValue|
         -> Result<(), SceneToolError> {
            let _comp = world
                .get_component_mut::<VelocityComponent>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "VelocityComponent".to_string(),
                })?;
            Err(SceneToolError::FieldNotFound {
                component: "VelocityComponent".to_string(),
                field: field_name.to_string(),
            })
        },
    });
}

fn register_particle_emitter_component(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "ParticleEmitterComponent",
        has_component: |world: &World, entity: EntityId| {
            world
                .get_component::<ParticleEmitterComponent>(entity)
                .is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, ParticleEmitterComponent::default());
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<ParticleEmitterComponent>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| ParticleEmitterComponent::fields(),
        get_field_value: |_world: &mut World, _entity: EntityId, _field_name: &str| {
            // Complex fields (EmitterConfig, Option<EmitterHandle>, Vec<u32>, Option<f32>)
            // don't map to useful FieldMut variants, so no field-level access.
            None
        },
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          _value: FieldValue|
         -> Result<(), SceneToolError> {
            let _comp = world
                .get_component_mut::<ParticleEmitterComponent>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "ParticleEmitterComponent".to_string(),
                })?;
            Err(SceneToolError::FieldNotFound {
                component: "ParticleEmitterComponent".to_string(),
                field: field_name.to_string(),
            })
        },
    });
}

fn register_audio_emitter(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "AudioEmitter",
        has_component: |world: &World, entity: EntityId| {
            world.get_component::<AudioEmitter>(entity).is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, AudioEmitter::new(""));
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<AudioEmitter>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| AudioEmitter::fields(),
        get_field_value: |world: &mut World, entity: EntityId, field_name: &str| {
            let comp = world.get_component_mut::<AudioEmitter>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::F32(v) => FieldValue::F32(*v),
                FieldMut::Bool(v) => FieldValue::Bool(*v),
                FieldMut::String(v) => FieldValue::String(v.clone()),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
            let comp = world
                .get_component_mut::<AudioEmitter>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "AudioEmitter".to_string(),
                })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "AudioEmitter".to_string(),
                        field: field_name.to_string(),
                    })?;
            match (field_mut, value) {
                (FieldMut::F32(ref mut target), FieldValue::F32(v)) => {
                    **target = v;
                    Ok(())
                }
                (FieldMut::Bool(ref mut target), FieldValue::Bool(v)) => {
                    **target = v;
                    Ok(())
                }
                (FieldMut::String(ref mut target), FieldValue::String(v)) => {
                    **target = v;
                    Ok(())
                }
                (_, v) => Err(field_type_mismatch(field_name, "f32/bool/String", v)),
            }
        },
    });
}

fn register_collider_shape(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "ColliderShape",
        has_component: |world: &World, entity: EntityId| {
            world.get_component::<ColliderShape>(entity).is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(
                entity,
                ColliderShape::Sphere(katla_physics::SphereShape::new(0.5)),
            );
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<ColliderShape>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| Vec::new(),
        get_field_value: |_world: &mut World, _entity: EntityId, _field_name: &str| None,
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          _value: FieldValue|
         -> Result<(), SceneToolError> {
            let _comp = world
                .get_component_mut::<ColliderShape>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "ColliderShape".to_string(),
                })?;
            Err(SceneToolError::FieldNotFound {
                component: "ColliderShape".to_string(),
                field: field_name.to_string(),
            })
        },
    });
}

fn register_collision_filter(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "CollisionFilter",
        has_component: |world: &World, entity: EntityId| {
            world.get_component::<CollisionFilter>(entity).is_some()
        },
        create_default: |world: &mut World, entity: EntityId| {
            world.add_component(entity, CollisionFilter::default());
        },
        remove_component: |world: &mut World, entity: EntityId| {
            world.remove_component::<CollisionFilter>(entity);
        },
        get_fields: |_world: &World, _entity: EntityId| Vec::new(),
        get_field_value: |_world: &mut World, _entity: EntityId, _field_name: &str| None,
        set_field_value: |world: &mut World,
                          entity: EntityId,
                          field_name: &str,
                          _value: FieldValue|
         -> Result<(), SceneToolError> {
            let _comp = world
                .get_component_mut::<CollisionFilter>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "CollisionFilter".to_string(),
                })?;
            Err(SceneToolError::FieldNotFound {
                component: "CollisionFilter".to_string(),
                field: field_name.to_string(),
            })
        },
    });
}

pub(crate) fn build_editor_component_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_name_component(&mut registry);
    register_point_light(&mut registry);
    register_mass_component(&mut registry);
    register_drag_component(&mut registry);
    register_perspective_component(&mut registry);
    register_directional_light(&mut registry);
    register_script_component(&mut registry);
    register_velocity_component(&mut registry);
    register_particle_emitter_component(&mut registry);
    register_audio_emitter(&mut registry);
    register_collider_shape(&mut registry);
    register_collision_filter(&mut registry);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_contains_expected_types() {
        let registry = build_editor_component_registry();
        assert!(registry.is_registered("NameComponent"));
        assert!(registry.is_registered("PointLight"));
        assert!(registry.is_registered("MassComponent"));
        assert!(registry.is_registered("DragComponent"));
        assert!(registry.is_registered("PerspectiveComponent"));
        assert!(registry.is_registered("DirectionalLight"));
        assert!(registry.is_registered("ScriptComponent"));
        assert!(registry.is_registered("VelocityComponent"));
        assert!(registry.is_registered("ParticleEmitterComponent"));
    }

    #[test]
    fn test_registry_excludes_complex_types() {
        let registry = build_editor_component_registry();
        assert!(!registry.is_registered("TransformComponent"));
        assert!(!registry.is_registered("DrawableComponent"));
    }

    #[test]
    fn test_name_component_field_access() {
        let mut world = World::new();
        let registry = build_editor_component_registry();
        let entity = world.create_entity();
        world.add_component(entity, NameComponent::new("Test"));

        let entry = registry.get("NameComponent").unwrap();
        assert!((entry.has_component)(&world, entity));

        let value = (entry.get_field_value)(&mut world, entity, "name").unwrap();
        assert_eq!(value.as_string(), Some("Test"));
    }

    #[test]
    fn test_point_light_f32_fields() {
        let mut world = World::new();
        let registry = build_editor_component_registry();
        let entity = world.create_entity();
        world.add_component(entity, PointLight::new([1.0, 0.5, 0.0], 10.0, 50.0));

        let entry = registry.get("PointLight").unwrap();
        let intensity = (entry.get_field_value)(&mut world, entity, "intensity").unwrap();
        assert_eq!(intensity.as_f32(), Some(10.0));

        let range = (entry.get_field_value)(&mut world, entity, "range").unwrap();
        assert_eq!(range.as_f32(), Some(50.0));
    }

    #[test]
    fn test_set_field_value() {
        let mut world = World::new();
        let registry = build_editor_component_registry();
        let entity = world.create_entity();
        world.add_component(entity, MassComponent { mass: 1.0 });

        let entry = registry.get("MassComponent").unwrap();
        (entry.set_field_value)(&mut world, entity, "mass", FieldValue::F32(5.0)).unwrap();

        let comp = world.get_component::<MassComponent>(entity).unwrap();
        assert!((comp.mass - 5.0).abs() < 1e-6);
    }
}
