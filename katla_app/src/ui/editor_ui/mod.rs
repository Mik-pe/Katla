//! Game Engine Editor UI
//!
//! A full game engine-style interface with:
//! - Entity Hierarchy panel (left)
//! - Viewport window (center)
//! - Properties/Inspector panel (right)
//! - Toolbar (top)
//! - Status bar (bottom)

mod asset_browser;
mod co_creator;
mod declarative;
mod layout;
#[cfg(test)]
mod tests;
#[allow(dead_code)]
mod types;

use std::sync::{Arc, Mutex};

use katla_ecs::EntityId;
use katla_gfx::TextureHandle;
use katla_math::{Rect2D, Vec2};
use katla_ui::declarative::{StateId, ViewTree};
use katla_ui::dock::DockTree;
use katla_ui::{ColorScheme, DrawList, UiContext};

use crate::ui::console::LogBuffer;
use crate::util::BackgroundLoader;

use crate::{
    Preferences,
    resources::viewport_state::ViewportGridState,
    ui::{
        ParticleInspectorAction, ParticleInspectorData, ParticleInspectorState,
        editor_ui::types::{
            EditorSettings, HierarchyState, PreferencesAction, PreferencesPanelState,
        },
    },
};

use asset_browser::AssetBrowserState;
use co_creator::CoCreatorState;

pub use asset_browser::ThumbnailState;
pub use types::*;

/// Parameters for rendering the editor UI.
pub struct EditorRenderParams<'a> {
    pub preferences: &'a Preferences,
    pub screen_size: Vec2,
    pub scale_factor: f32,
    pub entities: &'a [EntityInfo],
    pub fps: f32,
    pub frame_time_ms: f32,
    pub frame_count: usize,
    pub loader: &'a mut BackgroundLoader,
    pub thumbnail_texture_handles: &'a std::collections::HashMap<std::path::PathBuf, TextureHandle>,
    pub llm_config: &'a katla_agent::LlmConfig,
    #[expect(dead_code)]
    pub undo_count: usize,
    #[expect(dead_code)]
    pub redo_count: usize,
    pub agent_undo_count: usize,
    pub audio_levels: katla_audio::LevelsSnapshot,
    pub audio_active_voices: usize,
    pub audio_peak_voices: usize,
}

/// Game Engine Editor UI state.
pub struct EditorUI {
    /// Currently selected entity.
    pub selected_entity: Option<EntityId>,
    /// Preferences panel state (visibility, position, tab, scroll).
    preferences_panel_state: PreferencesPanelState,
    /// Session-only editor settings (not persisted).
    editor_settings: EditorSettings,
    /// Hierarchy panel state (scroll, expanded entities, context menu).
    hierarchy_state: HierarchyState,
    /// Left panel (hierarchy) width in pixels.
    pub left_panel_width: f32,
    /// Right panel (inspector) width in pixels.
    pub right_panel_width: f32,
    /// Play mode active.
    pub is_playing: bool,
    /// Whether play mode is paused (vs actively playing).
    pub is_paused: bool,
    /// Grid visibility.
    pub show_grid: bool,
    /// Stats panel visible.
    pub show_stats: bool,
    /// Physics debug wireframe overlay visible.
    pub show_physics_debug: bool,
    /// Reverb zone wireframe overlay visible.
    pub show_reverb_debug: bool,
    /// Draw calls submitted last frame.
    pub last_draw_call_count: usize,
    /// Font scale multiplier (1.0 = 100%).
    pub font_scale: f32,
    /// Deferred actions to be processed by the application.
    pub pending_actions: Vec<EditorAction>,
    /// Last known viewport panel size (width, height) in pixels.
    last_viewport_size: (u32, u32),
    /// Last known viewport panel bounds in logical screen coordinates.
    pub(crate) last_viewport_bounds: Rect2D,
    /// Last known screen size (for computing floating panel default positions).
    last_screen_size: Vec2,

