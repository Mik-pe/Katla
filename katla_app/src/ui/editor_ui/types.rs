use std::collections::HashSet;

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
    Console,
    Mixer,
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
            EditorPanel::Mixer => 9,
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
            EditorPanel::Mixer => "Mixer",
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
            9 => Some(EditorPanel::Mixer),
            _ => None,
        }
    }

    pub fn all_editor_panels() -> &'static [EditorPanel] {
        &[
            EditorPanel::Hierarchy,
            EditorPanel::Viewport,
            EditorPanel::Inspector,
            EditorPanel::AssetBrowser,
            EditorPanel::Console,
            EditorPanel::Mixer,
        ]
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
    /// Perspective camera data (if entity has PerspectiveComponent)
    pub perspective: Option<PerspectiveInfo>,
    /// Directional light data (if entity has DirectionalLight)
    pub directional_light: Option<DirectionalLightInfo>,
    /// Audio emitter data (if entity has AudioEmitter)
    pub audio_emitter: Option<AudioEmitterInfo>,
    /// Audio source data (if entity has AudioSource)
    pub audio_source: Option<AudioSourceInfo>,
    /// Whether entity has AudioListener
    pub has_audio_listener: bool,
    /// Collider shape data (if entity has ColliderShape)
    pub collider_shape: Option<ColliderShapeInfo>,
    /// Rigid body data (if entity has RigidBody)
    pub rigid_body: Option<RigidBodyInfo>,
    /// Physics material data (if entity has PhysicsMaterial)
    pub physics_material: Option<PhysicsMaterialInfo>,
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

/// Perspective camera inspector data.
#[derive(Debug, Clone)]
pub struct PerspectiveInfo {
    pub fov: f32,
    pub near: f32,
    pub aspect_ratio: f32,
}

/// Directional light inspector data.
#[derive(Debug, Clone)]
pub struct DirectionalLightInfo {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
}

/// Audio source inspector data.
#[derive(Debug, Clone)]
pub struct AudioSourceInfo {
    pub path: String,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub duration_secs: Option<f64>,
}

/// Audio emitter inspector data.
#[derive(Debug, Clone)]
pub struct AudioEmitterInfo {
    pub source_path: String,
    pub volume: f32,
    pub looping: bool,
    pub playing: bool,
    pub spatial: bool,
    pub min_distance: f32,
    pub max_distance: f32,
    pub rolloff_factor: f32,
}

/// Collider shape type for the inspector dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColliderShapeType {
    Sphere,
    Box,
    Capsule,
}

impl ColliderShapeType {
    pub fn label(self) -> &'static str {
        match self {
            ColliderShapeType::Sphere => "Sphere",
            ColliderShapeType::Box => "Box",
            ColliderShapeType::Capsule => "Capsule",
        }
    }

    pub fn all() -> &'static [ColliderShapeType] {
        &[
            ColliderShapeType::Sphere,
            ColliderShapeType::Box,
            ColliderShapeType::Capsule,
        ]
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "Sphere" => Some(ColliderShapeType::Sphere),
            "Box" => Some(ColliderShapeType::Box),
            "Capsule" => Some(ColliderShapeType::Capsule),
            _ => None,
        }
    }
}

/// Collider shape inspector data.
#[derive(Debug, Clone)]
pub struct ColliderShapeInfo {
    pub shape_type: ColliderShapeType,
    pub sphere_radius: f32,
    pub box_half_extents: [f32; 3],
    pub capsule_half_height: f32,
    pub capsule_radius: f32,
}

/// Rigid body type for the inspector dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigidBodyType {
    Static,
    Dynamic,
    Kinematic,
}

impl RigidBodyType {
    pub fn label(self) -> &'static str {
        match self {
            RigidBodyType::Static => "Static",
            RigidBodyType::Dynamic => "Dynamic",
            RigidBodyType::Kinematic => "Kinematic",
        }
    }

    pub fn all() -> &'static [RigidBodyType] {
        &[
            RigidBodyType::Static,
            RigidBodyType::Dynamic,
            RigidBodyType::Kinematic,
        ]
    }
}

impl From<katla_physics::BodyType> for RigidBodyType {
    fn from(bt: katla_physics::BodyType) -> Self {
        match bt {
            katla_physics::BodyType::Static => RigidBodyType::Static,
            katla_physics::BodyType::Dynamic => RigidBodyType::Dynamic,
            katla_physics::BodyType::Kinematic => RigidBodyType::Kinematic,
        }
    }
}

/// Rigid body inspector data.
#[derive(Debug, Clone)]
pub struct RigidBodyInfo {
    pub body_type: RigidBodyType,
    pub gravity_scale: f32,
    pub linear_velocity: [f32; 3],
}

