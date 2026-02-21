//! Game Engine Editor UI
//!
//! A full game engine-style interface with:
//! - Entity Hierarchy panel (left)
//! - Viewport window (center)
//! - Properties/Inspector panel (right)
//! - Toolbar (top)
//! - Status bar (bottom)

use katla_ecs::EntityId;
use katla_math::{Color, Rect2D, Vec2, Vec3};
use katla_ui::{input::mouse_button, DrawList, FontId, FontSize, ForkAwesome, TextureId, UiContext};
use std::collections::HashSet;
use std::path::PathBuf;

use super::asset_browser::AssetBrowserState;
use super::model_preview::ModelPreviewState;
use super::theme::Theme;

/// Model types that can be spawned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnableModel {
    Fox,
    Cube,
    Sphere,
    Cylinder,
    Plane,
    Torus,
}

impl SpawnableModel {
    pub fn name(&self) -> &'static str {
        match self {
            SpawnableModel::Fox => "Fox",
            SpawnableModel::Cube => "Cube",
            SpawnableModel::Sphere => "Sphere",
            SpawnableModel::Cylinder => "Cylinder",
            SpawnableModel::Plane => "Plane",
            SpawnableModel::Torus => "Torus",
        }
    }

    pub fn all() -> &'static [SpawnableModel] {
        &[
            SpawnableModel::Fox,
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
}

/// Preferences panel tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferencesTab {
    #[default]
    Appearance,
    Editor,
    Keybindings,
    About,
}

impl PreferencesTab {
    pub fn all() -> &'static [PreferencesTab] {
        &[
            PreferencesTab::Appearance,
            PreferencesTab::Editor,
            PreferencesTab::Keybindings,
            PreferencesTab::About,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            PreferencesTab::Appearance => "Appearance",
            PreferencesTab::Editor => "Editor",
            PreferencesTab::Keybindings => "Keybindings",
            PreferencesTab::About => "About",
        }
    }

    pub fn icon(&self) -> char {
        match self {
            PreferencesTab::Appearance => ForkAwesome::PAINT_BRUSH,
            PreferencesTab::Editor => ForkAwesome::PENCIL,
            PreferencesTab::Keybindings => ForkAwesome::KEY,
            PreferencesTab::About => ForkAwesome::INFO_CIRCLE,
        }
    }
}

/// Action requested from the editor UI.
#[derive(Debug, Clone)]
pub enum EditorAction {
    /// Spawn a new model at the given position.
    SpawnModel(SpawnableModel, Vec3),
    /// Spawn a model from a specific file path.
    SpawnModelAtPath {
        path: PathBuf,
        position: Vec3,
    },
    /// Delete an entity.
    DeleteEntity(EntityId),
    /// Duplicate an entity.
    DuplicateEntity(EntityId),
    /// Select an entity.
    SelectEntity(EntityId),
    /// Move selected entity.
    MoveEntity(EntityId, Vec3),
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
    /// Toolbar.
    Toolbar,
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
    /// Show spawn menu.
    show_spawn_menu: bool,
    /// Spawn menu just opened this frame (skip click-outside check).
    spawn_menu_just_opened: bool,
    /// Show preferences panel.
    show_preferences: bool,
    /// Preferences panel position (None = centered).
    preferences_panel_pos: Option<Vec2>,
    /// Currently dragging panel title bar.
    dragging_panel: bool,
    /// Offset from panel top-left when dragging started.
    drag_offset: Vec2,
    /// Currently selected preferences tab.
    preferences_tab: PreferencesTab,
    /// Left panel (hierarchy) width in pixels.
    pub left_panel_width: f32,
    /// Right panel (inspector) width in pixels.
    pub right_panel_width: f32,
    /// Which panel resize handle is being dragged (if any).
    resizing_panel: Option<PanelResizeEdge>,
    /// Camera movement speed (for editor camera).
    pub camera_speed: f32,
    /// Snap to grid when moving entities.
    pub snap_to_grid: bool,
    /// Grid size for snapping.
    pub grid_size: f32,
    /// Play mode active.
    pub is_playing: bool,
    /// Grid visibility.
    pub show_grid: bool,
    /// Stats panel visible.
    pub show_stats: bool,
    /// Font scale multiplier (1.0 = 100%).
    pub font_scale: f32,
    /// Selected spawn model.
    selected_spawn: SpawnableModel,
    /// Spawn position input.
    spawn_pos: [f32; 3],
    /// Deferred actions to be processed by the application.
    pub pending_actions: Vec<EditorAction>,
    /// Last known viewport panel size (width, height) in pixels.
    last_viewport_size: (u32, u32),
    /// Entities expanded in the hierarchy tree.
    expanded_entities: HashSet<EntityId>,
    /// Hierarchy context menu open.
    hierarchy_context_menu_open: bool,
    /// Entity for hierarchy context menu.
    hierarchy_context_entity: Option<EntityId>,
    /// Current color theme.
    pub theme: Theme,
    /// Asset browser panel state.
    pub asset_browser: AssetBrowserState,
    /// Model preview panel state.
    pub model_preview: ModelPreviewState,
    /// Currently focused panel (receives keyboard input).
    pub focused_panel: FocusedPanel,
    /// Main viewport texture ID (set by application during setup).
    pub main_viewport_texture_id: katla_ui::TextureId,
}

