use serde::Deserialize;

use crate::llm::ToolDefinition;

/// Typed arguments for the `spawn_entity` tool.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SpawnEntityArgs {
    pub position: Option<[f32; 3]>,
    pub rotation: Option<[f32; 3]>,
    pub scale: Option<[f32; 3]>,
    pub name: Option<String>,
}

/// Typed arguments for the `destroy_entity` tool.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DestroyEntityArgs {
    pub entity_id: u64,
}

/// Typed arguments for the `set_field` tool.
#[derive(Debug, Clone, Deserialize)]
pub struct SetFieldArgs {
    pub entity_id: u64,
    pub component: String,
    pub field: String,
    pub value: serde_json::Value,
}

/// Typed arguments for the `query_entities` tool.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct QueryEntitiesArgs {
    pub component_filter: Option<String>,
    pub limit: Option<u64>,
}

/// Typed arguments for the `get_scene_hierarchy` tool.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct GetSceneHierarchyArgs {}

/// Typed arguments for the `duplicate_entity` tool.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DuplicateEntityArgs {
    pub entity_id: u64,
    pub position_offset: Option<[f32; 3]>,
}

/// Typed arguments for the `list_available_components` tool.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ListAvailableComponentsArgs {}

/// Typed arguments for the `add_component` tool.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AddComponentArgs {
    pub entity_id: u64,
    pub component: String,
}

/// Typed arguments for the `get_component_attributes` tool.
#[derive(Debug, Clone, Deserialize)]
pub struct GetComponentAttributesArgs {
    pub entity_id: u64,
    pub component: String,
}

/// Build tool definitions for the LLM's function calling.
pub fn build_tool_definitions() -> Vec<ToolDefinition> {
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
        ToolDefinition {
            name: "list_available_components".to_string(),
            description:
                "List all registered component types with their settable fields and types."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "add_component".to_string(),
            description: "Add a component with default values to an existing entity.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "integer", "description": "The entity ID to add the component to" },
                    "component": { "type": "string", "description": "Component type name" }
                },
                "required": ["entity_id", "component"]
            }),
        },
        ToolDefinition {
            name: "get_component_attributes".to_string(),
            description:
                "Get settable fields, types, and current values for a component on an entity."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "integer", "description": "The entity ID" },
                    "component": { "type": "string", "description": "Component type name" }
                },
                "required": ["entity_id", "component"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_tool_definitions() {
        let tools = build_tool_definitions();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|t| t.name == "spawn_entity"));
        assert!(tools.iter().any(|t| t.name == "destroy_entity"));
        assert!(tools.iter().any(|t| t.name == "set_field"));
        assert!(tools.iter().any(|t| t.name == "query_entities"));
        assert!(tools.iter().any(|t| t.name == "get_scene_hierarchy"));
        assert!(tools.iter().any(|t| t.name == "duplicate_entity"));
        assert!(tools.iter().any(|t| t.name == "list_available_components"));
        assert!(tools.iter().any(|t| t.name == "add_component"));
        assert!(tools.iter().any(|t| t.name == "get_component_attributes"));
    }
}
