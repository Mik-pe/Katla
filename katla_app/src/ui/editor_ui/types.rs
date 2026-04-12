use katla_ecs::EntityId;
use katla_math::Vec3;

/// Panel IDs for the dockable panel system.
/// Each variant maps to a unique u64 ID used by the dock layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorPanel {
    Hierarchy,
    Viewport,
    Inspector,
    AssetBrowser,
    CoCreator,
    Preferences,
    ParticleInspector,
}

impl EditorPanel {
    pub fn id(self) -> u64 {
        match self {
            EditorPanel::Hierarchy => 1,
            EditorPanel::Viewport => 2,
            EditorPanel::Inspector => 3,
            EditorPanel::AssetBrowser => 4,
            EditorPanel::CoCreator => 5,
            EditorPanel::Preferences => 6,
            EditorPanel::ParticleInspector => 7,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EditorPanel::Hierarchy => "Hierarchy",
            EditorPanel::Viewport => "Viewport",
            EditorPanel::Inspector => "Inspector",
            EditorPanel::AssetBrowser => "Asset Browser",
            EditorPanel::CoCreator => "AI Co-Creator",
            EditorPanel::Preferences => "Preferences",
            EditorPanel::ParticleInspector => "Particle Inspector",
        }
    }

    pub fn from_id(id: u64) -> Option<Self> {
        match id {
            1 => Some(EditorPanel::Hierarchy),
            2 => Some(EditorPanel::Viewport),
            3 => Some(EditorPanel::Inspector),
            4 => Some(EditorPanel::AssetBrowser),
            5 => Some(EditorPanel::CoCreator),
            6 => Some(EditorPanel::Preferences),
            7 => Some(EditorPanel::ParticleInspector),
            _ => None,
        }
    }
}

/// Model types that can be spawned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnableModel {
    Cube,
    Sphere,
    Cylinder,
    Plane,
    Torus,
}

/// Entity info for the hierarchy panel.
#[derive(Debug, Clone)]
pub struct EntityInfo {
    pub id: EntityId,
    pub name: String,
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
    pub entity_type: String,
    /// List of component type names on this entity
    pub components: Vec<String>,
    /// Depth in hierarchy (0 = root, 1 = child of root, etc.)
    pub depth: u32,
    /// Whether this entity has children (for showing expand/collapse arrow)
    pub has_children: bool,
    /// Parent entity ID (if any)
    pub parent_id: Option<EntityId>,
    /// Point light data (if entity has PointLight component)
    pub point_light: Option<PointLightInfo>,
    /// Particle emitter data (if entity has ParticleEmitterComponent)
    pub particle_emitter: Option<ParticleEmitterInfo>,
}

/// Point light inspector data.
#[derive(Debug, Clone)]
pub struct PointLightInfo {
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
}

/// Particle emitter inspector data.
#[derive(Debug, Clone)]
pub struct ParticleEmitterInfo {
    pub emit_rate: f32,
    pub velocity_magnitude: f32,
    pub base_lifetime: f32,
    pub gravity: f32,
    pub base_scale: f32,
}

#[derive(Debug, Clone)]
pub enum Panel {
    Preferences,
    ParticleInspector,
    CoCreator,
}

/// Action requested from the editor UI.
#[derive(Debug, Clone)]
pub enum EditorAction {
    /// Spawn a new model at the given position.
    SpawnModel(SpawnableModel, Vec3),
    /// Spawn a model from a specific file path at the given screen position.
    SpawnModelAtPath {
        path: std::path::PathBuf,
        screen_pos: katla_math::Vec2,
    },
    /// Delete an entity.
    DeleteEntity(EntityId),
    /// Duplicate an entity.
    DuplicateEntity(EntityId),
    /// Save the current scene to the default path.
    SaveScene,
    /// Open a scene from a file dialog.
    OpenScene,
    /// Create a new empty scene.
    NewScene,
    /// Quit the application.
    Quit,
    /// Select an entity.
    SelectEntity(EntityId),
    /// Change the editor theme.
    SetTheme(String),
    /// Toggle grid visibility.
    ToggleGrid,
    /// Toggle stats visibility.
    ToggleStats,
    /// Set font scale (1.0 = 100%).
    SetFontScale(f32),
    /// Open panel
    OpenPanel(Panel),
    /// Toggle the selected particle emitter active/inactive.
    ToggleParticleEmitter,
    /// Reset the global particle system (clear all particles).
    ResetParticleSystem,
    /// Set the gizmo transform mode.
    SetGizmoMode(u8), // 0=Translate, 1=Rotate, 2=Scale
    /// AI Co-Creator request from the chat panel.
    CoCreatorRequest(String),
    /// Set the LLM provider kind ("disabled", "open_ai", "open_ai_compatible").
    SetLlmProvider(String),
    /// Set the LLM API key.
    SetLlmApiKey(String),
    /// Set the LLM base URL (for OpenAI-compatible endpoints).
    SetLlmBaseUrl(String),
    /// Set the LLM model identifier.
    SetLlmModel(String),
    /// Set the LLM max response tokens.
    SetLlmMaxTokens(u32),
    /// Set the LLM sampling temperature.
    SetLlmTemperature(f32),
    /// Save the LLM configuration to disk.
    SaveLlmConfig,
    /// Undo the last editor operation.
    Undo,
    /// Redo the last undone editor operation.
    Redo,
    /// Undo the last AI agent operation.
    AgentUndo,
}

/// Which panel is currently focused (receives input).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedPanel {
    /// No panel focused (default).
    #[default]
    None,
    /// Game viewport - game receives input.
    Viewport,
    /// Hierarchy panel.
    Hierarchy,
    /// Inspector panel.
    Inspector,
    /// Asset browser panel.
    AssetBrowser,
}
