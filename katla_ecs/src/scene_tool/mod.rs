mod command;
mod executor;
pub mod registry;

pub use command::{
    DestroyEntityCommand, DuplicateEntityCommand, SceneCommand, SetFieldCommand,
    SpawnEntityCommand, UndoGroup,
};
pub use executor::SceneToolExecutor;
pub use registry::{ComponentRegistry, ComponentRegistryEntry, FieldValue};

use crate::EntityId;

#[cfg(test)]
mod tests;

/// A tool the AI can invoke to manipulate the scene.
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub param_schema: serde_json::Value,
}

/// Result of executing a scene tool.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub success: bool,
    pub message: String,
    pub affected_entities: Vec<EntityId>,
    pub data: Option<serde_json::Value>,
}

/// Parameters for scene tool operations.
#[derive(Debug, Clone)]
pub enum SceneOp {
    /// Spawn an entity at a transform. Returns the new EntityId.
    SpawnEntity {
        position: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
        name: Option<String>,
        primitive: Option<String>,
    },
    /// Destroy an entity.
    DestroyEntity { entity: EntityId },
    /// Set a component field value on an entity.
    SetField {
        entity: EntityId,
        component: String,
        field: String,
        value: serde_json::Value,
    },
    /// Query entities matching a filter.
    QueryEntities {
        component_filter: Option<String>,
        name_filter: Option<String>,
        position: Option<[f32; 3]>,
        radius: Option<f32>,
        limit: Option<usize>,
    },
    /// Get the scene hierarchy (parent-child tree).
    GetSceneHierarchy,
    /// Duplicate an entity with an optional position offset.
    DuplicateEntity {
        entity: EntityId,
        position_offset: Option<[f32; 3]>,
    },
    /// List all registered component types with their fields and types.
    ListAvailableComponents,
    /// Add a component with default values to an existing entity.
    AddComponent { entity: EntityId, component: String },
    /// Get settable fields, types, and current values for a component on an entity.
    GetComponentAttributes { entity: EntityId, component: String },
    /// Set or clear the parent of an entity.
    SetParent {
        entity: EntityId,
        parent: Option<EntityId>,
    },
    /// Spawn a GLTF model from the assets directory.
    SpawnModel {
        path: String,
        position: [f32; 3],
        default_animation: Option<String>,
    },
}

/// Operations for project resource files (scenes, particles, materials, etc).
#[derive(Debug, Clone)]
pub enum ResourceOp {
    /// List resource files under a directory path.
    ListResources {
        /// Directory path relative to project root (e.g., "assets/particles").
        path: String,
        /// Optional file extension filter (e.g., "json", "katla").
        filter: Option<String>,
    },
    /// Read a resource file's content as a string.
    ReadResource {
        /// File path relative to project root.
        path: String,
    },
    /// Write content to an existing resource file.
    WriteResource {
        /// File path relative to project root.
        path: String,
        /// New file content.
        content: String,
    },
    /// Create a new resource file with optional template.
    CreateResource {
        /// File path relative to project root.
        path: String,
        /// Optional template name for content generation.
        template: Option<String>,
        /// Initial file content (if no template).
        content: Option<String>,
    },
    /// Delete a resource file.
    DeleteResource {
        /// File path relative to project root.
        path: String,
    },
}

/// Error type for scene tool operations.
#[derive(Debug, Clone)]
pub enum SceneToolError {
    EntityNotFound(EntityId),
    ComponentNotFound {
        entity: EntityId,
        component: String,
    },
    FieldNotFound {
        component: String,
        field: String,
    },
    InvalidFieldValue {
        field: String,
        expected_type: String,
        got: String,
    },
    WorldError(String),
}

impl std::fmt::Display for SceneToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SceneToolError::EntityNotFound(id) => write!(f, "Entity not found: {id}"),
            SceneToolError::ComponentNotFound { entity, component } => {
                write!(f, "Component '{component}' not found on entity {entity}")
            }
            SceneToolError::FieldNotFound { component, field } => {
                write!(f, "Field '{field}' not found on component '{component}'")
            }
            SceneToolError::InvalidFieldValue {
                field,
                expected_type,
                got,
            } => {
                write!(
                    f,
                    "Invalid value for field '{field}': expected {expected_type}, got {got}"
                )
            }
            SceneToolError::WorldError(msg) => write!(f, "World error: {msg}"),
        }
    }
}

impl std::error::Error for SceneToolError {}
