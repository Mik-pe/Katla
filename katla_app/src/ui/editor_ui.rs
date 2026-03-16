//! Game Engine Editor UI
//!
//! A full game engine-style interface with:
//! - Entity Hierarchy panel (left)
//! - Viewport window (center)
//! - Properties/Inspector panel (right)
//! - Toolbar (top)
//! - Status bar (bottom)

mod asset_browser;
mod hierarchy;
mod inspector;
mod preferences;
mod status_bar;
mod toolbar;
mod viewport;
mod viewport_grid;

use katla_ecs::EntityId;
use katla_gfx::TextureHandle;
use katla_math::{Color, Rect2D, Vec2, Vec3};
use katla_ui::{mouse_button, DrawList, FontSize, UiContext};
use std::path::PathBuf;

use crate::{
    resources::viewport_state::ViewportGridState,
    ui::{
        editor_ui::hierarchy::HierarchyState,
        editor_ui::preferences::{
            EditorSettings, PreferencesAction, PreferencesPanel, PreferencesPanelState,
        },
        editor_ui::toolbar::{Toolbar, ToolbarState},
    },
    Preferences,
};

use super::theme::Theme;
use asset_browser::{build_asset_browser, AssetAction, AssetBrowserState, AssetType};

pub use asset_browser::ThumbnailState;
pub use preferences::PanelState;

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
}

#[derive(Debug, Clone)]
pub enum Panel {
    Preferences,
    Editor,
}

/// Action requested from the editor UI.
#[derive(Debug, Clone)]
pub enum EditorAction {
    /// Spawn a new model at the given position.
    SpawnModel(SpawnableModel, Vec3),
    /// Spawn a model from a specific file path.
    SpawnModelAtPath { path: PathBuf, position: Vec3 },
    /// Delete an entity.
    DeleteEntity(EntityId),
    /// Duplicate an entity.
    DuplicateEntity(EntityId),
    /// Select an entity.
    SelectEntity(EntityId),
    /// Toggle play/pause.
    TogglePlay,
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

/// Panel resize edge for dragging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelResizeEdge {
    /// Left panel right edge.
    LeftPanelRight,
    /// Right panel left edge.
    RightPanelLeft,
    /// Asset browser top edge.
    AssetBrowserTop,
}

/// Game Engine Editor UI state.
pub struct EditorUI {
    /// Whether the editor is visible.
    pub visible: bool,
    /// Currently selected entity.
    pub selected_entity: Option<EntityId>,
    /// Spawn menu visibility state.
    spawn_menu_state: PanelState,
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
    /// Which panel resize handle is being dragged (if any).
    resizing_panel: Option<PanelResizeEdge>,
    /// Play mode active.
    pub is_playing: bool,
    /// Grid visibility.
    pub show_grid: bool,
    /// Stats panel visible.
    pub show_stats: bool,
    /// Font scale multiplier (1.0 = 100%).
    pub font_scale: f32,
    /// Deferred actions to be processed by the application.
    pub pending_actions: Vec<EditorAction>,
    /// Last known viewport panel size (width, height) in pixels.
    last_viewport_size: (u32, u32),

    toolbar_state: ToolbarState,
    /// Current color theme.
    pub theme: Theme,
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
    /// Whether the particle inspector panel is visible.
    pub show_particle_inspector: bool,
}

impl EditorUI {
    pub fn new() -> Self {
        Self {
            visible: true,
            selected_entity: None,
            spawn_menu_state: PanelState::default(),
            preferences_panel_state: PreferencesPanelState::default(),
            editor_settings: EditorSettings::default(),
            hierarchy_state: HierarchyState::default(),
            left_panel_width: 220.0,
            right_panel_width: 280.0,
            resizing_panel: None,
            is_playing: false,
            show_grid: true,
            show_stats: true,
            font_scale: 1.0,
            pending_actions: Vec::new(),
            last_viewport_size: (800, 600),
            toolbar_state: ToolbarState::default(),
            theme: Theme::catppuccin(),
            asset_browser: AssetBrowserState::new(),
            focused_panel: FocusedPanel::Viewport,
            viewport_grid_state: ViewportGridState::new(),
            viewport_texture_ids: [None, None, None, None],
            selected_particle_emitter: None,
            show_particle_inspector: false,
        }
    }

    /// Create editor with a specific theme.
    pub fn with_theme(theme: Theme) -> Self {
        let mut editor = Self::new();
        editor.theme = theme;
        editor
    }

    /// Set the editor theme.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
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
        // Encode bindless index in TextureId with high bit set (bit 63)
        // This distinguishes it from regular texture handles
        const BINDLESS_FLAG: u64 = 1 << 63;
        let texture_id = katla_ui::TextureId::new(BINDLESS_FLAG | (bindless_index as u64));

