use crate::inspect::{FieldMut, Inspect};
use crate::scene_tool::{
    ComponentRegistry, ComponentRegistryEntry, FieldValue, SceneOp, SceneToolError,
};
use crate::{Component, World};

use super::Agent;
use super::harness::{AgentHarness, AgentMessage, HarnessMessage};
use super::session::{AgentAction, Observation};

// --- Test-only components ---

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

// --- Mock Agent ---

struct MockAgent {
    steps: Vec<Option<SceneOp>>,
    step: usize,
    pub observations: Vec<String>,
}

impl MockAgent {
    fn new(steps: Vec<Option<SceneOp>>) -> Self {
        Self {
            steps,
            step: 0,
            observations: Vec::new(),
        }
    }
}

impl Agent for MockAgent {
    fn observe(&mut self, observation: &Observation) {
        self.observations.push(observation.scene_summary.clone());
    }

    fn decide(&mut self) -> Option<SceneOp> {
        if self.step < self.steps.len() {
            let op = self.steps[self.step].clone();
            self.step += 1;
            op
        } else {
            None
        }
    }

    fn on_result(&mut self, _action: &AgentAction) {}

    fn name(&self) -> &str {
        "MockAgent"
    }
}

// --- Registry helpers ---

fn build_test_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();

    // TestTransform
    registry.register(ComponentRegistryEntry {
        type_name: "TestTransform",
        has_component: |world: &crate::World, entity: crate::EntityId| {
            world.get_component::<TestTransform>(entity).is_some()
        },
        create_default: |world: &mut crate::World, entity: crate::EntityId| {
            world.add_component(entity, TestTransform::default());
        },
        get_fields: |_world: &crate::World, _entity: crate::EntityId| TestTransform::fields(),
        get_field_value: |world: &mut crate::World, entity: crate::EntityId, field_name: &str| {
            let comp = world.get_component_mut::<TestTransform>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::F32(v) => FieldValue::F32(*v),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut crate::World,
                          entity: crate::EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
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
        has_component: |world: &crate::World, entity: crate::EntityId| {
            world.get_component::<TestName>(entity).is_some()
        },
        create_default: |world: &mut crate::World, entity: crate::EntityId| {
            world.add_component(entity, TestName::default());
        },
        get_fields: |_world: &crate::World, _entity: crate::EntityId| TestName::fields(),
        get_field_value: |world: &mut crate::World, entity: crate::EntityId, field_name: &str| {
            let comp = world.get_component_mut::<TestName>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::String(v) => FieldValue::String(v.clone()),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut crate::World,
                          entity: crate::EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
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
        has_component: |world: &crate::World, entity: crate::EntityId| {
            world.get_component::<TestLight>(entity).is_some()
        },
        create_default: |world: &mut crate::World, entity: crate::EntityId| {
            world.add_component(entity, TestLight::default());
        },
        get_fields: |_world: &crate::World, _entity: crate::EntityId| TestLight::fields(),
        get_field_value: |world: &mut crate::World, entity: crate::EntityId, field_name: &str| {
            let comp = world.get_component_mut::<TestLight>(entity)?;
            let field_mut = comp.field_mut(field_name)?;
            Some(match field_mut {
                FieldMut::F32(v) => FieldValue::F32(*v),
                FieldMut::Bool(v) => FieldValue::Bool(*v),
                _ => FieldValue::Unknown,
            })
        },
        set_field_value: |world: &mut crate::World,
                          entity: crate::EntityId,
                          field_name: &str,
                          value: FieldValue|
         -> Result<(), SceneToolError> {
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
fn test_sync_agent_spawns_entities() {
    let mut world = World::new();
    let registry = build_test_registry();

    let mut agent = MockAgent::new(vec![
        Some(SceneOp::SpawnEntity {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: Some("A".to_string()),
        }),
        Some(SceneOp::SpawnEntity {
            position: [1.0, 0.0, 0.0],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: Some("B".to_string()),
        }),
        Some(SceneOp::SpawnEntity {
            position: [2.0, 0.0, 0.0],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: Some("C".to_string()),
        }),
    ]);

    let mut harness = AgentHarness::new();
    harness
        .run_sync_agent(&mut agent, &mut world, &registry)
        .unwrap();

    assert_eq!(world.entity_count(), 3);
}

#[test]
fn test_sync_agent_set_field() {
    let mut world = World::new();
    let registry = build_test_registry();

    let entity = world.create_entity();
    world.add_component(
        entity,
        TestLight {
            intensity: 0.5,
            enabled: true,
        },
    );

    let mut agent = MockAgent::new(vec![Some(SceneOp::SetField {
        entity,
        component: "TestLight".to_string(),
        field: "intensity".to_string(),
        value: serde_json::json!(2.5),
    })]);

    let mut harness = AgentHarness::new();
    harness
        .run_sync_agent(&mut agent, &mut world, &registry)
        .unwrap();

    let light = world.get_component::<TestLight>(entity).unwrap();
    assert_eq!(light.intensity, 2.5);
}

#[test]
fn test_sync_agent_session_actions() {
    let mut world = World::new();
    let registry = build_test_registry();

    let mut agent = MockAgent::new(vec![
        Some(SceneOp::SpawnEntity {
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: None,
        }),
        Some(SceneOp::SpawnEntity {
            position: [1.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: None,
        }),
    ]);

    let mut harness = AgentHarness::new();
    harness
        .run_sync_agent(&mut agent, &mut world, &registry)
        .unwrap();

    assert_eq!(harness.session().action_count(), 2);
}

#[test]
fn test_session_undo_last() {
    let mut world = World::new();
    let registry = build_test_registry();

    let mut agent = MockAgent::new(vec![
        Some(SceneOp::SpawnEntity {
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: Some("Keep".to_string()),
        }),
        Some(SceneOp::SpawnEntity {
            position: [1.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: Some("Undo".to_string()),
        }),
    ]);

    let mut harness = AgentHarness::new();
    harness
        .run_sync_agent(&mut agent, &mut world, &registry)
        .unwrap();

    assert_eq!(world.entity_count(), 2);

    harness.session_mut().undo_last(&mut world).unwrap();
    assert_eq!(world.entity_count(), 1);
}

#[test]
fn test_session_undo_all() {
    let mut world = World::new();
    let registry = build_test_registry();

    let mut agent = MockAgent::new(vec![
        Some(SceneOp::SpawnEntity {
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: None,
        }),
        Some(SceneOp::SpawnEntity {
            position: [1.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: None,
        }),
        Some(SceneOp::SpawnEntity {
            position: [2.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: None,
        }),
    ]);

    let mut harness = AgentHarness::new();
    harness
        .run_sync_agent(&mut agent, &mut world, &registry)
        .unwrap();

    assert_eq!(world.entity_count(), 3);

    harness.session_mut().undo_all(&mut world).unwrap();
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn test_sync_agent_query() {
    let mut world = World::new();
    let registry = build_test_registry();

    for _ in 0..3 {
        let e = world.create_entity();
        world.add_component(e, TestLight::default());
    }

    let mut agent = MockAgent::new(vec![Some(SceneOp::QueryEntities {
        component_filter: Some("TestLight".to_string()),
        name_filter: None,
        position: None,
        radius: None,
        limit: None,
    })]);

    let mut harness = AgentHarness::new();
    harness
        .run_sync_agent(&mut agent, &mut world, &registry)
        .unwrap();

    let actions = harness.session().actions();
    assert_eq!(actions.len(), 1);
    let result = actions[0].result.as_ref().unwrap();
    assert_eq!(result.affected_entities.len(), 3);
}

#[test]
fn test_agent_finishes() {
    let mut world = World::new();
    let registry = build_test_registry();

    let mut agent = MockAgent::new(vec![]);

    let mut harness = AgentHarness::new();
    harness
        .run_sync_agent(&mut agent, &mut world, &registry)
        .unwrap();

    assert!(harness.session().finished);
    assert_eq!(harness.session().action_count(), 0);
}

#[test]
fn test_harness_channels() {
    let mut world = World::new();
    let registry = build_test_registry();

    let mut harness = AgentHarness::new();
    let (agent_tx, harness_rx) = harness.channels();

    agent_tx
        .send(AgentMessage::ExecuteOp(SceneOp::SpawnEntity {
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: Some("ChanneledEntity".to_string()),
        }))
        .unwrap();

    agent_tx.send(AgentMessage::Finished).unwrap();

    let processed = harness.tick(&mut world, &registry).unwrap();
    assert_eq!(processed, 1);

    let processed2 = harness.tick(&mut world, &registry).unwrap();
    assert_eq!(processed2, 0);
    assert!(harness.session().finished);

    assert_eq!(world.entity_count(), 1);

    let msg = harness_rx.try_recv().unwrap();
    match msg {
        HarnessMessage::Result(_id, result) => {
            assert!(result.is_ok());
            let tool_result = result.unwrap();
            assert_eq!(tool_result.affected_entities.len(), 1);
        }
        HarnessMessage::Observation(_) => panic!("Expected Result, got Observation"),
        HarnessMessage::Shutdown => panic!("Expected Result, got Shutdown"),
    }
}

#[test]
fn test_observation_builder() {
    let mut world = World::new();
    let registry = build_test_registry();

    let e = world.create_entity();
    world.add_component(e, TestLight::default());

    let obs = super::observation::build_observation(&world, &registry);
    assert_eq!(obs.entity_count, 1);
    assert!(obs.scene_summary.contains("TestLight: 1"));
}

#[test]
fn test_agent_observes_each_step() {
    let mut world = World::new();
    let registry = build_test_registry();

    let mut agent = MockAgent::new(vec![
        Some(SceneOp::SpawnEntity {
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: None,
        }),
        Some(SceneOp::SpawnEntity {
            position: [1.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            name: None,
        }),
    ]);

    let mut harness = AgentHarness::new();
    harness
        .run_sync_agent(&mut agent, &mut world, &registry)
        .unwrap();

    // 2 spawn actions + 1 final None decision = 3 observe calls
    assert!(agent.observations.len() >= 3);
}
