use crate::llm::ToolDefinition;

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
    }
}
