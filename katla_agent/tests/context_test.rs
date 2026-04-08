use katla_agent::{SceneContext, serialize_scene_context};
use katla_ecs::scene_tool::{ComponentRegistry, ComponentRegistryEntry, FieldValue};
use katla_ecs::{Component, World};

#[derive(Component, Default)]
struct TransformComponent {
    x: f32,
    y: f32,
    z: f32,
}

fn test_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();

    registry.register(ComponentRegistryEntry {
        type_name: "TransformComponent",
        has_component: |world, id| world.get_component::<TransformComponent>(id).is_some(),
        create_default: |world, id| {
            world.add_component(id, TransformComponent::default());
        },
        get_fields: |_world, _id| {
            vec![
                katla_ecs::inspect::FieldInfo {
                    name: "x",
                    display_name: "X",
                    type_name: "f32",
                    kind: katla_ecs::inspect::FieldKind::Float,
                    constraints: katla_ecs::inspect::FieldConstraints::default(),
                },
                katla_ecs::inspect::FieldInfo {
                    name: "y",
                    display_name: "Y",
                    type_name: "f32",
                    kind: katla_ecs::inspect::FieldKind::Float,
                    constraints: katla_ecs::inspect::FieldConstraints::default(),
                },
                katla_ecs::inspect::FieldInfo {
                    name: "z",
                    display_name: "Z",
                    type_name: "f32",
                    kind: katla_ecs::inspect::FieldKind::Float,
                    constraints: katla_ecs::inspect::FieldConstraints::default(),
                },
            ]
        },
        get_field_value: |world, id, field| {
            let comp = world.get_component::<TransformComponent>(id)?;
            match field {
                "x" => Some(FieldValue::F32(comp.x)),
                "y" => Some(FieldValue::F32(comp.y)),
                "z" => Some(FieldValue::F32(comp.z)),
                _ => None,
            }
        },
        set_field_value: |world, id, field, value| {
            let comp = world
                .get_component_mut::<TransformComponent>(id)
                .ok_or_else(
                    || katla_ecs::scene_tool::SceneToolError::ComponentNotFound {
                        entity: id,
                        component: "TransformComponent".to_string(),
                    },
                )?;
            match field {
                "x" => comp.x = value.as_f32().unwrap(),
                "y" => comp.y = value.as_f32().unwrap(),
                "z" => comp.z = value.as_f32().unwrap(),
                _ => {}
            }
            Ok(())
        },
    });

    registry
}

#[test]
fn test_serialize_scene_context_empty_world() {
    let mut world = World::new();
    let registry = test_registry();

    let ctx = serialize_scene_context(&mut world, &registry, None);

    assert_eq!(ctx.entity_count, 0);
    assert!(ctx.selected_entity.is_none());
    assert!(ctx.component_counts.is_empty());
}

#[test]
fn test_serialize_scene_context_with_entities() {
    let mut world = World::new();
    let registry = test_registry();

    let e1 = world.create_entity();
    world.add_component(
        e1,
        TransformComponent {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
    );

    let e2 = world.create_entity();
    world.add_component(
        e2,
        TransformComponent {
            x: 4.0,
            y: 5.0,
            z: 6.0,
        },
    );

    let ctx = serialize_scene_context(&mut world, &registry, Some(e1));

    assert_eq!(ctx.entity_count, 2);
    assert_eq!(ctx.component_counts.len(), 1);
    assert_eq!(ctx.component_counts[0].type_name, "TransformComponent");
    assert_eq!(ctx.component_counts[0].count, 2);

    let selected = ctx.selected_entity.unwrap();
    assert_eq!(selected.id, e1.id());
    assert_eq!(selected.components.len(), 1);
    assert_eq!(selected.components[0].type_name, "TransformComponent");
    assert_eq!(selected.components[0].fields.len(), 3);
    assert_eq!(selected.components[0].fields[0].name, "x");
}

#[test]
fn test_serialize_scene_context_no_selected_entity() {
    let mut world = World::new();
    let registry = test_registry();

    let e1 = world.create_entity();
    world.add_component(e1, TransformComponent::default());

    let ctx = serialize_scene_context(&mut world, &registry, None);

    assert_eq!(ctx.entity_count, 1);
    assert!(ctx.selected_entity.is_none());
    assert_eq!(ctx.component_counts.len(), 1);
}

#[test]
fn test_serialize_scene_context_invalid_entity() {
    let mut world = World::new();
    let registry = test_registry();

    let _e1 = world.create_entity();
    let e2 = world.create_entity();
    world.destroy_entity(e2);

    let ctx = serialize_scene_context(&mut world, &registry, Some(e2));

    assert_eq!(ctx.entity_count, 1);
    assert!(ctx.selected_entity.is_none());
}

#[test]
fn test_scene_context_serialization_roundtrip() {
    let mut world = World::new();
    let registry = test_registry();

    let e1 = world.create_entity();
    world.add_component(
        e1,
        TransformComponent {
            x: 10.0,
            y: 20.0,
            z: 30.0,
        },
    );

    let ctx = serialize_scene_context(&mut world, &registry, Some(e1));
    let json = serde_json::to_string(&ctx).unwrap();
    let parsed: SceneContext = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.entity_count, ctx.entity_count);
    assert_eq!(
        parsed.selected_entity.unwrap().id,
        ctx.selected_entity.unwrap().id
    );
}