        // Store in viewport_texture_ids for the viewport grid widget
        self.viewport_texture_ids = [Some(texture_id), None, None, None];
    }

    /// Get the current viewport panel size in pixels.
    pub fn viewport_size(&self) -> (u32, u32) {
        self.last_viewport_size
    }

    /// Get the current theme key (for preferences).
    pub fn theme_key(&self) -> &'static str {
        match self.theme.name {
            "Catppuccin Mocha" => "catppuccin",
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
            _ => "catppuccin",
        }
    }

    /// Get the current theme name.
    pub fn theme_name(&self) -> &'static str {
        self.theme.name
    }

    pub fn open_panel(&mut self, panel: Panel) {
        match panel {
            Panel::Preferences => {
                self.preferences_panel_state.visibility.open();
            }
            Panel::Editor => {}
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
            PreferencesAction::SetSnapToGrid(value) => {
                self.editor_settings.snap_to_grid = value;
            }
            PreferencesAction::SetCameraSpeed(value) => {
                self.editor_settings.camera_speed = value;
            }
            PreferencesAction::SetGridSize(value) => {
                self.editor_settings.grid_size = value;
            }
            PreferencesAction::Close => {
                self.preferences_panel_state.visibility.close();
                self.preferences_panel_state.position = None;
            }
        }
    }

    /// Build the editor UI.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        ui: &mut UiContext,
        preferences: &Preferences,
        entities: &[EntityInfo],
        fps: f32,
        frame_count: usize,
        loader: &mut crate::util::BackgroundLoader,
        thumbnail_texture_handles: &std::collections::HashMap<std::path::PathBuf, TextureHandle>,
    ) {
        let screen_size = ui.screen_size();

        // Get visible entities (respecting collapsed state) for keyboard navigation
        let visible_entities: Vec<EntityId> = entities
            .iter()
            .filter(|e| {
                hierarchy::is_entity_visible(e, entities, &self.hierarchy_state.expanded_entities)
            })
            .map(|e| e.id)
            .collect();

        // === KEYBOARD SHORTCUTS ===
        // Delete key - delete selected entity
        if ui.key_pressed(katla_ui::input::KeyCode::Delete) {
            if let Some(entity_id) = self.selected_entity {
                if entities.iter().any(|e| e.id == entity_id) {
                    self.pending_actions
                        .push(EditorAction::DeleteEntity(entity_id));
                    self.selected_entity = None;
                }
            }
        }

        // Arrow Up - select previous entity
        if ui.key_pressed(katla_ui::input::KeyCode::ArrowUp) {
            if let Some(current_id) = self.selected_entity {
                if let Some(pos) = visible_entities.iter().position(|id| *id == current_id) {
                    if pos > 0 {
                        self.selected_entity = Some(visible_entities[pos - 1]);
                    }
                }
            } else if !visible_entities.is_empty() {
                // No selection - select last entity
                self.selected_entity = Some(*visible_entities.last().unwrap());
            }
        }

        // Arrow Down - select next entity
        if ui.key_pressed(katla_ui::input::KeyCode::ArrowDown) {
            if let Some(current_id) = self.selected_entity {
                if let Some(pos) = visible_entities.iter().position(|id| *id == current_id) {
                    if pos < visible_entities.len() - 1 {
                        self.selected_entity = Some(visible_entities[pos + 1]);
                    }
                }
            } else if !visible_entities.is_empty() {
                // No selection - select first entity
                self.selected_entity = Some(visible_entities[0]);
            }
        }

        // Arrow Right - expand selected entity
        if ui.key_pressed(katla_ui::input::KeyCode::ArrowRight) {
            if let Some(entity_id) = self.selected_entity {
                if !self.hierarchy_state.expanded_entities.contains(&entity_id) {
                    self.hierarchy_state.expanded_entities.insert(entity_id);
                }
            }
        }

        // Arrow Left - collapse selected entity (or select parent)
        if ui.key_pressed(katla_ui::input::KeyCode::ArrowLeft) {
            if let Some(entity_id) = self.selected_entity {
                if self.hierarchy_state.expanded_entities.contains(&entity_id) {
                    // Collapse if expanded
                    self.hierarchy_state.expanded_entities.remove(&entity_id);
                } else {
                    // Select parent if collapsed
                    if let Some(entity) = entities.iter().find(|e| e.id == entity_id) {
                        if let Some(parent_id) = entity.parent_id {
                            self.selected_entity = Some(parent_id);
                        }
                    }
                }
            }
        }

        // Escape - deselect entity
        if ui.key_pressed(katla_ui::input::KeyCode::Escape) {
            self.selected_entity = None;
        }

        let toolbar_height = 32.0;
        let status_bar_height = 24.0;

        // Asset browser height (0 if collapsed)
        let asset_browser_height = if self.asset_browser.collapsed {
            28.0 // Just the header when collapsed
        } else {
            self.asset_browser.panel_height
        };

        // === TOOLBAR (top) ===
        ui.add(Toolbar::new(
            screen_size,
            toolbar_height,
            &mut self.toolbar_state,
            &self.theme,
            preferences,
        ));
        // Take any actions from Toolbar
        self.pending_actions
            .append(&mut self.toolbar_state.pending_actions);

        // Panel Y range (between toolbar and asset browser, no gaps)
        let panel_top = toolbar_height; // Just after toolbar border
        let panel_bottom = screen_size.y() - status_bar_height - asset_browser_height;
        let panel_height = panel_bottom - panel_top;

        // === PANEL RESIZE HANDLING ===
        let resize_handle_width = 5.0;
        let min_panel_width = 150.0;
        let min_viewport_width = 200.0;
        let min_asset_browser_height = 100.0;

        // Left panel resize handle (right edge of left panel)
        let left_resize_bounds = Rect2D::from_origin_size(
            Vec2::new(self.left_panel_width - resize_handle_width / 2.0, panel_top),
            Vec2::new(resize_handle_width, panel_height),
        );

        // Right panel resize handle (left edge of right panel)
        let right_panel_x = screen_size.x() - self.right_panel_width;
        let right_resize_bounds = Rect2D::from_origin_size(
            Vec2::new(right_panel_x - resize_handle_width / 2.0, panel_top),
            Vec2::new(resize_handle_width, panel_height),
        );

        // Asset browser resize handle (top edge)
        let asset_resize_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_bottom - resize_handle_width / 2.0),
            Vec2::new(screen_size.x(), resize_handle_width),
        );

        // Handle ongoing resize
        if let Some(resize_edge) = self.resizing_panel {
            if ui.mouse_down(mouse_button::LEFT) {
                let mouse_x = ui.mouse_pos().x();
                let mouse_y = ui.mouse_pos().y();

                match resize_edge {
                    PanelResizeEdge::LeftPanelRight => {
                        let max_width =
                            (screen_size.x() - self.right_panel_width - min_viewport_width)
                                .max(min_panel_width);
                        self.left_panel_width = mouse_x.clamp(min_panel_width, max_width).round();
                    }
                    PanelResizeEdge::RightPanelLeft => {
                        let min_x = self.left_panel_width + min_viewport_width;
                        let max_width = (screen_size.x() - min_x).max(min_panel_width);
                        self.right_panel_width = (screen_size.x() - mouse_x)
                            .clamp(min_panel_width, max_width)
                            .round();
                    }
                    PanelResizeEdge::AssetBrowserTop => {
                        let max_height = (screen_size.y()
                            - status_bar_height
                            - toolbar_height
                            - min_viewport_width)
                            .max(min_asset_browser_height);
                        self.asset_browser.panel_height =
                            (screen_size.y() - mouse_y - status_bar_height)
                                .clamp(min_asset_browser_height, max_height)
                                .round();
                    }
                }
            } else {
                self.resizing_panel = None;
            }
        }

        // Check for resize handle hover and start dragging
        if self.resizing_panel.is_none() {
            if ui.is_hovered(left_resize_bounds) {
                ui.set_mouse_cursor(katla_ui::input::MouseCursor::ResizeHorizontal);
                if ui.mouse_clicked(mouse_button::LEFT) {
                    self.resizing_panel = Some(PanelResizeEdge::LeftPanelRight);
                }
            } else if ui.is_hovered(right_resize_bounds) {
                ui.set_mouse_cursor(katla_ui::input::MouseCursor::ResizeHorizontal);
                if ui.mouse_clicked(mouse_button::LEFT) {
                    self.resizing_panel = Some(PanelResizeEdge::RightPanelLeft);
                }
            } else if ui.is_hovered(asset_resize_bounds) && !self.asset_browser.collapsed {
                ui.set_mouse_cursor(katla_ui::input::MouseCursor::ResizeVertical);
                if ui.mouse_clicked(mouse_button::LEFT) {
                    self.resizing_panel = Some(PanelResizeEdge::AssetBrowserTop);
                }
            }
        }

        // === LEFT PANEL: Entity Hierarchy ===
        let left_panel_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_top),
            Vec2::new(self.left_panel_width, panel_height),
        );
        ui.add(hierarchy::Hierarchy::new(
            left_panel_bounds,
            &mut self.hierarchy_state,
            &mut self.selected_entity,
            entities,
            &mut self.focused_panel,
            &mut self.pending_actions,
            &self.theme,
        ));

        // Left panel right border
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(self.left_panel_width, panel_top),
                Vec2::new(1.0, panel_height),
            ),
            Color::new(0.3, 0.3, 0.3, 1.0),
        );

        // === RIGHT PANEL: Properties Inspector ===
        let right_panel_bounds = Rect2D::from_origin_size(
            Vec2::new(right_panel_x, panel_top),
            Vec2::new(self.right_panel_width, panel_height),
        );
        ui.add(inspector::Inspector::new(
            right_panel_bounds,
            &mut self.selected_entity,
            entities,
            &mut self.focused_panel,
            &mut self.pending_actions,
            &self.theme,
        ));

        // Right panel left border
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(right_panel_x - 1.0, panel_top),
                Vec2::new(1.0, panel_height),
            ),
            Color::new(0.3, 0.3, 0.3, 1.0),
        );

        // === CENTER: Viewport Area ===
        let viewport_bounds = Rect2D::new(
            Vec2::new(self.left_panel_width + 1.0, panel_top),
            Vec2::new(right_panel_x - 1.0, panel_bottom),
        );

        // Track viewport size for render target sizing
        self.last_viewport_size = (
            viewport_bounds.width().max(1.0) as u32,
            viewport_bounds.height().max(1.0) as u32,
        );

        // Draw viewport grid
        let grid_response = ui.add(viewport_grid::ViewportGrid::new(
            viewport_bounds,
            &self.viewport_grid_state,
            &self.viewport_texture_ids,
            &self.theme,
            &mut self.focused_panel,
        ));

        // Update active viewport based on hover
        if grid_response.hovered {
            let min = viewport_bounds.min;
            let max = viewport_bounds.max;
            crate::input::update_active_viewport(
                &mut self.viewport_grid_state,
                ui.mouse_pos(),
                min,
                max,
            );
        }

        // === ASSET BROWSER (bottom panel) ===
        let asset_browser_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_bottom),
            Vec2::new(screen_size.x(), asset_browser_height),
        );
        build_asset_browser(
            &mut self.asset_browser,
            ui,
            &self.theme,
            asset_browser_bounds,
            &mut self.focused_panel,
            loader,
            thumbnail_texture_handles,
        );

        // Process asset browser actions
        for action in self.asset_browser.take_actions() {
            match action {
                AssetAction::DragToViewport {
                    path,
                    asset_type,
                    screen_pos,
                } => {
                    // Check if dropped in viewport area (not in panels)
                    if viewport_bounds.contains(screen_pos) {
                        // Determine what to spawn based on asset type
                        match asset_type {
                            AssetType::Model => {
                                // Store the path for model loading (will be handled by application)
                                self.pending_actions.push(EditorAction::SpawnModelAtPath {
                                    path: path.clone(),
                                    position: Vec3::new(0.0, 0.0, 0.0), // TODO: Raycast for world position
                                });
                            }
                            _ => {
                                // For other asset types, spawn a cube as placeholder
                                self.pending_actions.push(EditorAction::SpawnModel(
                                    SpawnableModel::Cube,
                                    Vec3::new(0.0, 0.0, 0.0),
                                ));
                            }
                        }
                    }
                }
                AssetAction::ModelPreviewRequested(_path) => {
                    // Model preview functionality removed - log for now
                    log::debug!("Model preview requested but feature is disabled");
                }
                AssetAction::CreateFolder(parent_path) => {
                    // Create "New Folder" in the specified directory
                    let mut new_folder = parent_path.join("New Folder");
                    let mut counter = 1;
                    while new_folder.exists() {
                        new_folder = parent_path.join(format!("New Folder {}", counter));
                        counter += 1;
                    }
                    if let Err(e) = std::fs::create_dir(&new_folder) {
                        log::warn!("Failed to create folder: {}", e);
                    } else {
                        log::info!("Created folder: {:?}", new_folder);
                        self.asset_browser.scan_directory(thumbnail_texture_handles);
                    }
                }
                AssetAction::Delete(path) => {
                    if path.is_dir() {
                        if let Err(e) = std::fs::remove_dir_all(&path) {
                            log::warn!("Failed to delete folder: {}", e);
                        } else {
                            log::info!("Deleted folder: {:?}", path);
                            self.asset_browser.scan_directory(thumbnail_texture_handles);
                        }
                    } else if let Err(e) = std::fs::remove_file(&path) {
                        log::warn!("Failed to delete file: {}", e);
                    } else {
                        log::info!("Deleted file: {:?}", path);
                        self.asset_browser.scan_directory(thumbnail_texture_handles);
                    }
                }
                AssetAction::Rename { old_path, new_path } => {
                    // Rename file or folder
                    if old_path != new_path {
                        if let Err(e) = std::fs::rename(&old_path, &new_path) {
                            log::warn!("Failed to rename {:?} to {:?}: {}", old_path, new_path, e);
                        } else {
                            log::info!("Renamed {:?} to {:?}", old_path, new_path);
                            self.asset_browser.scan_directory(thumbnail_texture_handles);
                        }
                    }
                }
                AssetAction::Open(path) => {
                    // Navigate into folder or open file
                    if path.is_dir() {
                        self.asset_browser
                            .navigate_to(&path, thumbnail_texture_handles);
                    } else {
                        log::info!("Open file: {:?}", path);
                        // TODO: Open file in appropriate editor
                    }
                }
                AssetAction::CopyPath(path) => {
                    // Copy path as string (log for now, clipboard not implemented)
                    log::info!("Copy path: {:?}", path);
                    // TODO: Implement clipboard when available
                }
                AssetAction::ShowInExplorer(path) => {
                    // Open file manager at location
                    #[cfg(target_os = "windows")]
                    {
                        if let Err(e) = std::process::Command::new("explorer")
                            .args(["/select,", &path.to_string_lossy()])
                            .spawn()
                        {
                            log::warn!("Failed to open explorer: {}", e);
                        }
                    }
                    #[cfg(target_os = "macos")]
                    {
                        if let Err(e) = std::process::Command::new("open")
                            .args(["-R", &path.to_string_lossy()])
                            .spawn()
                        {
                            log::warn!("Failed to open finder: {}", e);
                        }
                    }
                    #[cfg(target_os = "linux")]
                    {
                        if let Err(e) = std::process::Command::new("xdg-open")
                            .arg(path.parent().unwrap_or(&path))
                            .spawn()
                        {
                            log::warn!("Failed to open file manager: {}", e);
                        }
                    }
                }
                AssetAction::MoveToFolder {
                    asset_path,
                    folder_path,
                } => {
                    // Move file/folder to destination folder
                    let file_name = asset_path.file_name().unwrap_or_default();
                    let dest_path = folder_path.join(file_name);
                    if asset_path != dest_path {
                        if let Err(e) = std::fs::rename(&asset_path, &dest_path) {
                            log::warn!("Failed to move {:?} to {:?}: {}", asset_path, dest_path, e);
                        } else {
                            log::info!("Moved {:?} to {:?}", asset_path, dest_path);
                            self.asset_browser.scan_directory(thumbnail_texture_handles);
                        }
                    }
                }
            }
        }

        // Asset browser top border
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(0.0, panel_bottom),
                Vec2::new(screen_size.x(), 1.0),
            ),
            Color::new(0.3, 0.3, 0.3, 1.0),
        );

        // Status bar top border (fills gap)
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(0.0, panel_bottom),
                Vec2::new(screen_size.x(), 1.0),
            ),
            Color::new(0.3, 0.3, 0.3, 1.0),
        );

        // === STATUS BAR (bottom) ===
        // Count selected items (selected_index is the primary, selected_indices are multi-select)
        let selected_count = if self.asset_browser.selected_indices.is_empty() {
            if self.asset_browser.selected_index.is_some() {
                1
            } else {
                0
            }
        } else {
            self.asset_browser.selected_indices.len()
        };
        let total_assets = self.asset_browser.assets.len();
        ui.add(status_bar::StatusBar::new(
            screen_size,
            status_bar_height,
            fps,
            frame_count,
            entities.len(),
            selected_count,
            total_assets,
            self.is_playing,
            &self.theme,
        ));

        // === PREFERENCES PANEL (overlay) ===
        if self.preferences_panel_state.visibility.is_visible() {
            let theme_key = self.theme_key();
            let mut actions = Vec::new();
            ui.add(PreferencesPanel::new(
                screen_size,
                &mut self.preferences_panel_state,
                preferences,
                &self.editor_settings,
                &self.theme,
                theme_key,
                &mut actions,
            ));

            for action in actions {
                self.apply_preferences_action(action);
            }
        }

        // === PARTICLE INSPECTOR PANEL (overlay) ===
        if self.show_particle_inspector {
            use crate::ui::ParticleInspector;

            let panel_width = 320.0;
            let panel_height = 600.0;
            let panel_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    screen_size.x() - panel_width - 20.0,
                    screen_size.y() - panel_height - 60.0,
                ),
                Vec2::new(panel_width, panel_height),
            );

            let _particle_inspector = ParticleInspector::new(
                panel_bounds,
                &mut self.selected_particle_emitter,
                &self.theme,
            );

            // Note: We'll render this in the application layer where we have access to World and particle system
            // For now, just draw the panel bounds
            ui.draw_rect(panel_bounds, self.theme.panel_bg);
            ui.draw_rect_border(
                panel_bounds,
                self.theme.panel_bg,
                self.theme.panel_border,
                1.0,
            );

            let header_height = 24.0;
            let header_bounds = Rect2D::from_origin_size(
                panel_bounds.min,
                Vec2::new(panel_bounds.width(), header_height),
            );
            ui.draw_rect(header_bounds, self.theme.panel_header);

            let header_pos =
                Vec2::new(panel_bounds.min.x() + 8.0, header_bounds.center().y() - 7.0);
            ui.draw_text(
                "Particle Inspector",
                header_pos,
                self.theme.text_primary,
                ui.scaled_font_size(FontSize::Medium),
            );
        }

        // === DRAG PREVIEW (rendered last to appear above all panels) ===
        if self.asset_browser.is_dragging {
            if let Some(drag_idx) = self.asset_browser.drag_asset {
                if let Some(asset) = self.asset_browser.assets.get(drag_idx) {
                    let mouse_pos = ui.mouse_pos();

                    // Preview size
                    let preview_size = 64.0;
                    let preview_offset = Vec2::new(preview_size * 0.5, preview_size * 0.5);

                    // Draw preview at cursor position with highest z-index
                    ui.with_z_index(katla_ui::z_index::TOOLTIP, |ui| {
                        // Semi-transparent background
                        let preview_bounds = Rect2D::from_origin_size(
                            mouse_pos - preview_offset,
                            Vec2::new(preview_size, preview_size),
                        );
                        ui.draw_rect(preview_bounds, self.theme.background.with_alpha(0.9));
                        ui.draw_rect_border(
                            preview_bounds,
                            self.theme.background.with_alpha(0.9),
                            self.theme.highlight,
                            2.0,
                        );

                        // Draw icon
                        let icon_char = asset.asset_type.icon();
                        let icon_size = preview_size * 0.4;
                        ui.draw_icon(
                            icon_char,
                            Vec2::new(
                                preview_bounds.center().x() - icon_size * 0.5,
                                preview_bounds.center().y() - icon_size * 0.5 - 8.0,
                            ),
                            icon_size,
                            self.theme.highlight,
                        );

                        // Draw name (truncated)
                        let max_chars = 12;
                        let display_name = if asset.name.len() > max_chars {
                            format!("{}...", &asset.name[..max_chars])
                        } else {
                            asset.name.clone()
                        };
                        ui.draw_text(
                            &display_name,
                            Vec2::new(
                                preview_bounds.min.x() + 4.0,
                                preview_bounds.min.y() + preview_size - 16.0,
                            ),
                            self.theme.text_primary,
                            ui.scaled_font_size(FontSize::XSmall),
                        );
                    });
                }
            }
        }
    }

    /// Render the editor UI and return the draw list.
    #[allow(clippy::too_many_arguments)]
    pub fn render<'a>(
        &'a mut self,
        ui: &'a mut UiContext,
        preferences: &Preferences,
        screen_size: Vec2,
        scale_factor: f32,
        entities: &'a [EntityInfo],
        fps: f32,
        frame_count: usize,
        loader: &'a mut crate::util::BackgroundLoader,
        thumbnail_texture_handles: &'a std::collections::HashMap<std::path::PathBuf, TextureHandle>,
    ) -> &'a DrawList {
        // Apply theme to UI style
        self.theme.apply_to_style(&mut ui.style);

        // Apply font scale for accessibility
        ui.set_font_scale(self.font_scale);

        ui.begin(screen_size, scale_factor);
        self.build(
            ui,
            preferences,
            entities,
            fps,
            frame_count,
            loader,
            thumbnail_texture_handles,
        );
        ui.end()
    }

    /// Take pending actions, clearing the list.
    pub fn take_actions(&mut self) -> Vec<EditorAction> {
        std::mem::take(&mut self.pending_actions)
    }
}

