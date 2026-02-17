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
use katla_ui::{input::mouse_button, DrawList, FontId, FontSize, ForkAwesome, UiContext};
use std::collections::HashSet;

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
    /// Delete an entity.
    DeleteEntity(EntityId),
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
    /// Current color theme.
    pub theme: Theme,
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
            theme: Theme::catppuccin(), // Default to Catppuccin because it's sick
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
        let left_panel_width = 220.0;
        let right_panel_width = 280.0;

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

        // Panel Y range (between toolbar and status bar, no gaps)
        let panel_top = toolbar_height + 1.0; // Just after toolbar border
        let panel_bottom = screen_size.y() - status_bar_height;
        let panel_height = panel_bottom - panel_top;

        // === LEFT PANEL: Entity Hierarchy ===
        let left_panel_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_top),
            Vec2::new(left_panel_width, panel_height),
        );
        self.build_hierarchy_panel(ui, entities, left_panel_bounds);

        // Left panel right border
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(left_panel_width, panel_top),
                Vec2::new(1.0, panel_height),
            ),
            Color::new(0.3, 0.3, 0.3, 1.0),
        );

        // === RIGHT PANEL: Properties Inspector ===
        let right_panel_x = screen_size.x() - right_panel_width;
        let right_panel_bounds = Rect2D::from_origin_size(
            Vec2::new(right_panel_x, panel_top),
            Vec2::new(right_panel_width, panel_height),
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
            Vec2::new(left_panel_width + 1.0, panel_top),
            Vec2::new(right_panel_x - 1.0, panel_bottom),
        );

        // Track viewport size for render target sizing
        self.last_viewport_size = (
            viewport_bounds.width().max(1.0) as u32,
            viewport_bounds.height().max(1.0) as u32,
        );

        self.build_viewport(ui, viewport_bounds);

        // Status bar top border (fills gap)
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(0.0, panel_bottom),
                Vec2::new(screen_size.x(), 1.0),
            ),
            Color::new(0.3, 0.3, 0.3, 1.0),
        );

        // === STATUS BAR (bottom) ===
        self.build_status_bar(
            ui,
            screen_size,
            status_bar_height,
            fps,
            frame_count,
            entities.len(),
        );

        // === PREFERENCES PANEL (overlay) ===
        if self.show_preferences {
            self.build_preferences_panel(ui, screen_size);
        }
    }

    fn build_toolbar(&mut self, ui: &mut UiContext, screen_size: Vec2, height: f32, padding: f32) {
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

        let button_width = 80.0;
        let button_height = height - padding * 2.0;
        let mut cursor = Vec2::new(padding, padding);

        // Play/Pause button
        let play_text = if self.is_playing {
            "|| Pause"
        } else {
            "> Play"
        };
        let play_bounds = Rect2D::from_origin_size(cursor, Vec2::new(button_width, button_height));
        let play_color = if self.is_playing {
            theme.success
        } else {
            theme.button_bg
        };
        ui.draw_rect(play_bounds, play_color);
        if ui.button("play_btn", play_text, play_bounds) {
            self.is_playing = !self.is_playing;
            self.pending_actions.push(EditorAction::TogglePlay);
        }
        cursor = Vec2::new(cursor.x() + button_width + padding, cursor.y());

        // Spawn dropdown
        let spawn_bounds = Rect2D::from_origin_size(cursor, Vec2::new(button_width, button_height));
        if ui.begin_dropdown("spawn_dropdown", "Spawn", spawn_bounds) {
            for model in SpawnableModel::all() {
                if ui.menu_item(
                    &format!("spawn_{}", model.name()),
                    model.name(),
                    Rect2D::from_origin_size(
                        Vec2::new(
                            spawn_bounds.min.x(),
                            spawn_bounds.max.y() + (*model as usize as f32) * 24.0,
                        ),
                        Vec2::new(spawn_bounds.width(), 24.0),
                    ),
                ) {
                    self.pending_actions
                        .push(EditorAction::SpawnModel(*model, Vec3::new(0.0, 0.0, 0.0)));
                    ui.close_current_popup();
                }
            }
            ui.end_dropdown();
        }
        cursor = Vec2::new(cursor.x() + button_width + padding, cursor.y());

        // Grid toggle
        let grid_text = if self.show_grid {
            "Grid: ON"
        } else {
            "Grid: OFF"
        };
        let grid_bounds = Rect2D::from_origin_size(cursor, Vec2::new(button_width, button_height));
        if ui.button("grid_btn", grid_text, grid_bounds) {
            self.show_grid = !self.show_grid;
        }
        cursor = Vec2::new(cursor.x() + button_width + padding, cursor.y());

        // Stats toggle
        let stats_text = if self.show_stats {
            "Stats: ON"
        } else {
            "Stats: OFF"
        };
        let stats_bounds = Rect2D::from_origin_size(cursor, Vec2::new(button_width, button_height));
        if ui.button("stats_btn", stats_text, stats_bounds) {
            self.show_stats = !self.show_stats;
        }

        // Settings button on the right side
        let settings_text = " Settings";
        let settings_text_size =
            ui.measure_text(settings_text, ui.scaled_font_size(FontSize::Medium));
        let icon_size = ui.scaled_font_size(FontSize::Large);
        let icon_padding = 4.0;
        let settings_total_width = icon_size + icon_padding + settings_text_size.x();
        let settings_bounds = Rect2D::from_origin_size(
            Vec2::new(
                screen_size.x() - settings_total_width - padding * 3.0,
                padding,
            ),
            Vec2::new(settings_total_width + padding * 2.0, button_height),
        );
        let settings_color = if self.show_preferences {
            theme.selection
        } else {
            theme.button_bg
        };
        ui.draw_rect(settings_bounds, settings_color);
        if ui.button("settings_btn", "", settings_bounds) {
            self.show_preferences = !self.show_preferences;
        }
        // Draw icon and text aligned
        let top_y = settings_bounds.center().y() - settings_text_size.y() * 0.5;
        let icon_pos = Vec2::new(settings_bounds.min.x() + padding, top_y);
        ui.draw_icon_aligned(
            ForkAwesome::COG,
            icon_pos,
            icon_size,
            theme.text_primary,
            FontId::DEFAULT,
        );
        let text_pos = Vec2::new(icon_pos.x() + icon_size + icon_padding, top_y);
        ui.draw_text(
            settings_text,
            text_pos,
            theme.text_primary,
            ui.scaled_font_size(FontSize::Medium),
        );
        ui.draw_icon(ForkAwesome::COG, icon_pos, icon_size, theme.text_primary);
        let text_pos = Vec2::new(
            icon_pos.x() + icon_size + icon_padding,
            settings_bounds.center().y() - settings_text_size.y() * 0.5,
        );
        ui.draw_text(
            settings_text,
            text_pos,
            theme.text_primary,
            ui.scaled_font_size(FontSize::Medium),
        );

        // Title in center
        let title = "Katla Engine Editor";
        let title_size = ui.measure_text(title, ui.scaled_font_size(FontSize::Large));
        let title_pos = Vec2::new(
            screen_size.x() * 0.5 - title_size.x() * 0.5,
            height * 0.5 - title_size.y() * 0.5,
        );
        ui.draw_text(
            title,
            title_pos,
            theme.text_muted,
            ui.scaled_font_size(FontSize::Large),
        );
    }

    fn build_hierarchy_panel(
        &mut self,
        ui: &mut UiContext,
        entities: &[EntityInfo],
        bounds: Rect2D,
    ) {
        let theme = self.theme.clone(); // Clone to avoid borrow issues
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

        // Entity list
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

            // Entity name
            let name_text = &entity.name;
            let name_pos = Vec2::new(text_x, cursor.y() + 3.0);
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

            // Click to select (but not on triangle)
            let triangle_width = if entity.has_children { 18.0 } else { 0.0 };
            let select_bounds = Rect2D::from_origin_size(
                Vec2::new(item_x + triangle_width, cursor.y()),
                Vec2::new(item_width - triangle_width, item_height),
            );
            let select_hovered = ui.is_hovered(select_bounds);

            if ui.input.mouse_clicked(mouse_button::LEFT) && select_hovered {
                self.selected_entity = Some(entity.id);
                self.pending_actions
                    .push(EditorAction::SelectEntity(entity.id));
            }

            // Right-click for context menu
            if ui.input.mouse_clicked(mouse_button::RIGHT) && is_hovered {
                self.selected_entity = Some(entity.id);
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
    }

    fn build_inspector_panel(
        &mut self,
        ui: &mut UiContext,
        entities: &[EntityInfo],
        bounds: Rect2D,
    ) {
        let theme = &self.theme;
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
        // Draw the viewport texture (rendered 3D scene)
        // UV x >= 1.0 signals viewport texture sampling in the shader
        // The shader subtracts 1.0 from x, so (1.0, 0.0) to (2.0, 1.0) maps to full texture
        ui.draw_image(
            bounds,
            Vec2::new(1.0, 0.0), // uv_min: viewport texture starts at (0, 0) after -1.0 offset
            Vec2::new(2.0, 1.0), // uv_max: viewport texture ends at (1, 1) after -1.0 offset
            Color::WHITE,
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
                // Clamp to screen bounds
                let clamped_x = new_pos
                    .x()
                    .clamp(0.0, (screen_size.x() - panel_width).max(0.0));
                let clamped_y = new_pos
                    .y()
                    .clamp(0.0, (screen_size.y() - panel_height).max(0.0));
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
        let mut cursor = Vec2::new(panel_bounds.min.x() + 16.0, content_start_y);
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
        if ui.button("pref_grid_toggle", "", grid_btn_bounds) {
            self.pending_actions.push(EditorAction::ToggleGrid);
        }
        let grid_color = if self.show_grid {
            theme.success
        } else {
            theme.button_bg
        };
        ui.draw_rect(grid_btn_bounds, grid_color);
        let grid_text = if self.show_grid {
            "✓ Show Grid"
        } else {
            "  Show Grid"
        };
        let grid_text_color = if self.show_grid {
            theme.button_text
        } else {
            theme.text_primary
        };
        ui.draw_text(
            grid_text,
            Vec2::new(
                grid_btn_bounds.min.x() + ui.scaled_font_size(FontSize::Medium),
                grid_btn_bounds.min.y() + 6.0,
            ),
            grid_text_color,
            ui.scaled_font_size(FontSize::Medium),
        );
        cursor = Vec2::new(cursor.x(), cursor.y() + row_height + 4.0);

        // Stats toggle
        let stats_btn_bounds =
            Rect2D::from_origin_size(cursor, Vec2::new(content_width, row_height));
        if ui.button("pref_stats_toggle", "", stats_btn_bounds) {
            self.pending_actions.push(EditorAction::ToggleStats);
        }
        let stats_color = if self.show_stats {
            theme.success
        } else {
            theme.button_bg
        };
        ui.draw_rect(stats_btn_bounds, stats_color);
        let stats_text = if self.show_stats {
            "✓ Show Stats Panel"
        } else {
            "  Show Stats Panel"
        };
        let stats_text_color = if self.show_stats {
            theme.button_text
        } else {
            theme.text_primary
        };
        ui.draw_text(
            stats_text,
            Vec2::new(
                stats_btn_bounds.min.x() + ui.scaled_font_size(FontSize::Medium),
                stats_btn_bounds.min.y() + 6.0,
            ),
            stats_text_color,
            ui.scaled_font_size(FontSize::Medium),
        );
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
        if ui.button("pref_snap_toggle", "", snap_btn_bounds) {
            self.snap_to_grid = !self.snap_to_grid;
        }
        let snap_color = if self.snap_to_grid {
            theme.success
        } else {
            theme.button_bg
        };
        ui.draw_rect(snap_btn_bounds, snap_color);
        let snap_text = if self.snap_to_grid {
            "✓ Snap to Grid"
        } else {
            "  Snap to Grid"
        };
        let snap_text_color = if self.snap_to_grid {
            theme.button_text
        } else {
            theme.text_primary
        };
        ui.draw_text(
            snap_text,
            Vec2::new(
                snap_btn_bounds.min.x() + ui.scaled_font_size(FontSize::Medium),
                snap_btn_bounds.min.y() + 6.0,
            ),
            snap_text_color,
            ui.scaled_font_size(FontSize::Medium),
        );
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

        for feature in features {
            let check_size = ui.measure_text("✓", ui.scaled_font_size(FontSize::Medium));
            ui.draw_text(
                "✓",
                Vec2::new(center_x - 100.0, cursor.y()),
                theme.success,
                ui.scaled_font_size(FontSize::Medium),
            );
            ui.draw_text(
                feature,
                Vec2::new(center_x - 80.0, cursor.y()),
                theme.text_primary,
                ui.scaled_font_size(FontSize::Medium),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + 18.0);
        }
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
    ) -> &'a DrawList {
        // Apply theme to UI style
        self.theme.apply_to_style(&mut ui.style);

        // Apply font scale for accessibility
        ui.set_font_scale(self.font_scale);

        ui.begin(screen_size, scale_factor);
        self.build(ui, entities, fps, frame_count);
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
