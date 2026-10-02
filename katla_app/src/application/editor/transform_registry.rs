//! Scalar transform fields shared by scene commands and undo snapshots.
use crate::components::{TransformComponent, TransformDirty};
use katla_ecs::inspect::{FieldConstraints, FieldInfo, FieldKind};
use katla_ecs::scene_tool::{
    ComponentRegistry, ComponentRegistryEntry, FieldValue, SceneToolError,
};
use katla_math::Quat;

const FIELDS: [&str; 9] = [
    "x", "y", "z", "scale_x", "scale_y", "scale_z", "rot_x", "rot_y", "rot_z",
];

pub(super) fn register(registry: &mut ComponentRegistry) {
    registry.register(ComponentRegistryEntry {
        type_name: "TransformComponent",
        has_component: |world, entity| world.get_component::<TransformComponent>(entity).is_some(),
        create_default: |world, entity| {
            world.add_component(entity, TransformComponent::default());
        },
        remove_component: |world, entity| {
            world.remove_component::<TransformComponent>(entity);
        },
        get_fields: |_, _| {
            FIELDS
                .iter()
                .map(|&name| FieldInfo {
                    name,
                    display_name: name,
                    type_name: "f32",
                    kind: FieldKind::Float,
                    constraints: FieldConstraints::default(),
                })
                .collect()
        },
        get_field_value: |world, entity, field| {
            let transform = &world.get_component::<TransformComponent>(entity)?.transform;
            let index = FIELDS.iter().position(|&name| name == field)?;
            let euler = transform.rotation.to_euler();
            Some(FieldValue::F32(match index {
                0..=2 => transform.position[index],
                3..=5 => transform.scale[index - 3],
                _ => [euler.0, euler.1, euler.2][index - 6],
            }))
        },
        set_field_value: |world, entity, field, value| {
            let index = FIELDS
                .iter()
                .position(|&name| name == field)
                .ok_or_else(|| SceneToolError::FieldNotFound {
                    component: "TransformComponent".into(),
                    field: field.into(),
                })?;
            let v = value.as_f32().filter(|v| v.is_finite()).ok_or_else(|| {
                SceneToolError::InvalidFieldValue {
                    field: field.into(),
                    expected_type: "finite f32".into(),
                    got: value.type_name().into(),
                }
            })?;
            let transform = &mut world
                .get_component_mut::<TransformComponent>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "TransformComponent".into(),
                })?
                .transform;
            match index {
                0..=2 => transform.position[index] = v,
                3..=5 => transform.scale[index - 3] = v,
                _ => {
                    let e = transform.rotation.to_euler();
                    let mut angles = [e.0, e.1, e.2];
                    angles[index - 6] = v;
                    transform.rotation = Quat::from_euler(angles[0], angles[1], angles[2]);
                }
            }
            world.add_component(entity, TransformDirty);
            Ok(())
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_ecs::{
        World,
        scene_tool::{SceneOp, SceneToolExecutor},
    };
    #[test]
    fn test_widen_door_undo_and_reject_nonfinite() {
        let mut world = World::new();
        let entity = world.spawn((TransformComponent::default(),));
        let mut registry = ComponentRegistry::new();
        register(&mut registry);
        let (_, mut undo) = SceneToolExecutor::execute(
            SceneOp::SetField {
                entity,
                component: "TransformComponent".into(),
                field: "scale_x".into(),
                value: serde_json::json!(1.5),
            },
            &mut world,
            &registry,
        )
        .unwrap();
        assert_eq!(
            world
                .get_component::<TransformComponent>(entity)
                .unwrap()
                .transform
                .scale
                .x(),
            1.5
        );
        undo.undo_all(&mut world).unwrap();
        assert_eq!(
            world
                .get_component::<TransformComponent>(entity)
                .unwrap()
                .transform
                .scale
                .x(),
            1.0
        );
        assert!(
            (registry.get("TransformComponent").unwrap().set_field_value)(
                &mut world,
                entity,
                "x",
                FieldValue::F32(f32::INFINITY)
            )
            .is_err()
        );
    }
}
