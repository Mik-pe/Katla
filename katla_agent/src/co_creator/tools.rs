use crate::llm::ToolDefinition;

/// Build tool definitions for the LLM's function calling.
pub fn build_tool_definitions() -> Vec<ToolDefinition> {
    use serde_json::json;

    vec![
        ToolDefinition {
            name: "prefab".into(),
            description: "Create reusable meshes and prefabs. Start with describe for JSON examples and geometry kinds, then validate/write assets, instantiate a preview and inspect it with editor_view. Read/edit named parts to iterate; capture exports an edited subtree; remove deletes a preview subtree. Paths are project-relative .katmesh/.katprefab. root_entity is a full decimal entity ID string. One mesh is baked to one material draw and shared across identical instances.".into(),
            parameters: crate::prefab::PrefabOp::tool_schema(),
        },
        ToolDefinition {
            name: "material".into(),
            description: "Discover presets, inspect a mesh material, or patch base_color (sRGB RGBA), metallic, roughness and ao in 0..1 on entity_ids as one undoable batch. Preset defaults can be overridden. Textures are preserved. IDs are decimal strings.".into(),
            parameters: json!({"type":"object","properties":{
                "action":{"type":"string","enum":["presets","inspect","set"]},
                "entity_id":{"type":"string"},
                "entity_ids":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":256},
                "preset":{"type":"string","enum":["plaster","oak","concrete","ceramic","brushed_metal","fabric"]},
                "base_color":{"type":"array","items":{"type":"number","minimum":0,"maximum":1},"minItems":4,"maxItems":4},
                "metallic":{"type":"number","minimum":0,"maximum":1},
                "roughness":{"type":"number","minimum":0,"maximum":1},
                "ao":{"type":"number","minimum":0,"maximum":1}
            },"required":["action"],"additionalProperties":false}),
        },
        ToolDefinition {
            name: "search_assets".into(),
            description: "Search resource-relative asset paths by all words in query and optional extensions. Use returned model paths directly with spawn_model.".into(),
            parameters: json!({"type":"object","properties":{
                "query":{"type":"string"},"extensions":{"type":"array","items":{"type":"string"}},
                "limit":{"type":"integer","minimum":1,"maximum":256}
            },"additionalProperties":false}),
        },
        ToolDefinition {
            name: "trigger".into(),
            description: "Create a sensor box with enter/exit rules, set_rules on an existing trigger, or inspect its rules and overlaps. create_box requires name, position, half_extents and rules. set_rules requires entity_id and rules. Actions: play_animation (target, clip, optional fade_seconds/looping/speed) or emit (name). target kinds: trigger, other, entity (with entity ID). Optional other_entity filters visitors. once fires once per play session. Runs only in play mode.".into(),
            parameters: crate::events::TriggerOp::tool_schema(),
        },
        ToolDefinition {
            name: "animation".into(),
            description: "Inspect an animated entity to discover clips and fade progress, or play a named clip with a crossfade. Default fade is 0.25 seconds, looping true, speed 1. Zero fade switches immediately. A positive fade during another fade returns an error without changing the pose; inspect and retry after completion.".into(),
            parameters: crate::animation::AnimationOp::tool_schema(),
        },
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
            description: "Spawn a GLTF model using a resource-relative path returned by search_assets.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the GLTF file relative to the discovered resources directory (e.g., 'models/character.glb')"
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
        assert!(tools.iter().any(|t| t.name == "material"
            && t.parameters["properties"]["entity_ids"]["items"]["type"] == "string"));
        assert!(tools.iter().any(|t| t.name == "search_assets"));
        assert!(tools.iter().any(|tool| tool.name == "trigger"
            && tool.parameters["properties"]["rules"]["maxItems"] == 64));
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
