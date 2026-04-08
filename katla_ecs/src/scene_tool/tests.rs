use crate::inspect::{FieldMut, Inspect};
use crate::scene_tool::registry::{ComponentRegistry, ComponentRegistryEntry, FieldValue};
use crate::scene_tool::{SceneOp, SceneToolError, SceneToolExecutor};
use crate::{Component, EntityId, World};

// --- Test-only components (derive(Component) generates Inspect impl when editor feature is on) ---

#[derive(Component, Default, Debug)]
struct TestTransform {
    x: f32,
    y: f32,
    z: f32,
    scale_x: f32,
    scale_y: f32,
    scale_z: f32,
    rot_x: f32,
    rot_y: f32,
    rot_z: f32,
}

#[derive(Component, Default, Debug)]
struct TestName {
    name: String,
}

#[derive(Component, Default, Debug)]
struct TestLight {
    intensity: f32,
    enabled: bool,
}

// --- Registry helpers ---

fn build_test_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();

    // TestTransform
    registry.register(ComponentRegistryEntry {
        type_name: "TestTransform",
        has_component: |world, entity| world.get_component::<TestTransform>(entity).is_some(),
        create_default: |world, entity| {
            world.add_component(entity, TestTransform::default());
        },
        get_fields: |_world, _entity| TestTransform::fields(),
        get_field_value: |world, entity, field_name| {
            let comp = world.get_component_mut::<TestTransform>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::F32(v) => FieldValue::F32(*v),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world, entity, field_name, value| {
            let comp = world
                .get_component_mut::<TestTransform>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "TestTransform".to_string(),
                })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "TestTransform".to_string(),
                        field: field_name.to_string(),
                    })?;
            match (field_mut, value) {
                (FieldMut::F32(ref mut target), FieldValue::F32(v)) => {
                    **target = v;
                    Ok(())
                }
                _ => Err(SceneToolError::InvalidFieldValue {
                    field: field_name.to_string(),
                    expected_type: "f32".to_string(),
                    got: "unsupported type".to_string(),
                }),
            }
        },
    });

    // TestName
    registry.register(ComponentRegistryEntry {
        type_name: "TestName",
        has_component: |world, entity| world.get_component::<TestName>(entity).is_some(),
        create_default: |world, entity| {
            world.add_component(entity, TestName::default());
        },
        get_fields: |_world, _entity| TestName::fields(),
        get_field_value: |world, entity, field_name| {
            let comp = world.get_component_mut::<TestName>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::String(v) => FieldValue::String(v.clone()),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world, entity, field_name, value| {
            let comp = world.get_component_mut::<TestName>(entity).ok_or_else(|| {
                SceneToolError::ComponentNotFound {
                    entity,
                    component: "TestName".to_string(),
                }
            })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "TestName".to_string(),
                        field: field_name.to_string(),
                    })?;
            match (field_mut, value) {
                (FieldMut::String(ref mut target), FieldValue::String(v)) => {
                    **target = v;
                    Ok(())
                }
                _ => Err(SceneToolError::InvalidFieldValue {
                    field: field_name.to_string(),
                    expected_type: "String".to_string(),
                    got: "unsupported type".to_string(),
                }),
            }
        },
    });

    // TestLight
    registry.register(ComponentRegistryEntry {
        type_name: "TestLight",
        has_component: |world, entity| world.get_component::<TestLight>(entity).is_some(),
        create_default: |world, entity| {
            world.add_component(entity, TestLight::default());
        },
        get_fields: |_world, _entity| TestLight::fields(),
        get_field_value: |world, entity, field_name| {
            let comp = world.get_component_mut::<TestLight>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::F32(v) => FieldValue::F32(*v),
                FieldMut::Bool(v) => FieldValue::Bool(*v),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world, entity, field_name, value| {
            let comp = world
                .get_component_mut::<TestLight>(entity)
                .ok_or_else(|| SceneToolError::ComponentNotFound {
                    entity,
                    component: "TestLight".to_string(),
                })?;
            let field_mut =
                comp.field_mut(field_name)
                    .ok_or_else(|| SceneToolError::FieldNotFound {
                        component: "TestLight".to_string(),
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
                _ => Err(SceneToolError::InvalidFieldValue {
                    field: field_name.to_string(),
                    expected_type: "unknown".to_string(),
                    got: "unsupported type".to_string(),
                }),
            }
        },
    });

    registry
}

// --- Tests ---

#[test]
fn test_spawn_entity_and_undo() {
    let mut world = World::new();
    let registry = build_test_registry();

    let op = SceneOp::SpawnEntity {
        position: [1.0, 2.0, 3.0],
        rotation: [0.0, 0.0, 0.0],
        scale: [1.0, 1.0, 1.0],
        name: Some("TestEntity".to_string()),
    };

    let (result, mut undo_group) = SceneToolExecutor::execute(op, &mut world, &registry).unwrap();

    assert!(result.success);
    assert_eq!(result.affected_entities.len(), 1);
    let entity = result.affected_entities[0];
    assert!(world.entity_exists(entity));

    // Undo should destroy the entity
    undo_group.undo_all(&mut world).unwrap();
    assert!(!world.entity_exists(entity));
}

#[test]
fn test_set_field_and_undo() {
    let mut world = World::new();
    let registry = build_test_registry();

    // Create entity with TestLight
    let entity = world.create_entity();
    world.add_component(
        entity,
        TestLight {
            intensity: 0.5,
            enabled: true,
        },
    );

    let op = SceneOp::SetField {
        entity,
        component: "TestLight".to_string(),
        field: "intensity".to_string(),
        value: serde_json::json!(1.5),
    };

    let (result, undo_group) = SceneToolExecutor::execute(op, &mut world, &registry).unwrap();
    assert!(result.success);

    // Verify field was changed
    let light = world.get_component::<TestLight>(entity).unwrap();
    assert_eq!(light.intensity, 1.5);

    // Undo should restore the old value
    let _ = light;
    let mut undo_group = undo_group;
    undo_group.undo_all(&mut world).unwrap();
    let light = world.get_component::<TestLight>(entity).unwrap();
    assert_eq!(light.intensity, 0.5);
}

#[test]
fn test_destroy_entity_and_undo() {
    let mut world = World::new();
    let registry = build_test_registry();

    let entity = world.create_entity();
    world.add_component(entity, TestTransform::default());
    world.add_component(
        entity,
        TestName {
            name: "ToDelete".to_string(),
        },
    );

    let op = SceneOp::DestroyEntity { entity };
    let (result, mut undo_group) = SceneToolExecutor::execute(op, &mut world, &registry).unwrap();

    assert!(result.success);
    assert!(!world.entity_exists(entity));

    // Undo — entity is recreated with restored components
    undo_group.undo_all(&mut world).unwrap();
    assert_eq!(world.entity_count(), 1);

    // Verify components were restored (entity has a new ID)
    let entities: Vec<EntityId> = world.entity_ids().collect();
    let new_entity = entities[0];
    let transform = world.get_component::<TestTransform>(new_entity).unwrap();
    // Transform was default when destroyed, so should be default after restore
    assert_eq!(transform.x, 0.0);
    let name_comp = world.get_component::<TestName>(new_entity).unwrap();
    assert_eq!(name_comp.name, "ToDelete");
}

#[test]
fn test_query_entities_by_component() {
    let mut world = World::new();
    let registry = build_test_registry();

    // Create entities with different components
    let e1 = world.create_entity();
    world.add_component(
        e1,
        TestLight {
            intensity: 1.0,
            enabled: true,
        },
    );

    let e2 = world.create_entity();
    world.add_component(
        e2,
        TestLight {
            intensity: 2.0,
            enabled: false,
        },
    );

    let e3 = world.create_entity();
    world.add_component(e3, TestTransform::default());

    let op = SceneOp::QueryEntities {
        component_filter: Some("TestLight".to_string()),
        name_filter: None,
        position: None,
        radius: None,
        limit: None,
    };

    let (result, _) = SceneToolExecutor::execute(op, &mut world, &registry).unwrap();
    assert!(result.success);
    assert_eq!(result.affected_entities.len(), 2);
    assert!(result.affected_entities.contains(&e1));
    assert!(result.affected_entities.contains(&e2));
    assert!(!result.affected_entities.contains(&e3));
}

#[test]
fn test_query_entities_with_limit() {
    let mut world = World::new();
    let registry = build_test_registry();

    for i in 0..5 {
        let e = world.create_entity();
        world.add_component(
            e,
            TestLight {
                intensity: i as f32,
                enabled: true,
            },
        );
    }

    let op = SceneOp::QueryEntities {
        component_filter: Some("TestLight".to_string()),
        name_filter: None,
        position: None,
        radius: None,
        limit: Some(3),
    };

    let (result, _) = SceneToolExecutor::execute(op, &mut world, &registry).unwrap();
    assert_eq!(result.affected_entities.len(), 3);
}

#[test]
fn test_duplicate_entity_and_undo() {
    let mut world = World::new();
    let registry = build_test_registry();

    let entity = world.create_entity();
    world.add_component(
        entity,
        TestLight {
            intensity: 5.0,
            enabled: true,
        },
    );
    world.add_component(
        entity,
        TestName {
            name: "Original".to_string(),
        },
    );

    let op = SceneOp::DuplicateEntity {
        entity,
        position_offset: Some([1.0, 0.0, 0.0]),
    };

    let (result, mut undo_group) = SceneToolExecutor::execute(op, &mut world, &registry).unwrap();
    assert!(result.success);

    let duplicate = result.affected_entities[0];
    assert!(world.entity_exists(entity));
    assert!(world.entity_exists(duplicate));
    assert_ne!(entity, duplicate);

    // Verify component was copied
    let dup_light = world.get_component::<TestLight>(duplicate).unwrap();
    assert_eq!(dup_light.intensity, 5.0);
    assert!(dup_light.enabled);

    // Undo should remove the duplicate
    undo_group.undo_all(&mut world).unwrap();
    assert!(world.entity_exists(entity));
    assert!(!world.entity_exists(duplicate));
}

#[test]
fn test_undo_group_multiple_spawns() {
    let mut world = World::new();
    let registry = build_test_registry();
    let mut group = crate::scene_tool::command::UndoGroup::new("Spawn 3 entities");

    let mut entities = Vec::new();
    for i in 0..3 {
        let op = SceneOp::SpawnEntity {
            position: [i as f32, 0.0, 0.0],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: Some(format!("Entity{i}")),
        };
        let (result, mut inner_group) =
            SceneToolExecutor::execute(op, &mut world, &registry).unwrap();
        assert!(result.success);
        entities.push(result.affected_entities[0]);
        // Move commands from inner group into the outer group
        while let Some(cmd) = inner_group.commands.pop() {
            group.commands.push(cmd);
        }
    }

    assert_eq!(world.entity_count(), 3);
    for e in &entities {
        assert!(world.entity_exists(*e));
    }

    // Undo all
    group.undo_all(&mut world).unwrap();
    for e in &entities {
        assert!(!world.entity_exists(*e));
    }
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn test_destroy_nonexistent_entity_error() {
    let mut world = World::new();
    let registry = build_test_registry();

    let fake_id = EntityId::from_raw(999999);
    let op = SceneOp::DestroyEntity { entity: fake_id };
    let result = SceneToolExecutor::execute(op, &mut world, &registry);
    assert!(result.is_err());
    match result {
        Err(SceneToolError::EntityNotFound(id)) => assert_eq!(id, fake_id),
        Err(other) => panic!("Expected EntityNotFound, got {other}"),
        Ok(_) => panic!("Expected error, got success"),
    }
}

#[test]
fn test_set_field_unregistered_component() {
    let mut world = World::new();
    let registry = build_test_registry();

    let entity = world.create_entity();
    world.add_component(entity, TestLight::default());

    let op = SceneOp::SetField {
        entity,
        component: "UnregisteredComponent".to_string(),
        field: "x".to_string(),
        value: serde_json::json!(1.0),
    };

    let result = SceneToolExecutor::execute(op, &mut world, &registry);
    assert!(result.is_err());
}

#[test]
fn test_get_scene_hierarchy() {
    let mut world = World::new();
    let registry = build_test_registry();

    for _ in 0..3 {
        world.create_entity();
    }

    let op = SceneOp::GetSceneHierarchy;
    let (result, _) = SceneToolExecutor::execute(op, &mut world, &registry).unwrap();
    assert!(result.success);
    assert_eq!(result.affected_entities.len(), 3);
}

#[test]
fn test_field_value_conversions() {
    assert_eq!(
        FieldValue::from_json(&serde_json::json!(42.0)).as_f32(),
        Some(42.0)
    );
    assert_eq!(
        FieldValue::from_json(&serde_json::json!(true)).as_bool(),
        Some(true)
    );
    assert_eq!(
        FieldValue::from_json(&serde_json::json!("hello")).as_string(),
        Some("hello")
    );
}

#[test]
fn test_registry_type_names() {
    let registry = build_test_registry();
    let names = registry.type_names();
    assert!(names.contains(&"TestTransform"));
    assert!(names.contains(&"TestName"));
    assert!(names.contains(&"TestLight"));
    assert!(registry.is_registered("TestTransform"));
    assert!(!registry.is_registered("Unregistered"));
}

#[test]
fn test_set_field_bool() {
    let mut world = World::new();
    let registry = build_test_registry();

    let entity = world.create_entity();
    world.add_component(
        entity,
        TestLight {
            intensity: 1.0,
            enabled: true,
        },
    );

    let op = SceneOp::SetField {
        entity,
        component: "TestLight".to_string(),
        field: "enabled".to_string(),
        value: serde_json::json!(false),
    };

    let (result, _undo_group) = SceneToolExecutor::execute(op, &mut world, &registry).unwrap();
    assert!(result.success);

    let light = world.get_component::<TestLight>(entity).unwrap();
    assert!(!light.enabled);
}

#[test]
fn test_duplicate_without_offset() {
    let mut world = World::new();
    let registry = build_test_registry();

    let entity = world.create_entity();
    world.add_component(
        entity,
        TestName {
            name: "Source".to_string(),
        },
    );

    let op = SceneOp::DuplicateEntity {
        entity,
        position_offset: None,
    };

    let (result, mut undo_group) = SceneToolExecutor::execute(op, &mut world, &registry).unwrap();
    assert!(result.success);

    let duplicate = result.affected_entities[0];
    let dup_name = world.get_component::<TestName>(duplicate).unwrap();
    assert_eq!(dup_name.name, "Source");

    undo_group.undo_all(&mut world).unwrap();
    assert!(!world.entity_exists(duplicate));
}