/// Physics material inspector data.
#[derive(Debug, Clone)]
pub struct PhysicsMaterialInfo {
    pub friction: f32,
    pub restitution: f32,
    pub density: f32,
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
    /// Toggle physics debug wireframe overlay.
    TogglePhysicsDebug,
    /// Toggle reverb zone wireframe overlay.
    ToggleReverbDebug,
    /// Set font scale (1.0 = 100%).
    SetFontScale(f32),
    /// Set master volume (0.0-1.0).
    SetMasterVolume(f32),
    /// Set SFX volume (0.0-1.0).
    SetSfxVolume(f32),
    /// Set music volume (0.0-1.0).
    SetMusicVolume(f32),
    /// Set ambient volume (0.0-1.0).
    SetAmbientVolume(f32),
    /// Open panel
    OpenPanel(Panel),
    /// Toggle the selected particle emitter active/inactive.
    ToggleParticleEmitter,
    /// Reset the global particle system (clear all particles).
    ResetParticleSystem,
    /// Set the gizmo transform mode.
    SetGizmoMode(u8), // 0=Translate, 1=Rotate, 2=Scale
    /// Add a registered component type to an entity.
    AddComponent { entity: EntityId, component: String },
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
    /// Update a particle emitter config field on an entity.
    SetEmitterField {
        entity: EntityId,
        field: crate::ui::particle_inspector::EmitterField,
    },
    /// Play/stop audio preview in asset browser.
    AudioPreviewToggle { path: std::path::PathBuf },
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
#[derive(Clone, Debug)]
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
    pub script_path: String,
    pub fov: f32,
    pub near: f32,
    pub aspect_ratio: f32,
    pub directional_direction: [f32; 3],
    pub directional_color: [f32; 3],
    pub directional_intensity: f32,
    pub audio_source_path: String,
    pub audio_volume: f32,
    pub audio_looping: bool,
    pub audio_spatial: bool,
    pub audio_min_distance: f32,
    pub audio_max_distance: f32,
    pub audio_rolloff_factor: f32,
    pub collider_shape_type: ColliderShapeType,
    pub collider_sphere_radius: f32,
    pub collider_box_half_extents: [f32; 3],
    pub collider_capsule_half_height: f32,
    pub collider_capsule_radius: f32,
    pub rigid_body_type: RigidBodyType,
    pub rigid_body_gravity_scale: f32,
    pub rigid_body_velocity: [f32; 3],
    pub physics_friction: f32,
    pub physics_restitution: f32,
    pub physics_density: f32,
    /// Cached script variables from the current frame's script instance.
    pub script_vars: Vec<(String, katla_script::ScriptVarValue)>,
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
            script_path: String::new(),
            fov: 60.0,
            near: 0.001,
            aspect_ratio: 16.0 / 9.0,
            directional_direction: [0.0, -1.0, 0.0],
            directional_color: [1.0; 3],
            directional_intensity: 1.0,
            audio_source_path: String::new(),
            audio_volume: 1.0,
            audio_looping: false,
            audio_spatial: false,
            audio_min_distance: 1.0,
            audio_max_distance: 100.0,
            audio_rolloff_factor: 1.0,
            collider_shape_type: ColliderShapeType::Sphere,
            collider_sphere_radius: 0.5,
            collider_box_half_extents: [0.5, 0.5, 0.5],
            collider_capsule_half_height: 0.5,
            collider_capsule_radius: 0.25,
            rigid_body_type: RigidBodyType::Dynamic,
            rigid_body_gravity_scale: 1.0,
            rigid_body_velocity: [0.0; 3],
            physics_friction: 0.5,
            physics_restitution: 0.0,
            physics_density: 1.0,
            script_vars: Vec::new(),
        }
    }
}

/// Hierarchy panel state (expanded entities tracking).
#[derive(Debug, Clone, Default)]
pub struct HierarchyState {
    pub expanded_entities: HashSet<EntityId>,
}

/// Preferences modal categories (sidebar order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferencesTab {
    #[default]
    Appearance,
    Viewport,
    Audio,
    Ai,
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

/// Actions emitted by the preferences panel.
#[derive(Debug, Clone)]
pub enum PreferencesAction {
    SetTheme(String),
    /// Select the visible preferences category (sidebar index).
    SetCategory(usize),
    ToggleGrid,
    ToggleStats,
    SetFontScale(f32),
    SetSnapToGrid(bool),
    SetCameraSpeed(f32),
    SetGridSize(f32),
    SetMasterVolume(f32),
    SetSfxVolume(f32),
    SetMusicVolume(f32),
    SetAmbientVolume(f32),
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
