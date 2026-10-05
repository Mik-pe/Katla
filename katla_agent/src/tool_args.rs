//! Scene and resource input types independent of any conversation runtime.
use serde::Deserialize;

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

/// Lossless IDs for the co-creator `set_parent` tool.
#[derive(Debug, Clone, Deserialize)]
pub struct SetParentArgs {
    pub entity_id: String,
    pub parent_id: Option<String>,
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