impl Default for EditorUI {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::editor_ui::hierarchy::Hierarchy;
    use crate::ui::editor_ui::preferences::PreferencesTab;

    /// Test that preferences panel stays open on the first frame after being opened.
    ///
    /// This tests the fix for the bug where the preferences panel would close
    /// immediately after being opened from the menu bar, because the same
    /// click that opened it would be detected as a "click outside".
    ///
    /// BEFORE FIX: Panel would close because mouse_clicked=true and mouse_in_panel=false
    /// AFTER FIX: Panel stays open because PanelState::JustOpened protects it
    #[test]
    fn test_preferences_stays_open_on_first_frame_after_menu_click() {
        let mut editor = EditorUI::new();

        // Simulate opening preferences from menu - this is what happens when
        // user clicks "Preferences..." menu item
        editor.preferences_panel_state.visibility.open();

        // Verify panel is in JustOpened state
        assert_eq!(
            editor.preferences_panel_state.visibility,
            PanelState::JustOpened
        );

        // Simulate the same frame: the click that opened the menu is still
        // registered as mouse_clicked=true, but mouse is not in the panel
        // (panel just appeared, mouse is still at menu position)
        let mouse_clicked = true;
        let mouse_in_panel = false;

        // Simulate the click-outside logic (now internal to PreferencesPanel)
        // Without JustOpened protection, panel would close
        if !editor.preferences_panel_state.dragging
            && !editor.preferences_panel_state.visibility.is_just_opened()
            && mouse_clicked
            && !mouse_in_panel
        {
            editor.preferences_panel_state.visibility.close();
            editor.preferences_panel_state.position = None;
        }
        editor.preferences_panel_state.visibility.mark_shown();

        // AFTER FIX: Panel should stay open because JustOpened protected it
        assert!(
            editor.preferences_panel_state.visibility.is_visible(),
            "preferences should stay open on first frame (JustOpened protection)"
        );
        assert_eq!(
            editor.preferences_panel_state.visibility,
            PanelState::Visible,
            "state should transition to Visible after first frame"
        );
    }

