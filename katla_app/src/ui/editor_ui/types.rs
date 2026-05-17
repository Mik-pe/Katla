use std::collections::HashSet;

use katla_ecs::EntityId;
use katla_math::Vec3;
use katla_ui::ForkAwesome;
use katla_ui::ScrollAreaState;
use katla_ui::widgets::{ColorPickerState, DraggablePanelState};

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
    Console,
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
            EditorPanel::Console => 8,
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
            EditorPanel::Console => "Console",
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
            8 => Some(EditorPanel::Console),
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

impl SpawnableModel {
    pub fn name(&self) -> &'static str {
        match self {
            SpawnableModel::Cube => "Cube",
            SpawnableModel::Sphere => "Sphere",
            SpawnableModel::Cylinder => "Cylinder",
            SpawnableModel::Plane => "Plane",
            SpawnableModel::Torus => "Torus",
        }
    }

    pub fn all() -> &'static [SpawnableModel] {
        &[
            SpawnableModel::Cube,
            SpawnableModel::Sphere,
            SpawnableModel::Cylinder,
            SpawnableModel::Plane,
            SpawnableModel::Torus,
        ]
    }
}

/// Toolbar state: menu open flags, pending actions, undo/redo counts.
#[derive(Debug, Clone, Default)]
pub struct ToolbarState {
    pub file_menu_open: bool,
    pub edit_menu_open: bool,
    pub view_menu_open: bool,
    pub create_menu_open: bool,
    pub pending_actions: Vec<EditorAction>,
    pub undo_count: usize,
    pub redo_count: usize,
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
    /// Script path (if entity has ScriptComponent)
    pub script_path: Option<String>,
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
    /// Enter play mode from editing.
    PlayStart,
    /// Toggle between Playing/Paused.
    PlayPause,
    /// Stop playing, return to editing and restore scene.
    PlayStop,
    /// Add a component to an entity by type name.
    AddComponent {
        entity: EntityId,
        component_type: String,
    },
    /// Remove a component from an entity by type name.
    RemoveComponent {
        entity: EntityId,
        component_type: String,
    },
    /// Clear all console log entries.
    ClearConsole,
    /// Toggle a console log level filter.
    ToggleConsoleFilterLevel { level_index: usize },
    /// Set the console search filter text.
    SetConsoleSearch { text: String },
    /// Set the script path on an entity's ScriptComponent.
    SetScriptPath { entity: EntityId, path: String },
}

/// Active tab in the bottom panel strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BottomPanelTab {
    #[default]
    AssetBrowser,
    Console,
}

impl BottomPanelTab {
    pub fn all() -> &'static [BottomPanelTab] {
        &[BottomPanelTab::AssetBrowser, BottomPanelTab::Console]
    }

    pub fn label(self) -> &'static str {
        match self {
            BottomPanelTab::AssetBrowser => "Assets",
            BottomPanelTab::Console => "Console",
        }
    }

    pub fn icon(self) -> char {
        match self {
            BottomPanelTab::AssetBrowser => katla_ui::ForkAwesome::FOLDER_OPEN,
            BottomPanelTab::Console => katla_ui::ForkAwesome::LIST,
        }
    }
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

/// Mutable inspector editing state for all editable properties.
pub struct InspectorEditState {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: [f32; 3],
    pub light_color: [f32; 3],
    pub light_intensity: f32,
    pub light_range: f32,
    pub emit_rate: f32,
    pub velocity: f32,
    pub lifetime: f32,
    pub gravity: f32,
    pub particle_scale: f32,
    pub light_color_picker: ColorPickerState,
    pub script_path: String,
}

impl Default for InspectorEditState {
    fn default() -> Self {
        Self {
            pos: [0.0; 3],
            rot: [0.0; 3],
            scale: [1.0, 1.0, 1.0],
            light_color: [1.0; 3],
            light_intensity: 1.0,
            light_range: 10.0,
            emit_rate: 10.0,
            velocity: 2.0,
            lifetime: 2.0,
            gravity: -9.81,
            particle_scale: 0.1,
            light_color_picker: ColorPickerState::new(),
            script_path: String::new(),
        }
    }
}

/// Hierarchy panel state (scroll, expanded entities, context menu).
#[derive(Debug, Clone, Default)]
pub struct HierarchyState {
    pub scroll_state: ScrollAreaState,
    pub expanded_entities: HashSet<EntityId>,
    pub context_menu_open: bool,
    pub context_entity: Option<EntityId>,
}

/// Preferences panel tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferencesTab {
    #[default]
    General,
    Viewport,
    Ai,
}

impl PreferencesTab {
    pub fn all() -> &'static [PreferencesTab] {
        &[
            PreferencesTab::General,
            PreferencesTab::Viewport,
            PreferencesTab::Ai,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            PreferencesTab::General => "General",
            PreferencesTab::Viewport => "Viewport",
            PreferencesTab::Ai => "AI",
        }
    }

    pub fn icon(&self) -> char {
        match self {
            PreferencesTab::General => ForkAwesome::PAINT_BRUSH,
            PreferencesTab::Viewport => ForkAwesome::CUBE,
            PreferencesTab::Ai => ForkAwesome::CUBE,
        }
    }
}

/// Session-only editor settings (not persisted between sessions).
#[derive(Debug, Clone)]
pub struct EditorSettings {
    pub snap_to_grid: bool,
    pub camera_speed: f32,
    pub grid_size: f32,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            snap_to_grid: true,
            camera_speed: 50.0,
            grid_size: 1.0,
        }
    }
}

/// Internal state for the preferences panel widget.
#[derive(Debug, Clone, Default)]
pub struct PreferencesPanelState {
    pub panel: DraggablePanelState,
    pub current_tab: PreferencesTab,
    pub scroll_state: ScrollAreaState,
    /// Snapshot of LLM config, refreshed from EditorState each frame.
    pub llm_config: katla_agent::LlmConfig,
}

/// Actions emitted by the preferences panel.
#[derive(Debug, Clone)]
pub enum PreferencesAction {
    SetTheme(String),
    ToggleGrid,
    ToggleStats,
    SetFontScale(f32),
    SetSnapToGrid(bool),
    SetCameraSpeed(f32),
    SetGridSize(f32),
    SetLlmProvider(String),
    SetLlmApiKey(String),
    SetLlmBaseUrl(String),
    SetLlmModel(String),
    SetLlmMaxTokens(u32),
    SetLlmTemperature(f32),
    SaveLlmConfig,
}

/// O(D) visibility check using a pre-built parent map.
pub fn is_entity_visible_fast(
    entity: &EntityInfo,
    parent_map: &std::collections::HashMap<EntityId, Option<EntityId>>,
    expanded: &HashSet<EntityId>,
) -> bool {
    let mut current = entity.parent_id;
    while let Some(parent_id) = current {
        if !expanded.contains(&parent_id) {
            return false;
        }
        current = parent_map.get(&parent_id).copied().flatten();
    }
    true
}
