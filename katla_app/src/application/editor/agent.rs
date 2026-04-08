use katla_agent::serialize_scene_context;
use katla_ecs::EntityId;
use katla_ecs::agent::{Agent, AgentAction, AgentSession, Observation};
use katla_ecs::scene_tool::ComponentRegistry;
use katla_ecs::scene_tool::SceneOp;
use katla_math::Vec3;

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

/// Process a co-creator chat request from the user.
///
/// This is a placeholder implementation that does simple pattern matching
/// on the user's text input and performs demo actions (spawning entities).
/// The actual LLM integration will be wired in later.
pub(crate) fn process_co_creator_request(app: &mut super::super::Application, text: &str) {
    let lower = text.to_lowercase();

    // Simple pattern matching for demo purposes
    if lower.contains("cube") || lower.contains("spawn") || lower.contains("create") {
        let count = extract_count(&lower);
        if count <= 1 {
            app.spawn_test_cube([0.0, 0.5, 0.0], [1.0, 1.0, 1.0]);
            app.editor
                .editor_ui
                .co_creator
                .add_assistant_message("Spawned a cube at the origin.");
        } else {
            let n = count.min(10);
            for i in 0..n {
                let angle = (i as f32 / n as f32) * std::f32::consts::TAU;
                let x = angle.cos() * 3.0;
                let z = angle.sin() * 3.0;
                app.spawn_test_cube([x, 0.5, z], [1.0, 1.0, 1.0]);
            }
            app.editor
                .editor_ui
                .co_creator
                .add_assistant_message(&format!("Spawned {} cubes in a ring formation.", n));
        }
    } else if lower.contains("sphere") {
        app.spawn_sphere([0.0, 0.7, 0.0], 0.7, 32, 16);
        app.editor
            .editor_ui
            .co_creator
            .add_assistant_message("Spawned a sphere at the origin.");
    } else if lower.contains("light") {
        use crate::components::{PointLight, TransformComponent};
        let entity = app.world.create_entity();
        app.world.add_component(
            entity,
            TransformComponent::from_position(Vec3::new(0.0, 3.0, 0.0)),
        );
        app.world
            .add_component(entity, PointLight::new([1.0, 1.0, 0.9], 10.0, 20.0));
        app.attach_billboard_icon(
            entity,
            crate::components::billboard::BillboardIcon::Lightbulb,
        );
        app.editor
            .editor_ui
            .co_creator
            .add_assistant_message("Spawned a point light at (0, 3, 0).");
    } else if lower.contains("help") {
        app.editor.editor_ui.co_creator.add_assistant_message(
            "I can help you build your scene! Try: 'spawn a cube', 'create 5 cubes', 'add a sphere', 'add a light'.",
        );
    } else {
        app.editor.editor_ui.co_creator.add_assistant_message(
            &format!("I understood: \"{}\". Try 'help' for available commands, or describe what you'd like to create.", text),
        );
    }
}

/// Extract a count from text like "spawn 5 cubes" or "create 3 entities".
fn extract_count(text: &str) -> usize {
    for word in text.split_whitespace() {
        if let Ok(n) = word.parse::<usize>() {
            return n;
        }
        // Handle written numbers
        match word {
            "one" => return 1,
            "two" => return 2,
            "three" => return 3,
            "four" => return 4,
            "five" => return 5,
            "six" => return 6,
            "seven" => return 7,
            "eight" => return 8,
            "nine" => return 9,
            "ten" => return 10,
            _ => {}
        }
    }
    0
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