    /// Test that preferences panel can be closed by clicking outside after first frame.
    ///
    /// This verifies that the JustOpened protection doesn't prevent normal closing.
    #[test]
    fn test_preferences_closes_on_click_outside_after_first_frame() {
        let mut editor = EditorUI::new();

        // Open preferences and transition to Visible (simulating first frame already passed)
        editor.preferences_panel_state.visibility.open();
        editor.preferences_panel_state.visibility.mark_shown();
        assert_eq!(
            editor.preferences_panel_state.visibility,
            PanelState::Visible
        );

        // Simulate second frame: user clicks outside the panel
        let mouse_clicked = true;
        let mouse_in_panel = false;

        // Simulate the click-outside logic
        if !editor.preferences_panel_state.dragging
            && !editor.preferences_panel_state.visibility.is_just_opened()
            && mouse_clicked
            && !mouse_in_panel
        {
            editor.preferences_panel_state.visibility.close();
            editor.preferences_panel_state.position = None;
        }
        editor.preferences_panel_state.visibility.mark_shown();

        // Panel should close normally
        assert_eq!(
            editor.preferences_panel_state.visibility,
            PanelState::Hidden,
            "preferences should close when clicking outside after first frame"
        );
    }

    /// Test that clicking inside the panel doesn't close it.
    #[test]
    fn test_preferences_does_not_close_when_clicking_inside() {
        let mut editor = EditorUI::new();

        // Open and transition to Visible
        editor.preferences_panel_state.visibility.open();
        editor.preferences_panel_state.visibility.mark_shown();

        // Click inside the panel
        let mouse_clicked = true;
        let mouse_in_panel = true;

        // Simulate the click-outside logic
        if !editor.preferences_panel_state.dragging
            && !editor.preferences_panel_state.visibility.is_just_opened()
            && mouse_clicked
            && !mouse_in_panel
        {
            editor.preferences_panel_state.visibility.close();
            editor.preferences_panel_state.position = None;
        }
        editor.preferences_panel_state.visibility.mark_shown();

        assert!(
            editor.preferences_panel_state.visibility.is_visible(),
            "preferences should stay open when clicking inside panel"
        );
    }

