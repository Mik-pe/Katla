use std::sync::Arc;

use katla_agent::serialize_scene_context;
use katla_agent::{ChatMessage, FinishReason, MessageRole, OpenAiProvider, ToolDefinition};
use katla_ecs::EntityId;
use katla_ecs::agent::{Agent, AgentAction, AgentSession, Observation};
use katla_ecs::scene_tool::ComponentRegistry;
use katla_ecs::scene_tool::SceneOp;
use katla_math::Vec3;
use log::warn;

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
/// If the LLM is configured and enabled, sends the message to the LLM backend
/// using streaming. Falls back to local pattern matching when disabled.
pub(crate) fn process_co_creator_request(app: &mut super::super::Application, text: &str) {
    if app.editor.llm_config.is_enabled() {
        submit_llm_stream_request(app, text);
    } else {
        process_local_request(app, text);
    }
}

/// Submit a user message to the LLM for streaming processing.
fn submit_llm_stream_request(app: &mut super::super::Application, text: &str) {
    let Some(ref bridge) = app.editor.async_bridge else {
        app.editor
            .editor_ui
            .co_creator
            .add_system_message("LLM runtime is not available.");
        return;
    };

    // Build conversation: system prompt + scene context + history + new message
    let mut system_content = katla_agent::co_creator::build_system_prompt();
    system_content.push_str("\n\n## Current Scene Context\n```json\n");
    system_content.push_str(&get_scene_context_json(
        &mut app.world,
        &app.editor.component_registry,
        app.editor.editor_ui.selected_entity,
    ));
    system_content.push_str("\n```");

    let mut messages = vec![ChatMessage {
        role: MessageRole::System,
        content: system_content,
        tool_calls: None,
    }];

    // Append existing conversation history
    messages.extend(app.editor.llm_conversation.iter().cloned());

    // Append the new user message
    let user_message = ChatMessage {
        role: MessageRole::User,
        content: text.to_string(),
        tool_calls: None,
    };
    messages.push(user_message.clone());
    app.editor.llm_conversation.push(user_message);

    // Build tool definitions
    let tools = build_tool_definitions();

    // Create provider and submit stream
    match OpenAiProvider::from_config(&app.editor.llm_config) {
        Ok(provider) => {
            let pending = bridge.submit_chat_stream(Arc::new(provider), messages, tools);
            app.editor.pending_llm_stream = Some(pending);
        }
        Err(e) => {
            warn!("Failed to create LLM provider: {}", e);
            app.editor
                .editor_ui
                .co_creator
                .add_system_message(&format!("LLM configuration error: {}", e));
        }
    }
}

/// Poll for streaming LLM chunks. Called each frame from the editor loop.
pub(crate) fn poll_llm_stream(app: &mut super::super::Application) {
    let Some(pending) = app.editor.pending_llm_stream.as_mut() else {
        return;
    };

    let chunks = pending.poll_chunks();
    for chunk in chunks {
        match chunk {
            Ok(stream_chunk) => {
                if !stream_chunk.content_delta.is_empty() {
                    app.editor
                        .editor_ui
                        .co_creator
                        .append_streaming_text(&stream_chunk.content_delta);
                }
                if stream_chunk.finish_reason.is_some() {
                    let full_text = app
                        .editor
                        .editor_ui
                        .co_creator
                        .messages
                        .last()
                        .map(|m| m.text.clone())
                        .unwrap_or_default();

                    app.editor.llm_conversation.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: full_text,
                        tool_calls: None,
                    });

                    match stream_chunk.finish_reason {
                        Some(FinishReason::Length) => {
                            app.editor
                                .editor_ui
                                .co_creator
                                .add_system_message("Response was truncated due to token limit.");
                        }
                        Some(FinishReason::ToolCall) => {
                            app.editor
                                .editor_ui
                                .co_creator
                                .add_system_message("(Tool calling not yet wired)");
                        }
                        _ => {}
                    }

                    app.editor.editor_ui.co_creator.finalize_streaming();
                }
            }
            Err(e) => {
                app.editor
                    .editor_ui
                    .co_creator
                    .add_system_message(&format!("LLM error: {}", e));
            }
        }
    }

    if pending.is_done() {
        app.editor.pending_llm_stream = None;
    }
}

/// Build tool definitions for the LLM's function calling.
fn build_tool_definitions() -> Vec<ToolDefinition> {
    use serde_json::json;

    vec![
        ToolDefinition {
            name: "spawn_entity".to_string(),
            description: "Spawn a new entity in the scene with a transform.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "position": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Position [x, y, z]"
                    },
                    "rotation": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Euler rotation [x, y, z] in degrees"
                    },
                    "scale": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Scale [x, y, z]"
                    },
                    "name": {
                        "type": "string",
                        "description": "Optional entity name"
                    }
                },
                "required": ["position"]
            }),
        },
        ToolDefinition {
            name: "destroy_entity".to_string(),
            description: "Remove an entity from the scene.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entity_id": {
                        "type": "integer",
                        "description": "The entity ID to destroy"
                    }
                },
                "required": ["entity_id"]
            }),
        },
        ToolDefinition {
            name: "set_field".to_string(),
            description: "Set a component field value on an entity.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "integer" },
                    "component": { "type": "string", "description": "Component type name" },
                    "field": { "type": "string", "description": "Field name" },
                    "value": { "description": "New value" }
                },
                "required": ["entity_id", "component", "field", "value"]
            }),
        },
        ToolDefinition {
            name: "query_entities".to_string(),
            description: "Query entities by component type.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "component_filter": {
                        "type": "string",
                        "description": "Component type name to filter by"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max entities to return"
                    }
                },
                "required": ["component_filter"]
            }),
        },
        ToolDefinition {
            name: "get_scene_hierarchy".to_string(),
            description: "Get the full scene hierarchy as JSON.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "duplicate_entity".to_string(),
            description: "Duplicate an entity with an optional position offset.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "integer" },
                    "position_offset": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Offset [x, y, z] from original position"
                    }
                },
                "required": ["entity_id"]
            }),
        },
    ]
}

/// Local pattern-matching fallback when no LLM is configured.
fn process_local_request(app: &mut super::super::Application, text: &str) {
    let lower = text.to_lowercase();

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
            "I can help you build your scene! Try: 'spawn a cube', 'create 5 cubes', 'add a sphere', 'add a light'.\n\
             \n\
             To connect me to an AI, configure an LLM provider in Edit > Preferences > AI tab.",
        );
    } else {
        app.editor
            .editor_ui
            .co_creator
            .add_assistant_message(&format!(
                "I understood: \"{}\". Try 'help' for available commands.\n\
             \n\
             For smarter responses, configure an LLM provider in Edit > Preferences > AI tab.",
                text
            ));
    }
}

/// Extract a count from text like "spawn 5 cubes" or "create 3 entities".
fn extract_count(text: &str) -> usize {
    for word in text.split_whitespace() {
        if let Ok(n) = word.parse::<usize>() {
            return n;
        }
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