    /// Current color scheme.
    pub theme: ColorScheme,
    /// Asset browser panel state.
    pub asset_browser: AssetBrowserState,
    /// Currently focused panel (receives keyboard input).
    pub focused_panel: FocusedPanel,
    /// Viewport grid state (layout and viewport assignments).
    pub viewport_grid_state: ViewportGridState,
    /// Texture IDs for each viewport slot (set by application during setup).
    /// These can be regular texture IDs or bindless texture IDs (high bit set).
    pub viewport_texture_ids: [Option<katla_ui::TextureId>; 4],
    /// Selected particle emitter entity for the particle inspector.
    pub selected_particle_emitter: Option<EntityId>,
    /// Draggable panel state for the particle inspector.
    pub particle_inspector_state: ParticleInspectorState,
    /// Pre-collected data for the particle inspector (refreshed each frame).
    pub particle_inspector_data: ParticleInspectorData,
    /// Transient save confirmation timer (seconds remaining to show "Scene saved").
    pub save_confirmation_timer: f32,
    /// Whether the UI wanted keyboard capture on the previous frame.
    /// Used to suppress Ctrl+S when a TextInput or modal is focused.
    pub prev_want_capture_keyboard: bool,
    /// Whether the UI wanted mouse capture on the previous frame.
    /// Used to block viewport picking/selection when clicking on floating UI.
    pub prev_want_capture_mouse: bool,
    /// Mutable inspector editing state for all editable properties.
    pub inspector_edit: types::InspectorEditState,
    /// The entity ID whose inspector editing state is currently populated.
    pub(crate) inspector_edit_entity: Option<EntityId>,
    /// Current gizmo mode (synced from Application for toolbar display).
    pub gizmo_mode: u8,
    /// AI Co-Creator chat panel state.
    pub co_creator: CoCreatorState,
    /// Inspector panel scroll state.
    #[expect(dead_code)]
    inspector_scroll_state: katla_ui::ScrollAreaState,
    /// Whether the "Add Component" dropdown is open.
    add_component_open: bool,
    /// Scroll state for the "Add Component" popup list.
    #[expect(dead_code)]
    add_component_scroll_state: katla_ui::ScrollAreaState,
    /// Search filter for the "Add Component" dropdown.
    add_component_filter: String,
    /// Whether to auto-focus the script path text input (set when ScriptComponent is added).
    focus_script_input: bool,
    /// Available component type names (populated from ComponentRegistry).
    available_components: Vec<&'static str>,
    /// Search/filter text for the hierarchy panel.
    hierarchy_search_filter: String,
    /// Dockable panel layout tree.
    dock_tree: DockTree<u64>,
    /// State IDs for the DockSpace widget (set after first frame).
    dock_state_id: Option<StateId>,
    drag_state_id: Option<StateId>,
    /// Declarative view tree for migrated panels.
    view_tree: ViewTree,
    /// Console panel state.
    pub(crate) console_state: declarative::ConsoleState,
    /// Shared log buffer backing the console panel.
    pub(crate) log_buffer: Arc<Mutex<LogBuffer>>,
}