    /// Test that dragging the panel prevents click-outside close.
    #[test]
    fn test_preferences_does_not_close_while_dragging() {
        let mut editor = EditorUI::new();

        // Open and transition to Visible
        editor.preferences_panel_state.visibility.open();
        editor.preferences_panel_state.visibility.mark_shown();
        editor.preferences_panel_state.dragging = true;

        // Click outside while dragging
        let mouse_clicked = true;
        let mouse_in_panel = false;

        // Simulate the click-outside logic
        if !editor.preferences_panel_state.dragging
            && !editor.preferences_panel_state.visibility.is_just_opened()
            && mouse_clicked
            && !mouse_in_panel
        {
            editor.preferences_panel_state.visibility.close();
            editor.preferences_panel_state.position = None;
        }
        editor.preferences_panel_state.visibility.mark_shown();

        assert!(
            editor.preferences_panel_state.visibility.is_visible(),
            "preferences should stay open while dragging panel"
        );
    }

    /// Test that no click means no close.
    #[test]
    fn test_preferences_does_not_close_without_click() {
        let mut editor = EditorUI::new();

        // Open and transition to Visible
        editor.preferences_panel_state.visibility.open();
        editor.preferences_panel_state.visibility.mark_shown();

        // No click happened
        let mouse_clicked = false;
        let mouse_in_panel = false;

        // Simulate the click-outside logic
        if !editor.preferences_panel_state.dragging
            && !editor.preferences_panel_state.visibility.is_just_opened()
            && mouse_clicked
            && !mouse_in_panel
        {
            editor.preferences_panel_state.visibility.close();
            editor.preferences_panel_state.position = None;
        }
        editor.preferences_panel_state.visibility.mark_shown();

        assert!(
            editor.preferences_panel_state.visibility.is_visible(),
            "preferences should stay open when no click occurred"
        );
    }

