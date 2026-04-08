use katla_agent::serialize_scene_context;
use katla_ecs::EntityId;
use katla_ecs::agent::{Agent, AgentAction, AgentSession, Observation};
use katla_ecs::scene_tool::ComponentRegistry;
use katla_ecs::scene_tool::SceneOp;

pub(crate) fn run_scripted_agent(
    ops: Vec<SceneOp>,
    world: &mut katla_ecs::World,
    registry: &ComponentRegistry,
) -> Result<AgentSession, katla_ecs::scene_tool::SceneToolError> {
    let mut harness = katla_ecs::agent::AgentHarness::new();
    let mut agent = ScriptedAgent { ops, step: 0 };
    harness.run_sync_agent(&mut agent, world, registry)?;
    Ok(std::mem::take(harness.session_mut()))
}

struct ScriptedAgent {
    ops: Vec<SceneOp>,
    step: usize,
}

impl Agent for ScriptedAgent {
    fn observe(&mut self, _observation: &Observation) {}
    fn decide(&mut self) -> Option<SceneOp> {
        if self.step < self.ops.len() {
            let op = self.ops[self.step].clone();
            self.step += 1;
            Some(op)
        } else {
            None
        }
    }
    fn on_result(&mut self, _action: &AgentAction) {}
    fn name(&self) -> &str {
        "ScriptedAgent"
    }
}

pub(crate) fn get_scene_context_json(
    world: &mut katla_ecs::World,
    registry: &ComponentRegistry,
    selected_entity: Option<EntityId>,
) -> String {
    let ctx = serialize_scene_context(world, registry, selected_entity);
    serde_json::to_string_pretty(&ctx).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{MassComponent, NameComponent, PointLight};

    fn test_world_and_registry() -> (katla_ecs::World, ComponentRegistry) {
        (
            katla_ecs::World::new(),
            super::super::component_registry::build_editor_component_registry(),
        )
    }

    #[test]
    fn test_scripted_agent_spawn() {
        let (mut world, registry) = test_world_and_registry();
        let ops = vec![
            SceneOp::SpawnEntity {
                position: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                name: Some("Entity A".to_string()),
            },
            SceneOp::SpawnEntity {
                position: [1.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                name: Some("Entity B".to_string()),
            },
        ];

        let session = run_scripted_agent(ops, &mut world, &registry).unwrap();
        assert_eq!(session.action_count(), 2);
        assert!(world.entity_count() >= 2);
    }

    #[test]
    fn test_scripted_agent_set_field() {
        let (mut world, registry) = test_world_and_registry();
        let entity = world.create_entity();
        world.add_component(entity, NameComponent::new("Original"));

        let ops = vec![SceneOp::SetField {
            entity,
            component: "NameComponent".to_string(),
            field: "name".to_string(),
            value: serde_json::json!("Updated"),
        }];

        let session = run_scripted_agent(ops, &mut world, &registry).unwrap();
        assert_eq!(session.action_count(), 1);

        let name = world.get_component::<NameComponent>(entity).unwrap();
        assert_eq!(name.name, "Updated");
    }

    #[test]
    fn test_scripted_agent_undo() {
        let (mut world, registry) = test_world_and_registry();

        let ops = vec![SceneOp::SpawnEntity {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            name: Some("TempEntity".to_string()),
        }];

        let mut session = run_scripted_agent(ops, &mut world, &registry).unwrap();
        assert!(world.entity_count() > 0);

        session.undo_all(&mut world).unwrap();
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn test_scene_context_json() {
        let (mut world, registry) = test_world_and_registry();
        let entity = world.create_entity();
        world.add_component(entity, NameComponent::new("TestEntity"));
        world.add_component(entity, PointLight::new([1.0, 0.0, 0.0], 5.0, 20.0));
        world.add_component(entity, MassComponent { mass: 2.5 });

        let json = get_scene_context_json(&mut world, &registry, Some(entity));
        assert!(json.contains("TestEntity"));
        assert!(json.contains("entity_count"));
    }
}
