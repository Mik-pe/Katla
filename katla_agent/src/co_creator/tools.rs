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
    pub shape: Option<String>,
    pub radius: Option<f32>,
    pub segments: Option<u32>,
    pub rings: Option<u32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub tube_radius: Option<f32>,
    pub tube_segments: Option<u32>,
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

/// Typed arguments for the `set_parent` tool.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SetParentArgs {
    pub entity_id: u64,
    pub parent_id: Option<u64>,
}

/// Typed arguments for the `spawn_model` tool.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SpawnModelArgs {
    pub path: String,
    pub position: Option<[f32; 3]>,
    pub default_animation: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ListResourcesArgs {
    pub path: Option<String>,
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadResourceArgs {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WriteResourceArgs {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CreateResourceArgs {
    pub path: String,
    pub template: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerateResourceArgs {
    pub path: String,
    pub resource_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoadSceneArgs {
    pub path: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SaveSceneArgs {
    pub path: Option<String>,
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
                    },
                    "shape": {
                        "type": "string",
                        "description": "Primitive shape: 'cube', 'sphere', 'plane', 'cylinder', 'cone', 'torus'. Default: 'cube'.",
                        "enum": ["cube", "sphere", "plane", "cylinder", "cone", "torus"]
                    },
                    "radius": {
                        "type": "number",
                        "description": "Radius for sphere, cylinder, cone, torus (default: 0.5)"
                    },
                    "segments": {
                        "type": "integer",
                        "description": "Longitudinal segments for sphere, cylinder, cone, torus"
                    },
                    "rings": {
                        "type": "integer",
                        "description": "Latitudinal rings for sphere"
                    },
                    "width": {
                        "type": "number",
                        "description": "Width for plane"
                    },
                    "height": {
                        "type": "number",
                        "description": "Height for cylinder, cone, plane"
                    },
                    "tube_radius": {
                        "type": "number",
                        "description": "Tube radius for torus"
                    },
                    "tube_segments": {
                        "type": "integer",
                        "description": "Tube segments for torus"
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
        ToolDefinition {
            name: "set_parent".to_string(),
            description:
                "Set or clear the parent of an entity. Pass null for parent_id to unparent."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entity_id": { "type": "integer", "description": "The entity to reparent" },
                    "parent_id": { "type": "integer", "description": "New parent entity ID, or null to clear" }
                },
                "required": ["entity_id"]
            }),
        },
        ToolDefinition {
            name: "list_resources".to_string(),
            description: "List resource files in a project directory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path relative to project root" },
                    "filter": { "type": "string", "description": "Optional file extension filter (e.g. 'json', 'katla')" }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "read_resource".to_string(),
            description: "Read a resource file's content as text.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path relative to project root" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "write_resource".to_string(),
            description: "Write content to an existing resource file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path relative to project root" },
                    "content": { "type": "string", "description": "New file content" }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "create_resource".to_string(),
            description: "Create a new resource file with optional template.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path relative to project root" },
                    "template": { "type": "string", "description": "Optional template name for content generation" },
                    "content": { "type": "string", "description": "Initial file content (if no template)" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "spawn_model".to_string(),
            description: "Spawn a GLTF model from the project's assets directory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the GLTF file relative to the assets directory (e.g., 'models/character.glb')"
                    },
                    "position": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Position [x, y, z] to spawn the model at"
                    },
                    "default_animation": {
                        "type": "string",
                        "description": "Optional name of the default animation to play"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "generate_resource".to_string(),
            description: "Generate a resource file from a natural language description. Creates particle systems, materials, or scenes based on descriptive keywords.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to project root (e.g. 'assets/particles/fire.json')"
                    },
                    "resource_type": {
                        "type": "string",
                        "enum": ["particle_system", "material", "scene"],
                        "description": "Type of resource to generate"
                    },
                    "description": {
                        "type": "string",
                        "description": "Natural language description of what to generate (e.g. 'a campfire with sparks', 'metallic blue material', 'empty night scene')"
                    }
                },
                "required": ["path", "resource_type", "description"]
            }),
        },
        ToolDefinition {
            name: "load_scene".to_string(),
            description: "Load a scene from a .katla file, replacing all entities in the current scene.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the .katla scene file relative to project root (e.g., 'assets/scenes/default.katla')"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "save_scene".to_string(),
            description: "Save the current scene to a .katla file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to save the scene file relative to project root. Defaults to 'assets/scenes/default.katla' if not specified."
                    }
                },
                "required": []
            }),
        },
    ]
}

#[cfg(all(test, feature = "llm-assistant"))]
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
        assert!(tools.iter().any(|t| t.name == "set_parent"));
        assert!(tools.iter().any(|t| t.name == "list_resources"));
        assert!(tools.iter().any(|t| t.name == "read_resource"));
        assert!(tools.iter().any(|t| t.name == "write_resource"));
        assert!(tools.iter().any(|t| t.name == "create_resource"));
        assert!(tools.iter().any(|t| t.name == "spawn_model"));
        assert!(tools.iter().any(|t| t.name == "generate_resource"));
        assert!(tools.iter().any(|t| t.name == "load_scene"));
        assert!(tools.iter().any(|t| t.name == "save_scene"));
    }
}