impl EditorUI {
    pub fn new() -> Self {
        Self {
            visible: true,
            selected_entity: None,
            show_spawn_menu: false,
            spawn_menu_just_opened: false,
            show_preferences: false,
            preferences_panel_pos: None,
            dragging_panel: false,
            drag_offset: Vec2::new(0.0, 0.0),
            preferences_tab: PreferencesTab::default(),
            left_panel_width: 220.0,
            right_panel_width: 280.0,
            resizing_panel: None,
            camera_speed: 50.0,
            snap_to_grid: true,
            grid_size: 1.0,
            is_playing: false,
            show_grid: true,
            show_stats: true,
            font_scale: 1.0,
            selected_spawn: SpawnableModel::Fox,
            spawn_pos: [0.0, 0.0, 0.0],
            pending_actions: Vec::new(),
            last_viewport_size: (800, 600), // Default size
            expanded_entities: HashSet::new(),
            hierarchy_context_menu_open: false,
            hierarchy_context_entity: None,
            theme: Theme::catppuccin(), // Default to Catppuccin because it's sick
            asset_browser: AssetBrowserState::new(),
            model_preview: ModelPreviewState::new(),
            focused_panel: FocusedPanel::Viewport, // Default to viewport
            main_viewport_texture_id: katla_ui::TextureId::VIEWPORT, // Will be updated during setup
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

    /// Get scaled font size in pixels.
    fn font_px(&self, size: FontSize) -> f32 {
        size.to_pixels_scaled(self.font_scale)
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

    /// Get the last known viewport panel size in pixels.
    pub fn viewport_size(&self) -> (u32, u32) {
        self.last_viewport_size
    }

    /// Check if an entity is expanded in the hierarchy.
    pub fn is_expanded(&self, entity_id: EntityId) -> bool {
        self.expanded_entities.contains(&entity_id)
    }

    /// Toggle expansion of an entity.
    pub fn toggle_expand(&mut self, entity_id: EntityId) {
        if self.expanded_entities.contains(&entity_id) {
            self.expanded_entities.remove(&entity_id);
        } else {
            self.expanded_entities.insert(entity_id);
        }
    }

    /// Check if an entity should be visible (all ancestors are expanded).
    fn is_entity_visible(&self, entity: &EntityInfo, all_entities: &[EntityInfo]) -> bool {
        let mut current = entity.parent_id;
        while let Some(parent_id) = current {
            // If parent is collapsed, this entity is not visible
            if !self.expanded_entities.contains(&parent_id) {
                return false;
            }
            // Find parent's parent
            current = all_entities
                .iter()
                .find(|e| e.id == parent_id)
                .and_then(|e| e.parent_id);
        }
        true
    }

    /// Build the editor UI.
    pub fn build(
        &mut self,
        ui: &mut UiContext,
        entities: &[EntityInfo],
        fps: f32,
        frame_count: usize,
        loader: &mut crate::util::BackgroundLoader,
        thumbnail_texture_ids: &std::collections::HashMap<std::path::PathBuf, TextureId>,
    ) {
        let screen_size = ui.screen_size();

        // Get visible entities (respecting collapsed state) for keyboard navigation
        let visible_entities: Vec<EntityId> = entities
            .iter()
            .filter(|e| self.is_entity_visible(e, entities))
            .map(|e| e.id)
            .collect();

        // === KEYBOARD SHORTCUTS ===
        // Delete key - delete selected entity
        if ui.input.key_pressed(katla_ui::input::KeyCode::Delete) {
            if let Some(entity_id) = self.selected_entity {
                if entities.iter().any(|e| e.id == entity_id) {
                    self.pending_actions
                        .push(EditorAction::DeleteEntity(entity_id));
                    self.selected_entity = None;
                }
            }
        }

        // Arrow Up - select previous entity
        if ui.input.key_pressed(katla_ui::input::KeyCode::ArrowUp) {
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
        if ui.input.key_pressed(katla_ui::input::KeyCode::ArrowDown) {
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
        if ui.input.key_pressed(katla_ui::input::KeyCode::ArrowRight) {
            if let Some(entity_id) = self.selected_entity {
                if !self.expanded_entities.contains(&entity_id) {
                    self.expanded_entities.insert(entity_id);
                }
            }
        }

        // Arrow Left - collapse selected entity (or select parent)
        if ui.input.key_pressed(katla_ui::input::KeyCode::ArrowLeft) {
            if let Some(entity_id) = self.selected_entity {
                if self.expanded_entities.contains(&entity_id) {
                    // Collapse if expanded
                    self.expanded_entities.remove(&entity_id);
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
        if ui.input.key_pressed(katla_ui::input::KeyCode::Escape) {
            self.selected_entity = None;
        }

        let padding = 4.0; // Inner padding for content
        let toolbar_height = 32.0;
        let status_bar_height = 24.0;

        // Asset browser height (0 if collapsed)
        let asset_browser_height = if self.asset_browser.collapsed {
            28.0 // Just the header when collapsed
        } else {
            self.asset_browser.panel_height
        };

        // === TOOLBAR (top) ===
        self.build_toolbar(ui, screen_size, toolbar_height, padding);

        // Toolbar bottom border (fills gap)
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(0.0, toolbar_height),
                Vec2::new(screen_size.x(), 1.0),
            ),
            Color::new(0.3, 0.3, 0.3, 1.0),
        );

        // Panel Y range (between toolbar and asset browser, no gaps)
        let panel_top = toolbar_height + 1.0; // Just after toolbar border
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
            if ui.input.is_mouse_down(katla_ui::input::mouse_button::LEFT) {
                let mouse_x = ui.input.mouse_pos.x();
                let mouse_y = ui.input.mouse_pos.y();

                match resize_edge {
                    PanelResizeEdge::LeftPanelRight => {
                        let max_width = (screen_size.x() - self.right_panel_width - min_viewport_width).max(min_panel_width);
                        self.left_panel_width = mouse_x.clamp(min_panel_width, max_width).round();
                    }
                    PanelResizeEdge::RightPanelLeft => {
                        let min_x = self.left_panel_width + min_viewport_width;
                        let max_width = (screen_size.x() - min_x).max(min_panel_width);
                        self.right_panel_width = (screen_size.x() - mouse_x).clamp(min_panel_width, max_width).round();
                    }
                    PanelResizeEdge::AssetBrowserTop => {
                        let max_height = (screen_size.y() - status_bar_height - toolbar_height - min_viewport_width).max(min_asset_browser_height);
                        self.asset_browser.panel_height = (screen_size.y() - mouse_y - status_bar_height).clamp(min_asset_browser_height, max_height).round();
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
                if ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
                    self.resizing_panel = Some(PanelResizeEdge::LeftPanelRight);
                }
            } else if ui.is_hovered(right_resize_bounds) {
                ui.set_mouse_cursor(katla_ui::input::MouseCursor::ResizeHorizontal);
                if ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
                    self.resizing_panel = Some(PanelResizeEdge::RightPanelLeft);
                }
            } else if ui.is_hovered(asset_resize_bounds) && !self.asset_browser.collapsed {
                ui.set_mouse_cursor(katla_ui::input::MouseCursor::ResizeVertical);
                if ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
                    self.resizing_panel = Some(PanelResizeEdge::AssetBrowserTop);
                }
            }
        }

        // === LEFT PANEL: Entity Hierarchy ===
        let left_panel_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_top),
            Vec2::new(self.left_panel_width, panel_height),
        );
        self.build_hierarchy_panel(ui, entities, left_panel_bounds);

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
        self.build_inspector_panel(ui, entities, right_panel_bounds);

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

        self.build_viewport(ui, viewport_bounds);

        // === ASSET BROWSER (bottom panel) ===
        let asset_browser_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_bottom),
            Vec2::new(screen_size.x(), asset_browser_height),
        );
        super::asset_browser::build_asset_browser(
            &mut self.asset_browser,
            ui,
            &self.theme,
            asset_browser_bounds,
            &mut self.focused_panel,
            loader,
            thumbnail_texture_ids,
        );

        // Process asset browser actions
        for action in self.asset_browser.take_actions() {
            match action {
                super::asset_browser::AssetAction::DragToViewport { path, asset_type, screen_pos } => {
                    // Check if dropped in viewport area (not in panels)
                    if viewport_bounds.contains(screen_pos) {
                        // Determine what to spawn based on asset type
                        match asset_type {
                            super::asset_browser::AssetType::Model => {
                                // Store the path for model loading (will be handled by application)
                                self.pending_actions.push(EditorAction::SpawnModelAtPath {
                                    path: path.clone(),
                                    position: Vec3::new(0.0, 0.0, 0.0), // TODO: Raycast for world position
                                });
                            }
                            _ => {
                                // For other asset types, spawn a cube as placeholder
                                self.pending_actions.push(EditorAction::SpawnModel(SpawnableModel::Cube, Vec3::new(0.0, 0.0, 0.0)));
                            }
                        }
                    }
                }
                super::asset_browser::AssetAction::CreateFolder(parent_path) => {
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
                        self.asset_browser.scan_directory(thumbnail_texture_ids);
                    }
                }
                super::asset_browser::AssetAction::Delete(path) => {
                    if path.is_dir() {
                        if let Err(e) = std::fs::remove_dir_all(&path) {
                            log::warn!("Failed to delete folder: {}", e);
                        } else {
                            log::info!("Deleted folder: {:?}", path);
                            self.asset_browser.scan_directory(thumbnail_texture_ids);
                        }
                    } else if let Err(e) = std::fs::remove_file(&path) {
                        log::warn!("Failed to delete file: {}", e);
                    } else {
                        log::info!("Deleted file: {:?}", path);
                        self.asset_browser.scan_directory(thumbnail_texture_ids);
                    }
                }
                super::asset_browser::AssetAction::Rename { old_path, new_path } => {
                    // Rename file or folder
                    if old_path != new_path {
                        if let Err(e) = std::fs::rename(&old_path, &new_path) {
                            log::warn!("Failed to rename {:?} to {:?}: {}", old_path, new_path, e);
                        } else {
                            log::info!("Renamed {:?} to {:?}", old_path, new_path);
                            self.asset_browser.scan_directory(thumbnail_texture_ids);
                        }
                    }
                }
                super::asset_browser::AssetAction::Open(path) => {
                    // Navigate into folder or open file
                    if path.is_dir() {
                        self.asset_browser.navigate_to(&path, thumbnail_texture_ids);
                    } else {
                        log::info!("Open file: {:?}", path);
                        // TODO: Open file in appropriate editor
                    }
                }
                super::asset_browser::AssetAction::ModelPreviewRequested(path) => {
                    // Request model preview in the preview panel
                    log::info!("Model preview requested: {:?}", path);
                    let load_id = loader.request_model(path.clone());
                    self.model_preview.model_path = Some(path);
                    self.model_preview.load_state = super::model_preview::LoadState::Loading;
                    self.model_preview.load_id = Some(load_id);
                    self.model_preview.visible = true;
                    self.model_preview.model = None;
                    self.model_preview.stats = None;
                }
                super::asset_browser::AssetAction::CopyPath(path) => {
                    // Copy path as string (log for now, clipboard not implemented)
                    log::info!("Copy path: {:?}", path);
                    // TODO: Implement clipboard when available
                }
                super::asset_browser::AssetAction::ShowInExplorer(path) => {
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
                super::asset_browser::AssetAction::MoveToFolder { asset_path, folder_path } => {
                    // Move file/folder to destination folder
                    let file_name = asset_path.file_name().unwrap_or_default();
                    let dest_path = folder_path.join(file_name);
                    if asset_path != dest_path {
                        if let Err(e) = std::fs::rename(&asset_path, &dest_path) {
                            log::warn!("Failed to move {:?} to {:?}: {}", asset_path, dest_path, e);
                        } else {
                            log::info!("Moved {:?} to {:?}", asset_path, dest_path);
                            self.asset_browser.scan_directory(thumbnail_texture_ids);
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
            if self.asset_browser.selected_index.is_some() { 1 } else { 0 }
        } else {
            self.asset_browser.selected_indices.len()
        };
        let total_assets = self.asset_browser.assets.len();
        self.build_status_bar(
            ui,
            screen_size,
            status_bar_height,
            fps,
            frame_count,
            entities.len(),
            selected_count,
            total_assets,
        );

        // === PREFERENCES PANEL (overlay) ===
        if self.show_preferences {
            self.build_preferences_panel(ui, screen_size);
        }

        // === MODEL PREVIEW PANEL (overlay) ===
        if self.model_preview.visible {
            self.build_model_preview_panel(ui, screen_size);
        }

        // === DRAG PREVIEW (rendered last to appear above all panels) ===
        if self.asset_browser.is_dragging {
            if let Some(drag_idx) = self.asset_browser.drag_asset {
                if let Some(asset) = self.asset_browser.assets.get(drag_idx) {
                    let mouse_pos = ui.input.mouse_pos;

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
                        ui.draw_rect_border(preview_bounds, self.theme.background.with_alpha(0.9), self.theme.highlight, 2.0);

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

    fn build_toolbar(&mut self, ui: &mut UiContext, screen_size: Vec2, height: f32, _padding_arg: f32) {
        let theme = &self.theme;
        let toolbar_bounds =
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(screen_size.x(), height));

        // Darker toolbar background
        ui.draw_rect(toolbar_bounds, theme.background_dark);
        ui.draw_line(
            Vec2::new(0.0, height),
            Vec2::new(screen_size.x(), height),
            theme.separator,
            1.0,
        );

        // Make menu bar items not have background by default (only on hover/active)
        let original_button_normal = ui.style.button_normal;
        ui.style.button_normal = Color::TRANSPARENT;

        // Internal padding for non-menu items
        let padding = 4.0;

        // No padding between menu items - menu bar should be tight
        let menu_item_width = 50.0;
        let dropdown_width = 120.0;
        let button_height = height;
        let mut cursor = Vec2::new(0.0, 0.0);  // Start from left edge

        // === FILE MENU ===
        let file_bounds = Rect2D::from_origin_size(cursor, Vec2::new(menu_item_width, button_height));
        if ui.begin_menu_item("file_menu", "File", file_bounds) {
            let item_height = 24.0;
            let mut item_y = file_bounds.max.y();

            // New Scene (placeholder)
            let new_bounds = Rect2D::from_origin_size(
                Vec2::new(file_bounds.min.x(), item_y),
                Vec2::new(dropdown_width, item_height),
            );
            if ui.menu_item("file_new", "New Scene", new_bounds) {
                // TODO: Implement new scene
                ui.close_current_popup();
            }
            item_y += item_height;

            // Open (placeholder)
            let open_bounds = Rect2D::from_origin_size(
                Vec2::new(file_bounds.min.x(), item_y),
                Vec2::new(dropdown_width, item_height),
            );
            if ui.menu_item("file_open", "Open...", open_bounds) {
                // TODO: Implement open scene
                ui.close_current_popup();
            }
            item_y += item_height;

            // Save (placeholder)
            let save_bounds = Rect2D::from_origin_size(
                Vec2::new(file_bounds.min.x(), item_y),
                Vec2::new(dropdown_width, item_height),
            );
            if ui.menu_item("file_save", "Save", save_bounds) {
                // TODO: Implement save scene
                ui.close_current_popup();
            }
            item_y += item_height;

            // Separator
            item_y += 4.0;

            // Quit
            let quit_bounds = Rect2D::from_origin_size(
                Vec2::new(file_bounds.min.x(), item_y),
                Vec2::new(dropdown_width, item_height),
            );
            if ui.menu_item("file_quit", "Quit", quit_bounds) {
                // Quit is handled at application level
                ui.close_current_popup();
            }

            ui.end_dropdown();
        }
        cursor = Vec2::new(cursor.x() + menu_item_width, cursor.y());  // No padding

        // === EDIT MENU ===
        let edit_bounds = Rect2D::from_origin_size(cursor, Vec2::new(menu_item_width, button_height));
        if ui.begin_menu_item("edit_menu", "Edit", edit_bounds) {
            let item_height = 24.0;
            let mut item_y = edit_bounds.max.y();

            // Undo (placeholder)
            let undo_bounds = Rect2D::from_origin_size(
                Vec2::new(edit_bounds.min.x(), item_y),
                Vec2::new(dropdown_width, item_height),
            );
            if ui.menu_item("edit_undo", "Undo", undo_bounds) {
                // TODO: Implement undo
                ui.close_current_popup();
            }
            item_y += item_height;

            // Redo (placeholder)
            let redo_bounds = Rect2D::from_origin_size(
                Vec2::new(edit_bounds.min.x(), item_y),
                Vec2::new(dropdown_width, item_height),
            );
            if ui.menu_item("edit_redo", "Redo", redo_bounds) {
                // TODO: Implement redo
                ui.close_current_popup();
            }
            item_y += item_height;

            // Separator
            item_y += 4.0;

            // Preferences
            let prefs_bounds = Rect2D::from_origin_size(
                Vec2::new(edit_bounds.min.x(), item_y),
                Vec2::new(dropdown_width, item_height),
            );
            if ui.menu_item("edit_prefs", "Preferences...", prefs_bounds) {
                self.show_preferences = true;
                ui.close_current_popup();
            }

            ui.end_dropdown();
        }
        cursor = Vec2::new(cursor.x() + menu_item_width, cursor.y());  // No padding

        // === VIEW MENU ===
        let view_bounds = Rect2D::from_origin_size(cursor, Vec2::new(menu_item_width, button_height));
        if ui.begin_menu_item("view_menu", "View", view_bounds) {
            let item_height = 24.0;
            let mut item_y = view_bounds.max.y();

            // Grid toggle
            let grid_bounds = Rect2D::from_origin_size(
                Vec2::new(view_bounds.min.x(), item_y),
                Vec2::new(dropdown_width, item_height),
            );
            if ui.toggle_menu_item("view_grid", "Grid", self.show_grid, grid_bounds) {
                self.show_grid = !self.show_grid;
                self.pending_actions.push(EditorAction::ToggleGrid);
                ui.close_current_popup();
            }
            item_y += item_height;

            // Stats toggle
            let stats_bounds = Rect2D::from_origin_size(
                Vec2::new(view_bounds.min.x(), item_y),
                Vec2::new(dropdown_width, item_height),
            );
            if ui.toggle_menu_item("view_stats", "Stats", self.show_stats, stats_bounds) {
                self.show_stats = !self.show_stats;
                self.pending_actions.push(EditorAction::ToggleStats);
                ui.close_current_popup();
            }

            ui.end_dropdown();
        }
        cursor = Vec2::new(cursor.x() + menu_item_width, cursor.y());  // No padding

        // === CREATE MENU ===
        let create_bounds = Rect2D::from_origin_size(cursor, Vec2::new(60.0, button_height));
        if ui.begin_menu_item("create_menu", "Create", create_bounds) {
            let item_height = 24.0;
            let mut item_y = create_bounds.max.y();

            for model in SpawnableModel::all() {
                let model_bounds = Rect2D::from_origin_size(
                    Vec2::new(create_bounds.min.x(), item_y),
                    Vec2::new(dropdown_width, item_height),
                );
                if ui.menu_item(&format!("create_{}", model.name()), model.name(), model_bounds) {
                    self.pending_actions
                        .push(EditorAction::SpawnModel(*model, Vec3::new(0.0, 0.0, 0.0)));
                    ui.close_current_popup();
                }
                item_y += item_height;
            }

            ui.end_dropdown();
        }
        cursor = Vec2::new(cursor.x() + 60.0 + padding, cursor.y());

        // === HELP MENU ===
        let help_bounds = Rect2D::from_origin_size(cursor, Vec2::new(menu_item_width, button_height));
        if ui.begin_menu_item("help_menu", "Help", help_bounds) {
            let item_height = 24.0;
            let item_y = help_bounds.max.y();

            // About
            let about_bounds = Rect2D::from_origin_size(
                Vec2::new(help_bounds.min.x(), item_y),
                Vec2::new(dropdown_width, item_height),
            );
            if ui.menu_item("help_about", "About", about_bounds) {
                // TODO: Show about dialog
                ui.close_current_popup();
            }

            ui.end_dropdown();
        }
        cursor = Vec2::new(cursor.x() + menu_item_width, cursor.y());  // No padding

        // Separator line before play controls
        cursor = Vec2::new(cursor.x() + padding * 2.0, cursor.y());
        ui.draw_line(
            Vec2::new(cursor.x(), padding),
            Vec2::new(cursor.x(), height - padding),
            theme.separator,
            1.0,
        );
        cursor = Vec2::new(cursor.x() + padding * 2.0, cursor.y());

        // Play/Pause button with icon
        let play_width = 70.0;
        let play_bounds = Rect2D::from_origin_size(cursor, Vec2::new(play_width, button_height));
        let play_color = if self.is_playing {
            theme.success
        } else {
            theme.button_bg
        };
        ui.draw_rect(play_bounds, play_color);

        // Draw play/pause icon and text (centered)
        let (play_icon, play_text) = if self.is_playing {
            (ForkAwesome::PAUSE, "Pause")
        } else {
            (ForkAwesome::PLAY, "Play")
        };
        let icon_size = 14.0;
        ui.draw_icon_text_centered(
            play_icon,
            play_text,
            play_bounds,
            icon_size,
            ui.scaled_font_size(FontSize::Small),
            theme.button_text,
        );

        if ui.button("play_btn", "", play_bounds) {
            self.is_playing = !self.is_playing;
            self.pending_actions.push(EditorAction::TogglePlay);
        }
        cursor = Vec2::new(cursor.x() + play_width + padding, cursor.y());

        // Title in center (only show if there's enough space)
        let title = "Katla Engine";
        let title_size = ui.measure_text(title, ui.scaled_font_size(FontSize::Medium));
        let title_pos = Vec2::new(
            screen_size.x() * 0.5 - title_size.x() * 0.5,
            height * 0.5 - title_size.y() * 0.5,
        );
        ui.draw_text(
            title,
            title_pos,
            theme.text_muted,
            ui.scaled_font_size(FontSize::Medium),
        );

        // Restore original button style
        ui.style.button_normal = original_button_normal;
    }

    fn build_hierarchy_panel(
        &mut self,
        ui: &mut UiContext,
        entities: &[EntityInfo],
        bounds: Rect2D,
    ) {
        let theme = self.theme.clone(); // Clone to avoid borrow issues

        // Focus this panel when clicked
        if ui.is_hovered(bounds) && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
            self.focused_panel = FocusedPanel::Hierarchy;
        }

        // Panel background
        ui.draw_rect(bounds, theme.panel_bg);
        ui.draw_rect_border(bounds, theme.panel_bg, theme.panel_border, 1.0);

        // Panel header
        let header_height = 24.0;
        let header_bounds =
            Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), header_height));
        ui.draw_rect(header_bounds, theme.panel_header);

        // Count visible entities (respecting collapsed state)
        let visible_count = entities
            .iter()
            .filter(|e| self.is_entity_visible(e, entities))
            .count();

        let header_text = format!("Hierarchy ({} entities)", visible_count);
        let header_pos = Vec2::new(bounds.min.x() + 8.0, header_bounds.center().y() - 7.0);
        ui.draw_text(
            &header_text,
            header_pos,
            theme.text_primary,
            ui.scaled_font_size(FontSize::Medium),
        );

        // Entity list with clipping
        let content_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.min.x(), bounds.min.y() + header_height),
            Vec2::new(bounds.width(), bounds.height() - header_height),
        );
        ui.push_clip(content_bounds);

        let mut cursor = Vec2::new(bounds.min.x(), bounds.min.y() + header_height + 4.0);
        let item_height = 22.0;
        let indent_per_level = 16.0;

        for entity in entities {
            // Skip entities whose ancestors are collapsed
            if !self.is_entity_visible(entity, entities) {
                continue;
            }

            // Calculate indentation based on depth
            let indent = entity.depth as f32 * indent_per_level;
            let item_x = bounds.min.x() + indent;
            let item_width = bounds.width() - indent;

            let item_bounds = Rect2D::from_origin_size(
                Vec2::new(item_x, cursor.y()),
                Vec2::new(item_width, item_height),
            );

            let is_selected = Some(entity.id) == self.selected_entity;
            let is_hovered = ui.is_hovered(item_bounds);

            let bg_color = if is_selected {
                theme.selection
            } else if is_hovered {
                theme.selection_hover
            } else {
                Color::TRANSPARENT
            };

            if bg_color != Color::TRANSPARENT {
                ui.draw_rect(item_bounds, bg_color);
            }

            // Tree line indicators for hierarchy
            if entity.depth > 0 {
                let line_x = item_x - 8.0;
                ui.draw_line(
                    Vec2::new(line_x, cursor.y()),
                    Vec2::new(line_x, cursor.y() + item_height),
                    theme.separator,
                    1.0,
                );
            }

            // Expand/collapse icon for entities with children
            let text_x = if entity.has_children {
                let is_expanded = self.expanded_entities.contains(&entity.id);
                let icon = if is_expanded {
                    ForkAwesome::CHEVRON_DOWN
                } else {
                    ForkAwesome::CHEVRON_RIGHT
                };
                let triangle_bounds = Rect2D::from_origin_size(
                    Vec2::new(item_x + 2.0, cursor.y()),
                    Vec2::new(16.0, item_height),
                );
                let triangle_hovered = ui.is_hovered(triangle_bounds);

                let triangle_color = if triangle_hovered {
                    theme.text_primary
                } else {
                    theme.text_secondary
                };

                let triangle_pos = Vec2::new(item_x + 3.0, cursor.y() + 3.0);
                ui.draw_icon_aligned(
                    icon,
                    triangle_pos,
                    ui.scaled_font_size(FontSize::Medium),
                    triangle_color,
                    FontId::DEFAULT,
                );

                // Click on triangle to toggle expand
                if ui.input.mouse_clicked(mouse_button::LEFT) && triangle_hovered {
                    self.toggle_expand(entity.id);
                }

                item_x + 18.0
            } else {
                // Leaf node - show a small dot indicator
                let dot_pos = Vec2::new(item_x + 6.0, cursor.y() + 8.0);
                ui.draw_rect(
                    Rect2D::from_origin_size(dot_pos, Vec2::new(4.0, 4.0)),
                    theme.text_muted,
                );
                item_x + 18.0
            };

            // Entity name with type icon
            let entity_icon = match entity.entity_type.as_str() {
                "Mesh" => ForkAwesome::CUBE,
                "Particle Emitter" => ForkAwesome::STAR,
                "Directional Light" => ForkAwesome::SUN,
                "Point Light" => ForkAwesome::LIGHTBULB,
                "Camera" => ForkAwesome::CAMERA,
                "Empty" => ForkAwesome::CIRCLE,
                _ => ForkAwesome::CUBE,
            };
            let entity_icon_color = match entity.entity_type.as_str() {
                "Mesh" => theme.entity_mesh,
                "Particle Emitter" => theme.entity_particle,
                "Directional Light" | "Point Light" => theme.entity_light,
                _ => theme.text_secondary,
            };

            // Draw entity type icon
            ui.draw_icon_aligned(
                entity_icon,
                Vec2::new(text_x, cursor.y() + 3.0),
                ui.scaled_font_size(FontSize::Medium),
                entity_icon_color,
                FontId::DEFAULT,
            );

            // Entity name (shifted right for icon)
            let name_text = &entity.name;
            let name_pos = Vec2::new(text_x + 16.0, cursor.y() + 3.0);
            ui.draw_text(
                name_text,
                name_pos,
                theme.text_secondary,
                ui.scaled_font_size(FontSize::Medium),
            );

            // Entity type badge with color coding from theme
            let badge_color = match entity.entity_type.as_str() {
                "Mesh" => theme.entity_mesh,
                "Particle Emitter" => theme.entity_particle,
                "Directional Light" | "Point Light" => theme.entity_light,
                _ => theme.entity_empty,
            };
            let badge_text = &entity.entity_type;
            let badge_size = ui.measure_text(badge_text, ui.scaled_font_size(FontSize::XSmall));
            let badge_pos = Vec2::new(item_bounds.max.x() - badge_size.x() - 8.0, cursor.y() + 5.0);
            ui.draw_text(
                badge_text,
                badge_pos,
                badge_color,
                ui.scaled_font_size(FontSize::XSmall),
            );

            // Click to select (but not on triangle or popup)
            let triangle_width = if entity.has_children { 18.0 } else { 0.0 };
            let select_bounds = Rect2D::from_origin_size(
                Vec2::new(item_x + triangle_width, cursor.y()),
                Vec2::new(item_width - triangle_width, item_height),
            );
            let select_hovered = ui.is_hovered(select_bounds);

            if ui.input.mouse_clicked(mouse_button::LEFT) && select_hovered && !ui.is_mouse_over_popup() {
                self.selected_entity = Some(entity.id);
                self.pending_actions
                    .push(EditorAction::SelectEntity(entity.id));
            }

            // Right-click for context menu (skip if popup already open)
            if ui.input.mouse_clicked(mouse_button::RIGHT) && is_hovered && !ui.has_open_popup() {
                self.selected_entity = Some(entity.id);
                self.hierarchy_context_entity = Some(entity.id);
                self.hierarchy_context_menu_open = true;
                ui.open_context_menu_at("hierarchy_context", ui.input.mouse_pos);
            }

            cursor = Vec2::new(cursor.x(), cursor.y() + item_height);

            // Stop if we've filled the panel
            if cursor.y() > bounds.max.y() - item_height {
                break;
            }
        }

        // Empty state
        if entities.is_empty() {
            let empty_text = "No entities in scene";
            let empty_size = ui.measure_text(empty_text, ui.scaled_font_size(FontSize::Medium));
            let empty_pos = Vec2::new(
                bounds.center().x() - empty_size.x() * 0.5,
                bounds.center().y() - empty_size.y() * 0.5,
            );
            ui.draw_text(
                empty_text,
                empty_pos,
                theme.text_muted,
                ui.scaled_font_size(FontSize::Medium),
            );
        }

        // === HIERARCHY CONTEXT MENU using popup system ===
        let hierarchy_menu_open = ui.is_context_menu_open("hierarchy_context");
        if self.hierarchy_context_menu_open && !hierarchy_menu_open {
            self.hierarchy_context_menu_open = false;
            self.hierarchy_context_entity = None;
        }

        // Render popup with automatic layout
        let clicked_action = ui.popup("hierarchy_context", |ui| {
            if ui.popup_item_with_shortcut("Duplicate", ForkAwesome::COPY, true, "Ctrl+D") { return Some("Duplicate"); }
            if ui.popup_item_with_shortcut("Rename", ForkAwesome::PENCIL, true, "F2") { return Some("Rename"); }
            ui.popup_separator();
            if ui.popup_item_with_shortcut("Delete", ForkAwesome::TRASH, true, "Del") { return Some("Delete"); }
            None::<&str>
        });

        // Process action
        if let Some(action) = clicked_action.flatten() {
            match action {
                "Duplicate" => {
                    if let Some(entity_id) = self.hierarchy_context_entity {
                        self.pending_actions.push(EditorAction::DuplicateEntity(entity_id));
                    }
                }
                "Rename" => {
                    // TODO: Implement rename mode
                }
                "Delete" => {
                    if let Some(entity_id) = self.hierarchy_context_entity {
                        self.pending_actions.push(EditorAction::DeleteEntity(entity_id));
                    }
                }
                _ => {}
            }
            self.hierarchy_context_menu_open = false;
        }

        ui.pop_clip();
    }

    fn build_inspector_panel(
        &mut self,
        ui: &mut UiContext,
        entities: &[EntityInfo],
        bounds: Rect2D,
    ) {
        let theme = &self.theme;

        // Focus this panel when clicked
        if ui.is_hovered(bounds) && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
            self.focused_panel = FocusedPanel::Inspector;
        }

        // Panel background
        ui.draw_rect(bounds, theme.panel_bg);
        ui.draw_rect_border(bounds, theme.panel_bg, theme.panel_border, 1.0);

        // Panel header
        let header_height = 24.0;
        let header_bounds =
            Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), header_height));
        ui.draw_rect(header_bounds, theme.panel_header);

        let header_pos = Vec2::new(bounds.min.x() + 8.0, header_bounds.center().y() - 7.0);
        ui.draw_text(
            "Inspector",
            header_pos,
            theme.text_primary,
            ui.scaled_font_size(FontSize::Medium),
        );

        // Find selected entity
        let selected = self
            .selected_entity
            .and_then(|id| entities.iter().find(|e| e.id == id));

        let mut cursor = Vec2::new(bounds.min.x() + 8.0, bounds.min.y() + header_height + 8.0);
        let line_height = 20.0;
        let label_width = 60.0;
        let _value_width = bounds.width() - label_width - 24.0;

        if let Some(entity) = selected {
            // Entity name header
            ui.draw_text(
                &entity.name,
                cursor,
                theme.text_primary,
                ui.scaled_font_size(FontSize::Large),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 8.0);

            // Separator
            ui.draw_line(
                Vec2::new(bounds.min.x() + 8.0, cursor.y()),
                Vec2::new(bounds.max.x() - 8.0, cursor.y()),
                theme.separator,
                1.0,
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + 8.0);

            // Transform section
            ui.draw_text(
                "Transform",
                cursor,
                theme.text_accent,
                ui.scaled_font_size(FontSize::Medium),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            // Position
            let pos_label_bounds =
                Rect2D::from_origin_size(cursor, Vec2::new(label_width, line_height));
            ui.label("Position:", pos_label_bounds);
            let pos_value_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + label_width, cursor.y()),
                Vec2::new(_value_width, line_height),
            );
            let pos_text = format!(
                "({:.2}, {:.2}, {:.2})",
                entity.position.x(),
                entity.position.y(),
                entity.position.z()
            );
            ui.label(&pos_text, pos_value_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            // Rotation
            let rot_label_bounds =
                Rect2D::from_origin_size(cursor, Vec2::new(label_width, line_height));
            ui.label("Rotation:", rot_label_bounds);
            let rot_value_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + label_width, cursor.y()),
                Vec2::new(_value_width, line_height),
            );
            let rot_text = format!(
                "({:.1}, {:.1}, {:.1})",
                entity.rotation.x(),
                entity.rotation.y(),
                entity.rotation.z()
            );
            ui.label(&rot_text, rot_value_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            // Scale
            let scale_label_bounds =
                Rect2D::from_origin_size(cursor, Vec2::new(label_width, line_height));
            ui.label("Scale:", scale_label_bounds);
            let scale_value_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + label_width, cursor.y()),
                Vec2::new(_value_width, line_height),
            );
            let scale_text = format!(
                "({:.2}, {:.2}, {:.2})",
                entity.scale.x(),
                entity.scale.y(),
                entity.scale.z()
            );
            ui.label(&scale_text, scale_value_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 8.0);

            // Separator
            ui.draw_line(
                Vec2::new(bounds.min.x() + 8.0, cursor.y()),
                Vec2::new(bounds.max.x() - 8.0, cursor.y()),
                theme.separator,
                1.0,
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + 8.0);

            // Entity type
            ui.draw_text(
                "Type",
                cursor,
                theme.text_accent,
                ui.scaled_font_size(FontSize::Medium),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            let type_text = format!("  {}", entity.entity_type);
            ui.draw_text(
                &type_text,
                cursor,
                theme.text_secondary,
                ui.scaled_font_size(FontSize::Medium),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 8.0);

            // Components list
            ui.draw_text(
                "Components",
                cursor,
                theme.text_accent,
                ui.scaled_font_size(FontSize::Medium),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            for component_name in &entity.components {
                let comp_text = format!("  {}", component_name);
                ui.draw_text(
                    &comp_text,
                    cursor,
                    theme.text_secondary,
                    ui.scaled_font_size(FontSize::Medium),
                );
                cursor = Vec2::new(cursor.x(), cursor.y() + line_height);
            }

            cursor = Vec2::new(cursor.x(), cursor.y() + 8.0);

            // Delete button
            let delete_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x() + 8.0, cursor.y()),
                Vec2::new(bounds.width() - 16.0, 28.0),
            );
            if ui.button("delete_entity", "Delete Entity", delete_bounds) {
                self.pending_actions
                    .push(EditorAction::DeleteEntity(entity.id));
                self.selected_entity = None;
            }
        } else {
            // No selection
            let no_selection = "No entity selected";
            let no_sel_size = ui.measure_text(no_selection, ui.scaled_font_size(FontSize::Medium));
            let no_sel_pos = Vec2::new(
                bounds.center().x() - no_sel_size.x() * 0.5,
                bounds.center().y() - no_sel_size.y() * 0.5,
            );
            ui.draw_text(
                no_selection,
                no_sel_pos,
                theme.text_muted,
                ui.scaled_font_size(FontSize::Medium),
            );
        }
    }

    fn build_viewport(&mut self, ui: &mut UiContext, bounds: Rect2D) {
        let theme = &self.theme;

        // Focus this panel when clicked
        if ui.is_hovered(bounds) && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
            self.focused_panel = FocusedPanel::Viewport;
        }

        // Draw the viewport texture (rendered 3D scene)
        // Use OPAQUE_IMAGE to force alpha = 1.0 (viewport may have 0 alpha from HDR)
        ui.draw_image(
            bounds,
            Vec2::new(0.0, 0.0), // uv_min
            Vec2::new(1.0, 1.0), // uv_max
            Color::OPAQUE_IMAGE,
            self.main_viewport_texture_id,
        );

        // Viewport border - draw ONLY the border lines, not a filled rect
        let border_width = 2.0;
        let border_color = theme.viewport_border;

        // Top border
        ui.draw_rect(
            Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), border_width)),
            border_color,
        );
        // Bottom border
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.max.y() - border_width),
                Vec2::new(bounds.width(), border_width),
            ),
            border_color,
        );
        // Left border
        ui.draw_rect(
            Rect2D::from_origin_size(bounds.min, Vec2::new(border_width, bounds.height())),
            border_color,
        );
        // Right border
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(bounds.max.x() - border_width, bounds.min.y()),
                Vec2::new(border_width, bounds.height()),
            ),
            border_color,
        );

        // Viewport label
        let vp_label = "3D View";
        let label_pos = Vec2::new(bounds.min.x() + 8.0, bounds.min.y() + 8.0);
        ui.draw_text(
            vp_label,
            label_pos,
            theme.text_muted,
            ui.scaled_font_size(FontSize::XSmall),
        );
    }

    fn build_status_bar(
        &mut self,
        ui: &mut UiContext,
        screen_size: Vec2,
        height: f32,
        fps: f32,
        frame_count: usize,
        entity_count: usize,
        selected_count: usize,
        total_assets: usize,
    ) {
        let theme = &self.theme;
        let bar_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, screen_size.y() - height),
            Vec2::new(screen_size.x(), height),
        );

        // Status bar background
        ui.draw_rect(bar_bounds, theme.background_dark);
        ui.draw_line(
            bar_bounds.min,
            Vec2::new(screen_size.x(), bar_bounds.min.y()),
            theme.separator,
            1.0,
        );

        let mut cursor = Vec2::new(8.0, bar_bounds.min.y() + 4.0);

        // FPS with theme colors
        let fps_text = format!("FPS: {:.0}", fps);
        let fps_color = if fps >= 55.0 {
            theme.success
        } else if fps >= 30.0 {
            theme.warning
        } else {
            theme.error
        };
        ui.draw_text(
            &fps_text,
            cursor,
            fps_color,
            ui.scaled_font_size(FontSize::Small),
        );

        // Separator
        cursor = Vec2::new(cursor.x() + 70.0, cursor.y());
        ui.draw_text(
            "|",
            cursor,
            theme.text_muted,
            ui.scaled_font_size(FontSize::Small),
        );
        cursor = Vec2::new(cursor.x() + 15.0, cursor.y());

        // Frame count
        let frame_text = format!("Frame: {}", frame_count);
        ui.draw_text(
            &frame_text,
            cursor,
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Small),
        );

        // Separator
        cursor = Vec2::new(cursor.x() + 100.0, cursor.y());
        ui.draw_text(
            "|",
            cursor,
            theme.text_muted,
            ui.scaled_font_size(FontSize::Small),
        );
        cursor = Vec2::new(cursor.x() + 15.0, cursor.y());

        // Entity count
        let entity_text = format!("Entities: {}", entity_count);
        ui.draw_text(
            &entity_text,
            cursor,
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Small),
        );

        // Separator
        cursor = Vec2::new(cursor.x() + 100.0, cursor.y());
        ui.draw_text(
            "|",
            cursor,
            theme.text_muted,
            ui.scaled_font_size(FontSize::Small),
        );
        cursor = Vec2::new(cursor.x() + 15.0, cursor.y());

        // Selected items count
        let selection_text = if selected_count > 0 {
            format!("Selected: {} / {}", selected_count, total_assets)
        } else {
            format!("Assets: {}", total_assets)
        };
        ui.draw_text(
            &selection_text,
            cursor,
            if selected_count > 0 { theme.highlight } else { theme.text_secondary },
            ui.scaled_font_size(FontSize::Small),
        );

        // Play mode indicator on right side
        let mode_text = if self.is_playing {
            "PLAYING"
        } else {
            "EDITING"
        };
        let mode_color = if self.is_playing {
            theme.success
        } else {
            theme.text_secondary
        };
        let mode_size = ui.measure_text(mode_text, ui.scaled_font_size(FontSize::Small));
        let mode_pos = Vec2::new(screen_size.x() - mode_size.x() - 8.0, cursor.y());
        ui.draw_text(
            mode_text,
            mode_pos,
            mode_color,
            ui.scaled_font_size(FontSize::Small),
        );

        // Theme name display
        let theme_text = format!("Theme: {}", theme.name);
        let theme_size = ui.measure_text(&theme_text, ui.scaled_font_size(FontSize::Small));
        let theme_pos = Vec2::new(
            screen_size.x() - mode_size.x() - theme_size.x() - 100.0,
            cursor.y(),
        );
        ui.draw_text(
            &theme_text,
            theme_pos,
            theme.text_muted,
            ui.scaled_font_size(FontSize::Small),
        );
    }

    fn build_preferences_panel(&mut self, ui: &mut UiContext, screen_size: Vec2) {
        let theme = self.theme.clone();
        let panel_width = 450.0;
        let panel_height = 500.0;
        let title_bar_height = 32.0;
        let tab_bar_height = 36.0;

        // Calculate panel position (centered by default, or use stored position)
        let default_pos = Vec2::new(
            screen_size.x() * 0.5 - panel_width * 0.5,
            screen_size.y() * 0.5 - panel_height * 0.5,
        );
        let panel_pos = self.preferences_panel_pos.unwrap_or(default_pos);

        // Handle dragging
        let title_bounds =
            Rect2D::from_origin_size(panel_pos, Vec2::new(panel_width, title_bar_height));

        // Check if we should start dragging (click on title bar, not on close button)
        let close_btn_area = Rect2D::from_origin_size(
            Vec2::new(panel_pos.x() + panel_width - 30.0, panel_pos.y()),
            Vec2::new(30.0, title_bar_height),
        );
        let can_drag = ui.is_hovered(title_bounds) && !ui.is_hovered(close_btn_area);

        if ui.input.mouse_clicked(mouse_button::LEFT) && can_drag {
            self.dragging_panel = true;
            let mouse_pos = ui.input.mouse_pos;
            self.drag_offset =
                Vec2::new(mouse_pos.x() - panel_pos.x(), mouse_pos.y() - panel_pos.y());
        }

        if self.dragging_panel {
            if ui.input.is_mouse_down(mouse_button::LEFT) {
                let mouse_pos = ui.input.mouse_pos;
                let new_pos = Vec2::new(
                    mouse_pos.x() - self.drag_offset.x(),
                    mouse_pos.y() - self.drag_offset.y(),
                );
                // Clamp to screen bounds and round to integer pixels
                // This ensures child UI elements stay pixel-aligned and don't wobble
                let clamped_x = new_pos
                    .x()
                    .clamp(0.0, (screen_size.x() - panel_width).max(0.0))
                    .round();
                let clamped_y = new_pos
                    .y()
                    .clamp(0.0, (screen_size.y() - panel_height).max(0.0))
                    .round();
                self.preferences_panel_pos = Some(Vec2::new(clamped_x, clamped_y));
            } else {
                self.dragging_panel = false;
            }
        }

        // Use current panel position (may have been updated during drag)
        let panel_pos = self.preferences_panel_pos.unwrap_or(default_pos);
        let panel_bounds =
            Rect2D::from_origin_size(panel_pos, Vec2::new(panel_width, panel_height));

        // Shadow
        let shadow_offset = Vec2::new(6.0, 6.0);
        let shadow_bounds = Rect2D::new(
            panel_bounds.min + shadow_offset,
            panel_bounds.max + shadow_offset,
        );
        ui.draw_rect(shadow_bounds, Color::new(0.0, 0.0, 0.0, 0.6));

        // Panel background
        ui.draw_rect(panel_bounds, theme.panel_bg);
        ui.draw_rect_border(panel_bounds, theme.panel_bg, theme.panel_border, 1.0);

        // Title bar
        let title_bounds =
            Rect2D::from_origin_size(panel_bounds.min, Vec2::new(panel_width, title_bar_height));
        let title_color = if self.dragging_panel || (can_drag && !self.dragging_panel) {
            theme.background_light
        } else {
            theme.panel_header
        };
        ui.draw_rect(title_bounds, title_color);

        // Drag handle indicator (three lines)
        let handle_x = panel_bounds.min.x() + panel_width * 0.5 - 20.0;
        let handle_y = panel_bounds.min.y() + 6.0;
        for i in 0..3 {
            let line_y = handle_y + i as f32 * 3.0;
            ui.draw_line(
                Vec2::new(handle_x, line_y),
                Vec2::new(handle_x + 40.0, line_y),
                theme.text_muted,
                1.0,
            );
        }

        let title_pos = Vec2::new(
            panel_bounds.min.x() + ui.scaled_font_size(FontSize::Medium),
            panel_bounds.min.y() + ui.scaled_font_size(FontSize::Large),
        );
        ui.draw_text(
            "Settings",
            title_pos,
            theme.text_primary,
            ui.scaled_font_size(FontSize::Large),
        );

        // Close button
        let close_size = 24.0;
        let close_bounds = Rect2D::from_origin_size(
            Vec2::new(
                panel_bounds.max.x() - close_size - 6.0,
                panel_bounds.min.y() + 4.0,
            ),
            Vec2::new(close_size, close_size),
        );
        if ui.button("close_prefs", "×", close_bounds) {
            self.show_preferences = false;
            self.preferences_panel_pos = None;
        }

        // === TAB BAR ===
        let tab_bar_bounds = Rect2D::from_origin_size(
            Vec2::new(
                panel_bounds.min.x(),
                panel_bounds.min.y() + title_bar_height,
            ),
            Vec2::new(panel_width, tab_bar_height),
        );
        ui.draw_rect(tab_bar_bounds, theme.background_dark);

        let tab_width = panel_width / PreferencesTab::all().len() as f32;
        for (i, tab) in PreferencesTab::all().iter().enumerate() {
            let tab_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    panel_bounds.min.x() + i as f32 * tab_width,
                    tab_bar_bounds.min.y(),
                ),
                Vec2::new(tab_width, tab_bar_height),
            );
            let is_selected = *tab == self.preferences_tab;

            // Tab background
            let tab_color = if is_selected {
                theme.panel_bg
            } else {
                theme.background_dark
            };
            ui.draw_rect(tab_bounds, tab_color);

            // Tab bottom border (highlight for selected)
            if is_selected {
                ui.draw_line(
                    Vec2::new(tab_bounds.min.x(), tab_bounds.max.y()),
                    Vec2::new(tab_bounds.max.x(), tab_bounds.max.y()),
                    theme.selection,
                    2.0,
                );
            }

            // Tab click
            if ui.button(&format!("tab_{:?}", tab), "", tab_bounds) && !is_selected {
                self.preferences_tab = *tab;
            }

            // Tab icon + text
            let icon = tab.icon();
            let icon_size = ui.scaled_font_size(FontSize::Medium);
            let text = tab.name();
            let text_size = ui.measure_text(text, ui.scaled_font_size(FontSize::Small));
            let total_width = icon_size + 4.0 + text_size.x();
            let start_x = tab_bounds.center().x() - total_width * 0.5;
            let top_y = tab_bounds.center().y() - text_size.y() * 0.5;

            let icon_color = if is_selected {
                theme.text_primary
            } else {
                theme.text_muted
            };
            ui.draw_icon_aligned(
                icon,
                Vec2::new(start_x, top_y),
                icon_size,
                icon_color,
                FontId::DEFAULT,
            );

            let text_color = if is_selected {
                theme.text_primary
            } else {
                theme.text_muted
            };
            ui.draw_text(
                text,
                Vec2::new(start_x + icon_size + 4.0, top_y),
                text_color,
                ui.scaled_font_size(FontSize::Small),
            );
        }

        // === TAB CONTENT ===
        let content_start_y = panel_bounds.min.y() + title_bar_height + tab_bar_height + 8.0;
        let cursor = Vec2::new(panel_bounds.min.x() + 16.0, content_start_y);
        let content_width = panel_width - 32.0;
        let row_height = 28.0;
        let spacing = 8.0;

        match self.preferences_tab {
            PreferencesTab::Appearance => {
                self.build_appearance_tab(ui, &theme, cursor, content_width, row_height, spacing);
            }
            PreferencesTab::Editor => {
                self.build_editor_tab(ui, &theme, cursor, content_width, row_height, spacing);
            }
            PreferencesTab::Keybindings => {
                self.build_keybindings_tab(ui, &theme, cursor, content_width, row_height, spacing);
            }
            PreferencesTab::About => {
                self.build_about_tab(ui, &theme, cursor, content_width);
            }
        }

        // Click outside to close (but not while dragging)
        // Use input.is_hovered directly to bypass popup_bounds and active_id checks
        let mouse_in_panel = ui.input.is_hovered(panel_bounds);
        if !self.dragging_panel && ui.input.mouse_clicked(mouse_button::LEFT) && !mouse_in_panel {
            self.show_preferences = false;
            self.preferences_panel_pos = None;
        }
    }

    fn build_appearance_tab(
        &mut self,
        ui: &mut UiContext,
        theme: &Theme,
        mut cursor: Vec2,
        content_width: f32,
        row_height: f32,
        spacing: f32,
    ) {
        // === THEME SECTION ===
        ui.draw_text(
            "Color Theme",
            cursor,
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Medium),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 20.0);

        // Theme grid (2 columns)
        let col_width = (content_width - spacing) / 2.0;
        let theme_names = [
            ("catppuccin", "Catppuccin"),
            ("nord", "Nord"),
            ("tokyo_night", "Tokyo Night"),
            ("dracula", "Dracula"),
            ("gruvbox", "Gruvbox"),
            ("one_dark", "One Dark"),
            ("material_palenight", "Material Palenight"),
            ("ayu_dark", "Ayu Dark"),
            ("github_dark", "GitHub Dark"),
            ("monokai", "Monokai"),
            ("rose_pine", "Rosé Pine"),
            ("kanagawa", "Kanagawa"),
            ("solarized_dark", "Solarized Dark"),
        ];

        let current_theme_key = self.theme_key();
        for (i, (key, display_name)) in theme_names.iter().enumerate() {
            let col = i % 2;
            let row = i / 2;
            let btn_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    cursor.x() + col as f32 * (col_width + spacing),
                    cursor.y() + row as f32 * (row_height + 4.0),
                ),
                Vec2::new(col_width, row_height),
            );

            let is_selected = *key == current_theme_key;

            if ui.button(&format!("theme_{}", key), "", btn_bounds) {
                self.pending_actions
                    .push(EditorAction::SetTheme(key.to_string()));
            }

            let btn_color = if is_selected {
                theme.selection
            } else {
                theme.button_bg
            };
            ui.draw_rect(btn_bounds, btn_color);

            let text_color = if is_selected {
                theme.button_text
            } else {
                theme.text_primary
            };
            let text_size = ui.measure_text(display_name, ui.scaled_font_size(FontSize::Small));
            let text_pos = Vec2::new(
                btn_bounds.center().x() - text_size.x() * 0.5,
                btn_bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(
                display_name,
                text_pos,
                text_color,
                ui.scaled_font_size(FontSize::Small),
            );
        }

        cursor = Vec2::new(cursor.x(), cursor.y() + 7.0 * (row_height + 4.0) + 16.0);

        // === VIEW OPTIONS ===
        ui.draw_text(
            "View Options",
            cursor,
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Medium),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 24.0);

        // Grid toggle
        let grid_btn_bounds =
            Rect2D::from_origin_size(cursor, Vec2::new(content_width, row_height));
        if ui.toggle_button("pref_grid_toggle", "Show Grid", self.show_grid, grid_btn_bounds, theme.success, theme.button_bg, theme.button_text) {
            self.pending_actions.push(EditorAction::ToggleGrid);
        }
        cursor = Vec2::new(cursor.x(), cursor.y() + row_height + 4.0);

        // Stats toggle
        let stats_btn_bounds =
            Rect2D::from_origin_size(cursor, Vec2::new(content_width, row_height));
        if ui.toggle_button("pref_stats_toggle", "Show Stats Panel", self.show_stats, stats_btn_bounds, theme.success, theme.button_bg, theme.button_text) {
            self.pending_actions.push(EditorAction::ToggleStats);
        }
        cursor = Vec2::new(cursor.x(), cursor.y() + row_height + 16.0);

        // === FONT SCALE ===
        ui.draw_text(
            "Font Scale",
            cursor,
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Medium),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 24.0);

        // Font scale buttons
        let font_scales = [
            (0.75, "75%"),
            (0.9, "90%"),
            (1.0, "100%"),
            (1.1, "110%"),
            (1.25, "125%"),
            (1.5, "150%"),
            (1.75, "175%"),
            (2.0, "200%"),
        ];
        let scale_btn_width = (content_width - 3.0 * spacing) / 4.0;
        for (i, (scale, label)) in font_scales.iter().enumerate() {
            let col = i % 4;
            let row = i / 4;
            let btn_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    cursor.x() + col as f32 * (scale_btn_width + spacing),
                    cursor.y() + row as f32 * (row_height + 4.0),
                ),
                Vec2::new(scale_btn_width, row_height),
            );

            let is_selected = (self.font_scale - scale).abs() < 0.01;

            if ui.button(&format!("font_scale_{}", scale), "", btn_bounds) {
                self.pending_actions
                    .push(EditorAction::SetFontScale(*scale));
            }

            let btn_color = if is_selected {
                theme.selection
            } else {
                theme.button_bg
            };
            ui.draw_rect(btn_bounds, btn_color);

            let text_color = if is_selected {
                theme.button_text
            } else {
                theme.text_primary
            };
            let text_size = ui.measure_text(label, ui.scaled_font_size(FontSize::Small));
            let text_pos = Vec2::new(
                btn_bounds.center().x() - text_size.x() * 0.5,
                btn_bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(
                label,
                text_pos,
                text_color,
                ui.scaled_font_size(FontSize::Small),
            );
        }
    }

    fn build_editor_tab(
        &mut self,
        ui: &mut UiContext,
        theme: &Theme,
        mut cursor: Vec2,
        content_width: f32,
        row_height: f32,
        _spacing: f32,
    ) {
        ui.draw_text(
            "Editor Settings",
            cursor,
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Medium),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 24.0);

        // Snap to grid toggle
        let snap_btn_bounds =
            Rect2D::from_origin_size(cursor, Vec2::new(content_width, row_height));
        if ui.toggle_button("pref_snap_toggle", "Snap to Grid", self.snap_to_grid, snap_btn_bounds, theme.success, theme.button_bg, theme.button_text) {
            self.snap_to_grid = !self.snap_to_grid;
        }
        cursor = Vec2::new(
            cursor.x(),
            cursor.y() + row_height + ui.scaled_font_size(FontSize::Medium),
        );

        // Camera speed
        ui.draw_text(
            "Camera Speed",
            cursor,
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Medium),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 20.0);

        let speed_text = format!("{:.0}", self.camera_speed);
        ui.draw_text(
            &speed_text,
            Vec2::new(cursor.x(), cursor.y()),
            theme.text_primary,
            ui.scaled_font_size(FontSize::Medium),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 20.0);

        // Slider background
        let slider_bounds = Rect2D::from_origin_size(cursor, Vec2::new(content_width, 20.0));
        ui.draw_rect(slider_bounds, theme.button_bg);

        // Slider fill
        let fill_percent = (self.camera_speed - ui.scaled_font_size(FontSize::XSmall)) / 190.0; // 10-200 range
        let fill_width = content_width * fill_percent;
        let fill_bounds = Rect2D::from_origin_size(cursor, Vec2::new(fill_width, 20.0));
        ui.draw_rect(fill_bounds, theme.selection);

        // Slider handle
        if ui.button("camera_speed_slider", "", slider_bounds) {
            // Click to set value
        }

        cursor = Vec2::new(cursor.x(), cursor.y() + 40.0);

        // Grid size
        ui.draw_text(
            &format!("Grid Size: {:.1}", self.grid_size),
            cursor,
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Medium),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 24.0);

        // Grid size buttons
        let sizes = [0.5, 1.0, 2.0, 5.0, ui.scaled_font_size(FontSize::XSmall)];
        let btn_width = (content_width - 4.0 * 8.0) / 5.0;
        for (i, &size) in sizes.iter().enumerate() {
            let btn_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + i as f32 * (btn_width + 8.0), cursor.y()),
                Vec2::new(btn_width, row_height),
            );
            let is_selected = (self.grid_size - size).abs() < 0.01;
            if ui.button(&format!("grid_size_{}", size), "", btn_bounds) {
                self.grid_size = size;
            }
            let btn_color = if is_selected {
                theme.selection
            } else {
                theme.button_bg
            };
            ui.draw_rect(btn_bounds, btn_color);
            let text_color = if is_selected {
                theme.button_text
            } else {
                theme.text_primary
            };
            let text = format!("{:.1}", size);
            let text_size = ui.measure_text(&text, ui.scaled_font_size(FontSize::Small));
            ui.draw_text(
                &text,
                Vec2::new(
                    btn_bounds.center().x() - text_size.x() * 0.5,
                    btn_bounds.center().y() - text_size.y() * 0.5,
                ),
                text_color,
                ui.scaled_font_size(FontSize::Small),
            );
        }
    }

    fn build_keybindings_tab(
        &mut self,
        ui: &mut UiContext,
        theme: &Theme,
        mut cursor: Vec2,
        content_width: f32,
        row_height: f32,
        _spacing: f32,
    ) {
        ui.draw_text(
            "Keyboard Shortcuts",
            cursor,
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Medium),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 24.0);

        let shortcuts = [
            ("Delete", "Delete selected entity"),
            ("↑ / ↓", "Navigate entity list"),
            ("← / →", "Collapse/Expand hierarchy"),
            ("Escape", "Deselect / Close panel"),
            ("T", "Test mesh spawn"),
        ];

        for (key, desc) in shortcuts {
            let row_bounds = Rect2D::from_origin_size(cursor, Vec2::new(content_width, row_height));
            ui.draw_rect(row_bounds, theme.button_bg);

            // Key badge
            let badge_width = 60.0;
            let badge_bounds = Rect2D::from_origin_size(cursor, Vec2::new(badge_width, row_height));
            ui.draw_rect(badge_bounds, theme.background_light);
            let key_size = ui.measure_text(key, ui.scaled_font_size(FontSize::Small));
            ui.draw_text(
                key,
                Vec2::new(
                    badge_bounds.center().x() - key_size.x() * 0.5,
                    badge_bounds.center().y() - key_size.y() * 0.5,
                ),
                theme.text_accent,
                ui.scaled_font_size(FontSize::Small),
            );

            // Description
            ui.draw_text(
                desc,
                Vec2::new(
                    cursor.x() + badge_width + ui.scaled_font_size(FontSize::Medium),
                    cursor.y() + 6.0,
                ),
                theme.text_primary,
                ui.scaled_font_size(FontSize::Medium),
            );

            cursor = Vec2::new(cursor.x(), cursor.y() + row_height + 4.0);
        }

        cursor = Vec2::new(cursor.x(), cursor.y() + 16.0);
        ui.draw_text(
            "(Custom keybindings coming soon)",
            cursor,
            theme.text_muted,
            ui.scaled_font_size(FontSize::Small),
        );
    }

    fn build_about_tab(
        &mut self,
        ui: &mut UiContext,
        theme: &Theme,
        mut cursor: Vec2,
        content_width: f32,
    ) {
        // Center content
        let center_x = cursor.x() + content_width * 0.5;

        // Logo / Title
        let title = "Katla Engine";
        let title_size = ui.measure_text(title, ui.scaled_font_size(FontSize::Huge));
        ui.draw_text(
            title,
            Vec2::new(center_x - title_size.x() * 0.5, cursor.y()),
            theme.text_primary,
            ui.scaled_font_size(FontSize::Huge),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 40.0);

        // Version
        let version = "Version 0.1.0";
        let version_size = ui.measure_text(version, ui.scaled_font_size(FontSize::Large));
        ui.draw_text(
            version,
            Vec2::new(center_x - version_size.x() * 0.5, cursor.y()),
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Large),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 30.0);

        // Description
        let desc = "A Vulkan-based 3D game engine\nwritten in Rust with ECS architecture.";
        for line in desc.split('\n') {
            let line_size = ui.measure_text(line, ui.scaled_font_size(FontSize::Medium));
            ui.draw_text(
                line,
                Vec2::new(center_x - line_size.x() * 0.5, cursor.y()),
                theme.text_muted,
                ui.scaled_font_size(FontSize::Medium),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + 20.0);
        }

        cursor = Vec2::new(cursor.x(), cursor.y() + 30.0);

        // Features
        ui.draw_text(
            "Features",
            Vec2::new(center_x - 30.0, cursor.y()),
            theme.text_secondary,
            ui.scaled_font_size(FontSize::Medium),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + 24.0);

        let features = [
            "Vulkan 1.3 with Dynamic Rendering",
            "ECS Architecture",
            "Skeletal Animation",
            "Particle Systems",
            "Hot Reloadable Shaders",
            "Immediate Mode UI",
        ];

        let check_icon = ForkAwesome::CHECK;
        let font_size = ui.scaled_font_size(FontSize::Medium);
        for feature in features {
            ui.draw_icon_label(
                check_icon,
                feature,
                Vec2::new(center_x - 100.0, cursor.y()),
                font_size,
                font_size,
                theme.success,
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + 18.0);
        }
    }

    /// Build the model preview panel (shown when a model is double-clicked).
    fn build_model_preview_panel(&mut self, ui: &mut UiContext, screen_size: Vec2) {
        let theme = &self.theme;
        let panel_width = self.model_preview.panel_width;
        let panel_height = 400.0;
        let title_bar_height = 28.0;
        let padding = 8.0;

        // Position on right side of screen
        let panel_x = screen_size.x() - panel_width - padding;
        let panel_y = 60.0; // Below toolbar

        let panel_bounds = Rect2D::from_origin_size(
            Vec2::new(panel_x, panel_y),
            Vec2::new(panel_width, panel_height),
        );

        // Draw panel with high z-index (above other panels)
        ui.with_z_index(katla_ui::z_index::POPUP, |ui| {
            // Panel background
            ui.draw_rect(panel_bounds, theme.panel_bg);
            ui.draw_rect_border(panel_bounds, theme.panel_bg, theme.panel_border, 1.0);

            // Title bar
            let title_bounds = Rect2D::from_origin_size(
                Vec2::new(panel_x, panel_y),
                Vec2::new(panel_width, title_bar_height),
            );
            ui.draw_rect(title_bounds, theme.panel_header);

            // Title text
            let model_name = self.model_preview.model_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Model Preview".to_string());
            ui.draw_text(
                &model_name,
                Vec2::new(panel_x + padding, panel_y + 7.0),
                theme.text_primary,
                ui.scaled_font_size(FontSize::Small),
            );

            // Close button (X)
            let close_btn_size = 20.0;
            let close_bounds = Rect2D::from_origin_size(
                Vec2::new(panel_x + panel_width - close_btn_size - 4.0, panel_y + 4.0),
                Vec2::new(close_btn_size, close_btn_size),
            );
            let close_hovered = ui.is_hovered(close_bounds);
            if close_hovered {
                ui.draw_rect(close_bounds, theme.button_hover);
            }
            ui.draw_icon(
                ForkAwesome::TIMES,
                Vec2::new(close_bounds.min.x() + 3.0, close_bounds.min.y() + 2.0),
                14.0,
                if close_hovered { theme.text_primary } else { theme.text_secondary },
            );
            if close_hovered && ui.input.mouse_clicked(mouse_button::LEFT) {
                self.model_preview.close();
            }

            // Content area
            let content_y = panel_y + title_bar_height + padding;
            let content_width = panel_width - padding * 2.0;
            let preview_height = 200.0;

            // === PREVIEW RENDER AREA ===
            let preview_bounds = Rect2D::from_origin_size(
                Vec2::new(panel_x + padding, content_y),
                Vec2::new(content_width, preview_height),
            );

            match &self.model_preview.load_state {
                super::model_preview::LoadState::Idle => {
                    // Show placeholder
                    ui.draw_rect(preview_bounds, theme.background_dark);
                    let text = "No model loaded";
                    let text_size = ui.measure_text(text, ui.scaled_font_size(FontSize::Medium));
                    ui.draw_text(
                        text,
                        Vec2::new(
                            preview_bounds.center().x() - text_size.x() * 0.5,
                            preview_bounds.center().y() - text_size.y() * 0.5,
                        ),
                        theme.text_muted,
                        ui.scaled_font_size(FontSize::Medium),
                    );
                }
                super::model_preview::LoadState::Loading => {
                    // Show loading indicator
                    ui.draw_rect(preview_bounds, theme.background_dark);

                    // Animated spinner
                    let rotation = (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() % 1000) as f32 / 1000.0 * std::f32::consts::TAU;
                    let spinner_chars = ['|', '/', '—', '\\'];
                    let spinner_idx = ((rotation / std::f32::consts::FRAC_PI_2) as usize) % 4;
                    let spinner_char = spinner_chars[spinner_idx];

                    let text = format!("Loading {}", spinner_char);
                    let text_size = ui.measure_text(&text, ui.scaled_font_size(FontSize::Large));
                    ui.draw_text(
                        &text,
                        Vec2::new(
                            preview_bounds.center().x() - text_size.x() * 0.5,
                            preview_bounds.center().y() - text_size.y() * 0.5,
                        ),
                        theme.text_secondary,
                        ui.scaled_font_size(FontSize::Large),
                    );

                    // Progress bar (indeterminate for now)
                    let bar_width = content_width * 0.6;
                    let bar_height = 4.0;
                    let bar_x = preview_bounds.center().x() - bar_width * 0.5;
                    let bar_y = preview_bounds.center().y() + 20.0;
                    let bar_bounds = Rect2D::from_origin_size(
                        Vec2::new(bar_x, bar_y),
                        Vec2::new(bar_width, bar_height),
                    );
                    ui.draw_rect(bar_bounds, theme.background);
                    // Animated progress segment
                    let progress = (rotation / std::f32::consts::TAU) * bar_width;
                    let seg_width = bar_width * 0.3;
                    let seg_x = bar_x + (progress * 0.7);
                    ui.draw_rect(
                        Rect2D::from_origin_size(
                            Vec2::new(seg_x.min(bar_x + bar_width - seg_width), bar_y),
                            Vec2::new(seg_width, bar_height),
                        ),
                        theme.highlight,
                    );
                }
                super::model_preview::LoadState::Loaded => {
                    // Show preview texture (rendered by Vulkan backend)
                    ui.draw_rect(preview_bounds, theme.background_dark);

                    // Draw the preview texture
                    // Use OPAQUE_IMAGE to force alpha = 1.0 (model preview may have 0 alpha)
                    if self.model_preview.model.is_some() {
                        ui.image(
                            self.model_preview.texture_id,
                            preview_bounds,
                            None,  // Use default UVs (0-1)
                            Some(Color::OPAQUE_IMAGE),  // Force opaque output
                        );
                    } else {
                        // Fallback text
                        let text = "Preview Ready";
                        let text_size = ui.measure_text(text, ui.scaled_font_size(FontSize::Medium));
                        ui.draw_text(
                            text,
                            Vec2::new(
                                preview_bounds.center().x() - text_size.x() * 0.5,
                                preview_bounds.center().y() - text_size.y() * 0.5,
                            ),
                            theme.text_secondary,
                            ui.scaled_font_size(FontSize::Medium),
                        );
                    }

                    // Handle orbit camera drag
                    if ui.is_hovered(preview_bounds) {
                        if ui.input.mouse_clicked(mouse_button::LEFT) {
                            self.model_preview.camera.begin_drag(ui.input.mouse_pos);
                        }

                        // Zoom with scroll
                        let scroll = ui.input.scroll_delta.y();
                        if scroll != 0.0 {
                            self.model_preview.camera.zoom(scroll * 0.5);
                        }
                    }

                    if ui.input.is_mouse_down(mouse_button::LEFT) {
                        self.model_preview.camera.update_drag(ui.input.mouse_pos);
                    }

                    if ui.input.mouse_released[mouse_button::LEFT] {
                        self.model_preview.camera.end_drag();
                    }
                }
                super::model_preview::LoadState::Failed(error) => {
                    // Show error message
                    ui.draw_rect(preview_bounds, theme.background_dark);
                    let text = "Failed to load".to_string();
                    let text_size = ui.measure_text(&text, ui.scaled_font_size(FontSize::Medium));
                    ui.draw_text(
                        &text,
                        Vec2::new(
                            preview_bounds.center().x() - text_size.x() * 0.5,
                            preview_bounds.center().y() - 30.0,
                        ),
                        theme.error,
                        ui.scaled_font_size(FontSize::Medium),
                    );

                    // Show error details (truncated)
                    let error_display = if error.len() > 40 {
                        format!("{}...", &error[..40])
                    } else {
                        error.clone()
                    };
                    let error_size = ui.measure_text(&error_display, ui.scaled_font_size(FontSize::XSmall));
                    ui.draw_text(
                        &error_display,
                        Vec2::new(
                            preview_bounds.center().x() - error_size.x() * 0.5,
                            preview_bounds.center().y() + 5.0,
                        ),
                        theme.text_muted,
                        ui.scaled_font_size(FontSize::XSmall),
                    );
                }
            }

            // === STATS SECTION ===
            let stats_y = content_y + preview_height + padding;
            let mut cursor = Vec2::new(panel_x + padding, stats_y);

            ui.draw_text(
                "Model Statistics",
                cursor,
                theme.text_secondary,
                ui.scaled_font_size(FontSize::Small),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + 20.0);

            if let Some(stats) = &self.model_preview.stats {
                let stat_items = [
                    ("Vertices", format!("{}", stats.vertex_count)),
                    ("Triangles", format!("{}", stats.triangle_count)),
                    ("Meshes", stats.mesh_count.to_string()),
                    ("Primitives", stats.primitive_count.to_string()),
                ];

                for (label, value) in stat_items {
                    ui.draw_text(
                        label,
                        cursor,
                        theme.text_muted,
                        ui.scaled_font_size(FontSize::XSmall),
                    );
                    let value_x = panel_x + panel_width - padding - ui.measure_text(&value, ui.scaled_font_size(FontSize::XSmall)).x();
                    ui.draw_text(
                        &value,
                        Vec2::new(value_x, cursor.y()),
                        theme.text_primary,
                        ui.scaled_font_size(FontSize::XSmall),
                    );
                    cursor = Vec2::new(cursor.x(), cursor.y() + 16.0);
                }

                // Animation info
                if stats.has_animations {
                    cursor = Vec2::new(cursor.x(), cursor.y() + 4.0);
                    ui.draw_text(
                        "Animations",
                        cursor,
                        theme.text_secondary,
                        ui.scaled_font_size(FontSize::XSmall),
                    );
                    cursor = Vec2::new(cursor.x(), cursor.y() + 14.0);

                    for anim_name in &stats.animation_names {
                        ui.draw_icon_label(
                            ForkAwesome::VIDEO_CAMERA,
                            anim_name,
                            cursor,
                            ui.scaled_font_size(FontSize::XSmall),
                            ui.scaled_font_size(FontSize::XSmall),
                            theme.text_muted,
                        );
                        cursor = Vec2::new(cursor.x(), cursor.y() + 14.0);
                    }

                    // Play/Pause button (future: animation controls)
                    cursor = Vec2::new(cursor.x(), cursor.y() + 4.0);
                    let btn_width = 80.0;
                    let btn_height = 24.0;
                    let btn_bounds = Rect2D::from_origin_size(
                        cursor,
                        Vec2::new(btn_width, btn_height),
                    );
                    let btn_hovered = ui.is_hovered(btn_bounds);
                    if btn_hovered {
                        ui.draw_rect(btn_bounds, theme.button_hover);
                    } else {
                        ui.draw_rect(btn_bounds, theme.button_bg);
                    }
                    ui.draw_rect_border(btn_bounds, theme.button_bg, theme.border, 1.0);

                    let btn_text = if self.model_preview.animation.playing { "Pause" } else { "Play" };
                    let btn_text_size = ui.measure_text(btn_text, ui.scaled_font_size(FontSize::XSmall));
                    ui.draw_text(
                        btn_text,
                        Vec2::new(
                            btn_bounds.center().x() - btn_text_size.x() * 0.5,
                            btn_bounds.center().y() - btn_text_size.y() * 0.5,
                        ),
                        if btn_hovered { theme.text_primary } else { theme.text_secondary },
                        ui.scaled_font_size(FontSize::XSmall),
                    );

                    if btn_hovered && ui.input.mouse_clicked(mouse_button::LEFT) {
                        self.model_preview.animation.playing = !self.model_preview.animation.playing;
                    }
                }

                // Skinning indicator
                if stats.has_skinning {
                    cursor = Vec2::new(cursor.x(), cursor.y() + 8.0);
                    ui.draw_icon_label(
                        ForkAwesome::USER,
                        "Has Skeleton",
                        cursor,
                        ui.scaled_font_size(FontSize::XSmall),
                        ui.scaled_font_size(FontSize::XSmall),
                        theme.info,
                    );
                }
            }
        });
    }

    /// Render the editor UI and return the draw list.
    pub fn render<'a>(
        &'a mut self,
        ui: &'a mut UiContext,
        screen_size: Vec2,
        scale_factor: f32,
        entities: &'a [EntityInfo],
        fps: f32,
        frame_count: usize,
        loader: &'a mut crate::util::BackgroundLoader,
        thumbnail_texture_ids: &'a std::collections::HashMap<std::path::PathBuf, TextureId>,
    ) -> &'a DrawList {
        // Apply theme to UI style
        self.theme.apply_to_style(&mut ui.style);

        // Apply font scale for accessibility
        ui.set_font_scale(self.font_scale);

        ui.begin(screen_size, scale_factor);
        self.build(ui, entities, fps, frame_count, loader, thumbnail_texture_ids);
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