    /// Test that clicking a tab in the preferences panel doesn't dismiss the window.
    ///
    /// This tests the bug where clicking a tab button would trigger the click-outside
    /// logic and close the panel before the tab change could take effect.
    #[test]
    fn test_preferences_tab_click_does_not_close_panel() {
        let mut ui = UiContext::new();
        ui.begin(Vec2::new(800.0, 600.0), 1.0);

        let mut state = PreferencesPanelState {
            visibility: PanelState::Visible,
            position: Some(Vec2::new(100.0, 100.0)),
            dragging: false,
            drag_offset: Vec2::new(0.0, 0.0),
            current_tab: PreferencesTab::Appearance,
            scroll_state: Default::default(),
        };

        let preferences = crate::Preferences::default();
        let editor_settings = EditorSettings::default();
        let theme = Theme::default();
        let theme_key = "catppuccin";
        let mut actions = Vec::new();

        // Simulate clicking on the Editor tab (second tab)
        // The tab is at x=100 + 450/4 = 212.5, y=100+32 = 132
        let tab_x = 100.0 + 450.0 / 4.0;
        let tab_y = 100.0 + 32.0;

        // Frame 1: Mouse press on tab button
        ui.input.mouse_pos = Vec2::new(tab_x + 10.0, tab_y + 10.0);
        ui.input.mouse_pressed[mouse_button::LEFT] = true;
        ui.input.mouse_down[mouse_button::LEFT] = true;

        let panel = PreferencesPanel::new(
            Vec2::new(800.0, 600.0),
            &mut state,
            &preferences,
            &editor_settings,
            &theme,
            theme_key,
            &mut actions,
        );

        ui.add(panel);
        ui.end();

        // Frame 2: Mouse release (this is when the button click is registered)
        ui.input.clear_frame_state();
        ui.begin(Vec2::new(800.0, 600.0), 1.0);
        ui.input.mouse_pos = Vec2::new(tab_x + 10.0, tab_y + 10.0);
        ui.input.mouse_down[mouse_button::LEFT] = false; // Released
        ui.input.mouse_released[mouse_button::LEFT] = true;

        let panel = PreferencesPanel::new(
            Vec2::new(800.0, 600.0),
            &mut state,
            &preferences,
            &editor_settings,
            &theme,
            theme_key,
            &mut actions,
        );

        ui.add(panel);

        // The panel should remain open after tab click
        assert!(
            state.visibility.is_visible(),
            "preferences panel should stay open after clicking tab"
        );

        // The tab should have changed to Editor
        assert_eq!(
            state.current_tab,
            PreferencesTab::Editor,
            "tab should change to Editor after clicking it"
        );

        // No Close action should have been triggered
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, PreferencesAction::Close)),
            "tab click should not trigger Close action"
        );
    }

    /// Test that clicking an entity in the hierarchy panel selects it.
    ///
    /// This tests the bug where hierarchy items couldn't be selected.
    #[test]
    fn test_hierarchy_entity_selection_works() {
        let mut ui = UiContext::new();
        ui.begin(Vec2::new(800.0, 600.0), 1.0);

        let mut state = HierarchyState::default();
        let mut selected_entity = None;
        let mut focused_panel = FocusedPanel::None;
        let mut pending_actions = Vec::new();

        // Create a temporary world to get valid EntityIds
        let mut world = katla_ecs::World::new();
        let entity1 = world.create_entity();
        let entity2 = world.create_entity();

        let entities = vec![
            EntityInfo {
                id: entity1,
                name: "Cube".to_string(),
                position: Vec3::new(0.0, 0.0, 0.0),
                rotation: Vec3::new(0.0, 0.0, 0.0),
                scale: Vec3::new(1.0, 1.0, 1.0),
                entity_type: "Mesh".to_string(),
                components: vec![],
                depth: 0,
                has_children: false,
                parent_id: None,
            },
            EntityInfo {
                id: entity2,
                name: "Sphere".to_string(),
                position: Vec3::new(0.0, 0.0, 0.0),
                rotation: Vec3::new(0.0, 0.0, 0.0),
                scale: Vec3::new(1.0, 1.0, 1.0),
                entity_type: "Mesh".to_string(),
                components: vec![],
                depth: 0,
                has_children: false,
                parent_id: None,
            },
        ];

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 400.0));
        let theme = Theme::default();

        // Simulate clicking on the second entity (Sphere)
        // Item height is 22.0, header is 24.0
        let click_y = 24.0 + 4.0 + 22.0 + 11.0; // In the middle of second item
        ui.input.mouse_pos = Vec2::new(100.0, click_y);
        ui.input.mouse_pressed[mouse_button::LEFT] = true;
        ui.input.mouse_down[mouse_button::LEFT] = true;

        let hierarchy = Hierarchy::new(
            bounds,
            &mut state,
            &mut selected_entity,
            &entities,
            &mut focused_panel,
            &mut pending_actions,
            &theme,
        );

        ui.add(hierarchy);

        // The entity should be selected
        assert_eq!(
            selected_entity,
            Some(entity2),
            "clicking entity should select it"
        );

        // A SelectEntity action should have been emitted
        assert!(
            pending_actions
                .iter()
                .any(|a| matches!(a, EditorAction::SelectEntity(id) if *id == entity2)),
            "selecting entity should emit SelectEntity action"
        );
    }

    /// Test PanelState enum methods.
    #[test]
    fn test_panel_state_enum() {
        let mut state = PanelState::default();
        assert_eq!(state, PanelState::Hidden);
        assert!(!state.is_visible());
        assert!(!state.is_just_opened());

        state.open();
        assert_eq!(state, PanelState::JustOpened);
        assert!(state.is_visible());
        assert!(state.is_just_opened());

        state.mark_shown();
        assert_eq!(state, PanelState::Visible);
        assert!(state.is_visible());
        assert!(!state.is_just_opened());

        // mark_shown on Visible is idempotent
        state.mark_shown();
        assert_eq!(state, PanelState::Visible);

        state.close();
        assert_eq!(state, PanelState::Hidden);
        assert!(!state.is_visible());

        // mark_shown on Hidden does nothing
        state.mark_shown();
        assert_eq!(state, PanelState::Hidden);
    }
}