impl EditorUI {
    pub fn new() -> Self {
        Self {
            selected_entity: None,
            preferences_panel_state: PreferencesPanelState::default(),
            editor_settings: EditorSettings::default(),
            hierarchy_state: HierarchyState::default(),
            left_panel_width: 220.0,
            right_panel_width: 280.0,
            is_playing: false,
            is_paused: false,
            show_grid: true,
            show_stats: true,
            show_physics_debug: false,
            show_reverb_debug: false,
            last_draw_call_count: 0,
            font_scale: 1.0,
            pending_actions: Vec::new(),
            last_viewport_size: (800, 600),
            last_viewport_bounds: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(800.0, 600.0)),
            last_screen_size: Vec2::new(800.0, 600.0),
            theme: ColorScheme::default_theme(),
            asset_browser: AssetBrowserState::new(),
            focused_panel: FocusedPanel::Viewport,
            viewport_grid_state: ViewportGridState::new(),
            viewport_texture_ids: [None, None, None, None],
            selected_particle_emitter: None,
            particle_inspector_state: ParticleInspectorState::default(),
            particle_inspector_data: ParticleInspectorData::default(),
            save_confirmation_timer: 0.0,
            prev_want_capture_keyboard: false,
            prev_want_capture_mouse: false,
            inspector_edit: types::InspectorEditState::default(),
            inspector_edit_entity: None,
            gizmo_mode: 0,
            co_creator: CoCreatorState::new(),
            inspector_scroll_state: katla_ui::ScrollAreaState::default(),
            add_component_open: false,
            add_component_scroll_state: katla_ui::ScrollAreaState::default(),
            add_component_filter: String::new(),
            focus_script_input: false,
            available_components: Vec::new(),
            hierarchy_search_filter: String::new(),
            dock_tree: Self::default_dock_tree(),
            dock_state_id: None,
            drag_state_id: None,
            view_tree: ViewTree::default(),
            console_state: declarative::ConsoleState::default(),
            log_buffer: Arc::new(Mutex::new(LogBuffer::new())),
        }
    }

    /// Clear all entity references held by the UI.
    /// Called after scene restore invalidates all EntityIds.
    pub fn clear_entity_references(&mut self) {
        self.selected_entity = None;
        self.selected_particle_emitter = None;
        self.hierarchy_state.expanded_entities.clear();
    }

    /// Set the shared log buffer (called by ApplicationBuilder after ConsoleLogger init).
    pub(crate) fn set_log_buffer(&mut self, buffer: Arc<Mutex<LogBuffer>>) {
        self.log_buffer = buffer;
    }

    /// Create editor with a specific theme.
    pub fn with_theme(theme: ColorScheme) -> Self {
        let mut editor = Self::new();
        editor.theme = theme;
        editor
    }

    /// Set the editor theme.
    pub fn set_theme(&mut self, theme: ColorScheme) {
        self.theme = theme;
    }

    /// Sync inspector editing state from the selected entity's EntityInfo.
    ///
    /// Called before UI build to populate slider values. Only updates when the
    /// selected entity changes (detected via entity ID).
    pub fn sync_inspector_edit_state(&mut self, entities: &[EntityInfo]) {
        if self.inspector_edit_entity != self.selected_entity {
            self.inspector_edit_entity = self.selected_entity;
            if let Some(entity) = self
                .selected_entity
                .and_then(|id| entities.iter().find(|e| e.id == id))
            {
                self.inspector_edit.pos = [
                    entity.position.x(),
                    entity.position.y(),
                    entity.position.z(),
                ];
                self.inspector_edit.rot = [
                    entity.rotation.x(),
                    entity.rotation.y(),
                    entity.rotation.z(),
                ];
                self.inspector_edit.scale = [entity.scale.x(), entity.scale.y(), entity.scale.z()];
                if let Some(ref pl) = entity.point_light {
                    self.inspector_edit.light_color = pl.color;
                    self.inspector_edit.light_intensity = pl.intensity;
                    self.inspector_edit.light_range = pl.range;
                }
                if let Some(ref pe) = entity.particle_emitter {
                    self.inspector_edit.emit_rate = pe.emit_rate;
                    self.inspector_edit.velocity = pe.velocity_magnitude;
                    self.inspector_edit.lifetime = pe.base_lifetime;
                    self.inspector_edit.gravity = pe.gravity;
                    self.inspector_edit.particle_scale = pe.base_scale;
                }
                if let Some(ref path) = entity.script_path {
                    self.inspector_edit.script_path = path.clone();
                } else {
                    self.inspector_edit.script_path.clear();
                }
                if let Some(ref p) = entity.perspective {
                    self.inspector_edit.fov = p.fov;
                    self.inspector_edit.near = p.near;
                    self.inspector_edit.aspect_ratio = p.aspect_ratio;
                }
                if let Some(ref dl) = entity.directional_light {
                    self.inspector_edit.directional_direction = dl.direction;
                    self.inspector_edit.directional_color = dl.color;
                    self.inspector_edit.directional_intensity = dl.intensity;
                }
                if let Some(ref ae) = entity.audio_emitter {
                    self.inspector_edit.audio_source_path = ae.source_path.clone();
                    self.inspector_edit.audio_volume = ae.volume;
                    self.inspector_edit.audio_looping = ae.looping;
                    self.inspector_edit.audio_spatial = ae.spatial;
                    self.inspector_edit.audio_min_distance = ae.min_distance;
                    self.inspector_edit.audio_max_distance = ae.max_distance;
                    self.inspector_edit.audio_rolloff_factor = ae.rolloff_factor;
                } else {
                    self.inspector_edit.audio_source_path.clear();
                    self.inspector_edit.audio_volume = 1.0;
                    self.inspector_edit.audio_looping = false;
                    self.inspector_edit.audio_spatial = false;
                    self.inspector_edit.audio_min_distance = 1.0;
                    self.inspector_edit.audio_max_distance = 100.0;
                    self.inspector_edit.audio_rolloff_factor = 1.0;
                }
                if let Some(ref cs) = entity.collider_shape {
                    self.inspector_edit.collider_shape_type = cs.shape_type;
                    self.inspector_edit.collider_sphere_radius = cs.sphere_radius;
                    self.inspector_edit.collider_box_half_extents = cs.box_half_extents;
                    self.inspector_edit.collider_capsule_half_height = cs.capsule_half_height;
                    self.inspector_edit.collider_capsule_radius = cs.capsule_radius;
                }
                if let Some(ref rb) = entity.rigid_body {
                    self.inspector_edit.rigid_body_type = rb.body_type;
                    self.inspector_edit.rigid_body_gravity_scale = rb.gravity_scale;
                    self.inspector_edit.rigid_body_velocity = rb.linear_velocity;
                }
                if let Some(ref pm) = entity.physics_material {
                    self.inspector_edit.physics_friction = pm.friction;
                    self.inspector_edit.physics_restitution = pm.restitution;
                    self.inspector_edit.physics_density = pm.density;
                }
            }
        }
    }

    /// Refresh script variable state from the script engine for the selected entity.
    /// Reads from the ScriptInspectorData ECS resource that the ScriptSystem populates.
    pub fn refresh_script_vars(&mut self, world: &katla_ecs::World) {
        let data = world.get_resource::<katla_script::ScriptInspectorData>();
        if let Some(data) = data {
            if let Some(entity) = self.selected_entity {
                self.inspector_edit.script_vars = data
                    .entries
                    .iter()
                    .find(|(id, _, _)| *id == entity)
                    .map(|(_, _, vars)| vars.clone())
                    .unwrap_or_default();
            } else {
                self.inspector_edit.script_vars.clear();
            }
        }
    }

    /// Set the font scale.
    pub fn set_font_scale(&mut self, scale: f32) {
        self.font_scale = scale.clamp(0.5, 3.0);
    }

    /// Set the viewport texture (tonemapped LDR output) for rendering in the viewport widget.
    ///
    /// This stores the bindless texture index that the UI will use to sample from the transient texture.
    /// We encode it as a special TextureId with a high bit set to distinguish from regular textures.
    pub fn set_viewport_bindless_index(&mut self, bindless_index: u32) {
        const BINDLESS_FLAG: u64 = 1 << 63;
        let texture_id = katla_ui::TextureId::new(BINDLESS_FLAG | (bindless_index as u64));
        self.viewport_texture_ids = [Some(texture_id), None, None, None];
    }

    /// Get the current viewport panel size in pixels.
    pub fn viewport_size(&self) -> (u32, u32) {
        self.last_viewport_size
    }

    /// Get the current theme key (for preferences).
    pub fn theme_key(&self) -> &'static str {
        match self.theme.name {
            "Default" => "default",
            "Nord" => "nord",
            "Tokyo Night" => "tokyo_night",
            "Dracula" => "dracula",
            "Gruvbox Dark" => "gruvbox",
            "One Dark" => "one_dark",
            "Material Palenight" => "material_palenight",
            "Ayu Dark" => "ayu_dark",
            "GitHub Dark" => "github_dark",
            "Monokai" => "monokai",
            "Rosé Pine" => "rose_pine",
            "Kanagawa" => "kanagawa",
            "Solarized Dark" => "solarized_dark",
            _ => "default",
        }
    }

    /// Get the current theme name.
    pub fn theme_name(&self) -> &'static str {
        self.theme.name
    }

    /// Set the list of available component type names from the ComponentRegistry.
    pub fn set_available_components(&mut self, names: Vec<&'static str>) {
        self.available_components = names;
    }

    pub fn open_panel(&mut self, panel: Panel) {
        match panel {
            Panel::Preferences => {
                self.preferences_panel_state.panel.open();
            }
            Panel::ParticleInspector => {
                self.particle_inspector_state.panel.open();
            }
            Panel::CoCreator => {
                self.co_creator.open();
            }
        }
    }

    /// Apply a preferences action, updating state or forwarding to EditorAction.
    pub fn apply_preferences_action(&mut self, action: PreferencesAction) {
        match action {
            PreferencesAction::SetTheme(name) => {
                self.pending_actions.push(EditorAction::SetTheme(name));
            }
            PreferencesAction::ToggleGrid => {
                self.pending_actions.push(EditorAction::ToggleGrid);
            }
            PreferencesAction::ToggleStats => {
                self.pending_actions.push(EditorAction::ToggleStats);
            }
            PreferencesAction::SetFontScale(scale) => {
                self.pending_actions.push(EditorAction::SetFontScale(scale));
            }
            PreferencesAction::SetMasterVolume(vol) => {
                self.pending_actions
                    .push(EditorAction::SetMasterVolume(vol));
            }
            PreferencesAction::SetSfxVolume(vol) => {
                self.pending_actions.push(EditorAction::SetSfxVolume(vol));
            }
            PreferencesAction::SetMusicVolume(vol) => {
                self.pending_actions.push(EditorAction::SetMusicVolume(vol));
            }
            PreferencesAction::SetAmbientVolume(vol) => {
                self.pending_actions
                    .push(EditorAction::SetAmbientVolume(vol));
            }
            PreferencesAction::SetSnapToGrid(value) => {
                self.editor_settings.snap_to_grid = value;
            }
            PreferencesAction::SetCameraSpeed(value) => {
                self.editor_settings.camera_speed = value;
            }
            PreferencesAction::SetGridSize(value) => {
                self.editor_settings.grid_size = value;
            }
            PreferencesAction::SetLlmProvider(value) => {
                self.pending_actions
                    .push(EditorAction::SetLlmProvider(value));
            }
            PreferencesAction::SetLlmApiKey(value) => {
                self.pending_actions.push(EditorAction::SetLlmApiKey(value));
            }
            PreferencesAction::SetLlmBaseUrl(value) => {
                self.pending_actions
                    .push(EditorAction::SetLlmBaseUrl(value));
            }
            PreferencesAction::SetLlmModel(value) => {
                self.pending_actions.push(EditorAction::SetLlmModel(value));
            }
            PreferencesAction::SetLlmMaxTokens(value) => {
                self.pending_actions
                    .push(EditorAction::SetLlmMaxTokens(value));
            }
            PreferencesAction::SetLlmTemperature(value) => {
                self.pending_actions
                    .push(EditorAction::SetLlmTemperature(value));
            }
            PreferencesAction::SaveLlmConfig => {
                self.pending_actions.push(EditorAction::SaveLlmConfig);
            }
        }
    }

    /// Apply a particle inspector action, updating state or forwarding to EditorAction.
    pub fn apply_particle_inspector_action(&mut self, action: ParticleInspectorAction) {
        match action {
            ParticleInspectorAction::SelectEmitter(entity_id) => {
                self.selected_particle_emitter = Some(entity_id);
            }
            ParticleInspectorAction::ToggleEmitter => {
                self.pending_actions
                    .push(EditorAction::ToggleParticleEmitter);
            }
            ParticleInspectorAction::ResetSystem => {
                self.pending_actions.push(EditorAction::ResetParticleSystem);
            }
            ParticleInspectorAction::Close => {
                self.particle_inspector_state.panel.close();
            }
            ParticleInspectorAction::SetEmitterField(entity, field) => {
                self.pending_actions
                    .push(EditorAction::SetEmitterField { entity, field });
            }
        }
    }

    /// Check if a screen-space point falls within any visible floating panel.
    fn is_click_on_floating_panel(&self, pos: Vec2) -> bool {
        let screen = self.last_screen_size;

        if let Some(bounds) = self
            .preferences_panel_state
            .panel
            .bounds(450.0, 500.0, screen)
            && bounds.contains(pos)
        {
            return true;
        }

        if let Some(bounds) = self
            .particle_inspector_state
            .panel
            .bounds(320.0, 600.0, screen)
            && bounds.contains(pos)
        {
            return true;
        }

        if let Some(bounds) = self.co_creator.panel.bounds(400.0, 500.0, screen)
            && bounds.contains(pos)
        {
            return true;
        }

        false
    }

    /// Eagerly update focused panel based on click position.
    ///
    /// Called during `window_event` (before UI build) so that the first click
    /// on a panel both sets focus AND is forwarded to the correct input handler
    /// without a one-frame delay.
    pub fn update_focused_panel_from_click(&mut self, mouse_pos: Vec2) {
        let toolbar_height = 32.0;
        if mouse_pos.y() < toolbar_height {
            return;
        }

        if self.is_click_on_floating_panel(mouse_pos) {
            return;
        }

        if self.last_viewport_bounds.contains(mouse_pos) {
            self.focused_panel = FocusedPanel::Viewport;
            return;
        }

        let left_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, toolbar_height),
            Vec2::new(self.left_panel_width, self.last_viewport_bounds.height()),
        );
        if left_bounds.contains(mouse_pos) {
            self.focused_panel = FocusedPanel::Hierarchy;
            return;
        }

        let right_panel_x = self.last_viewport_bounds.max.x();
        let right_bounds = Rect2D::from_origin_size(
            Vec2::new(right_panel_x, toolbar_height),
            Vec2::new(self.right_panel_width, self.last_viewport_bounds.height()),
        );
        if right_bounds.contains(mouse_pos) {
            self.focused_panel = FocusedPanel::Inspector;
            return;
        }

        if mouse_pos.y() >= self.last_viewport_bounds.max.y() {
            self.focused_panel = FocusedPanel::AssetBrowser;
        }
    }

    /// Render the editor UI and return the draw list.
    pub fn render<'a>(
        &'a mut self,
        ui: &'a mut UiContext,
        params: &mut EditorRenderParams,
    ) -> &'a DrawList {
        self.theme.apply_to_style(ui.style_mut());

        ui.set_font_scale(self.font_scale);

        ui.begin(params.screen_size, params.scale_factor);
        self.build(ui, params);
        ui.end()
    }

    /// Take pending actions, clearing the list.
    pub fn take_actions(&mut self) -> Vec<EditorAction> {
        std::mem::take(&mut self.pending_actions)
    }

    /// Show a transient save confirmation in the status bar.
    pub fn show_save_confirmation(&mut self) {
        self.save_confirmation_timer = 2.0;
    }

    /// Update per-frame timers (call once per frame).
    pub fn update_timers(&mut self, dt: f32) {
        if self.save_confirmation_timer > 0.0 {
            self.save_confirmation_timer = (self.save_confirmation_timer - dt).max(0.0);
        }
    }

    pub fn editor_settings(&self) -> &EditorSettings {
        &self.editor_settings
    }

    fn default_dock_tree() -> DockTree<u64> {
        use katla_ui::dock::{DockNode, SplitDirection};

        let hierarchy = DockNode::Leaf {
            tabs: vec![EditorPanel::Hierarchy.id()],
            active: 0,
        };
        let viewport = DockNode::Leaf {
            tabs: vec![EditorPanel::Viewport.id()],
            active: 0,
        };
        let inspector = DockNode::Leaf {
            tabs: vec![EditorPanel::Inspector.id()],
            active: 0,
        };
        let bottom_tabs = DockNode::Leaf {
            tabs: vec![
                EditorPanel::AssetBrowser.id(),
                EditorPanel::Console.id(),
                EditorPanel::Mixer.id(),
            ],
            active: 0,
        };

        let right = DockNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.7,
            children: [Box::new(viewport), Box::new(inspector)],
        };
        let main = DockNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.18,
            children: [Box::new(hierarchy), Box::new(right)],
        };
        let root = DockNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.75,
            children: [Box::new(main), Box::new(bottom_tabs)],
        };

        DockTree::new(root)
    }
}

impl Default for EditorUI {
    fn default() -> Self {
        Self::new()
    }
}
