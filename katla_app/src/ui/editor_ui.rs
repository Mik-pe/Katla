//! Game Engine Editor UI
//!
//! A full game engine-style interface with:
//! - Entity Hierarchy panel (left)
//! - Viewport window (center)
//! - Properties/Inspector panel (right)
//! - Toolbar (top)
//! - Status bar (bottom)

use katla_math::{Color, Rect2D, Vec2, Vec3};
use katla_ui::{DrawList, ForkAwesome, UiContext, input::mouse_button};
use katla_ecs::EntityId;
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
        &[SpawnableModel::Fox, SpawnableModel::Cube, SpawnableModel::Sphere, SpawnableModel::Cylinder, SpawnableModel::Plane, SpawnableModel::Torus]
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
    /// Play mode active.
    pub is_playing: bool,
    /// Grid visibility.
    pub show_grid: bool,
    /// Stats panel visible.
    show_stats: bool,
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
    theme: Theme,
}

impl EditorUI {
    pub fn new() -> Self {
        Self {
            visible: true,
            selected_entity: None,
            show_spawn_menu: false,
            spawn_menu_just_opened: false,
            is_playing: false,
            show_grid: true,
            show_stats: true,
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

    /// Get the current theme name.
    pub fn theme_name(&self) -> &'static str {
        self.theme.name
    }

    /// Cycle to the next theme.
    pub fn cycle_theme(&mut self) {
        let all_names = Theme::all_names();
        // Find current theme by name
        let current_idx = all_names.iter().position(|&name| {
            (name == "catppuccin" && self.theme.name == "Catppuccin Mocha") ||
            (name == "nord" && self.theme.name == "Nord") ||
            (name == "tokyo_night" && self.theme.name == "Tokyo Night") ||
            (name == "dracula" && self.theme.name == "Dracula") ||
            (name == "gruvbox" && self.theme.name == "Gruvbox Dark") ||
            (name == "default_dark" && self.theme.name == "Default Dark")
        }).unwrap_or(0);
        let next_idx = (current_idx + 1) % all_names.len();
        if let Some(theme) = Theme::by_name(all_names[next_idx]) {
            self.theme = theme;
        }
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
            current = all_entities.iter()
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
                    self.pending_actions.push(EditorAction::DeleteEntity(entity_id));
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

        // Escape - deselect / close menus
        if ui.input.key_pressed(katla_ui::input::KeyCode::Escape) {
            self.selected_entity = None;
            self.show_spawn_menu = false;
        }

        let padding = 4.0;  // Inner padding for content
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
        let panel_top = toolbar_height + 1.0;  // Just after toolbar border
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
        self.build_status_bar(ui, screen_size, status_bar_height, fps, frame_count, entities.len());

        // === SPAWN MENU POPUP ===
        if self.show_spawn_menu {
            self.build_spawn_menu(ui, screen_size);
        }
    }

    fn build_toolbar(&mut self, ui: &mut UiContext, screen_size: Vec2, height: f32, padding: f32) {
        let theme = &self.theme;
        let toolbar_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, 0.0),
            Vec2::new(screen_size.x(), height),
        );

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
        let play_text = if self.is_playing { "|| Pause" } else { "> Play" };
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

        // Spawn button
        let spawn_bounds = Rect2D::from_origin_size(cursor, Vec2::new(button_width, button_height));
        if ui.button("spawn_btn", "+ Spawn", spawn_bounds) {
            self.show_spawn_menu = !self.show_spawn_menu;
            self.spawn_menu_just_opened = self.show_spawn_menu;
        }
        cursor = Vec2::new(cursor.x() + button_width + padding, cursor.y());

        // Grid toggle
        let grid_text = if self.show_grid { "Grid: ON" } else { "Grid: OFF" };
        let grid_bounds = Rect2D::from_origin_size(cursor, Vec2::new(button_width, button_height));
        if ui.button("grid_btn", grid_text, grid_bounds) {
            self.show_grid = !self.show_grid;
        }
        cursor = Vec2::new(cursor.x() + button_width + padding, cursor.y());

        // Stats toggle
        let stats_text = if self.show_stats { "Stats: ON" } else { "Stats: OFF" };
        let stats_bounds = Rect2D::from_origin_size(cursor, Vec2::new(button_width, button_height));
        if ui.button("stats_btn", stats_text, stats_bounds) {
            self.show_stats = !self.show_stats;
        }

        // Title in center
        let title = "Katla Engine Editor";
        let title_size = ui.measure_text(title, 14.0);
        let title_pos = Vec2::new(
            screen_size.x() * 0.5 - title_size.x() * 0.5,
            height * 0.5 - title_size.y() * 0.5,
        );
        ui.draw_text(title, title_pos, theme.text_muted, 14.0);
    }

    fn build_hierarchy_panel(&mut self, ui: &mut UiContext, entities: &[EntityInfo], bounds: Rect2D) {
        let theme = self.theme.clone(); // Clone to avoid borrow issues
        // Panel background
        ui.draw_rect(bounds, theme.panel_bg);
        ui.draw_rect_border(bounds, theme.panel_bg, theme.panel_border, 1.0);

        // Panel header
        let header_height = 24.0;
        let header_bounds = Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), header_height));
        ui.draw_rect(header_bounds, theme.panel_header);

        // Count visible entities (respecting collapsed state)
        let visible_count = entities.iter()
            .filter(|e| self.is_entity_visible(e, entities))
            .count();

        let header_text = format!("Hierarchy ({} entities)", visible_count);
        let header_pos = Vec2::new(bounds.min.x() + 8.0, header_bounds.center().y() - 7.0);
        ui.draw_text(&header_text, header_pos, theme.text_primary, 12.0);

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
            let is_hovered = ui.input.is_hovered(item_bounds);

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
                let icon = if is_expanded { ForkAwesome::CHEVRON_DOWN } else { ForkAwesome::CHEVRON_RIGHT };
                let triangle_bounds = Rect2D::from_origin_size(
                    Vec2::new(item_x + 2.0, cursor.y()),
                    Vec2::new(16.0, item_height),
                );
                let triangle_hovered = ui.input.is_hovered(triangle_bounds);

                let triangle_color = if triangle_hovered {
                    theme.text_primary
                } else {
                    theme.text_secondary
                };

                let triangle_pos = Vec2::new(item_x + 3.0, cursor.y() + 2.0);
                ui.draw_icon(icon, triangle_pos, 12.0, triangle_color);

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
            ui.draw_text(name_text, name_pos, theme.text_secondary, 12.0);

            // Entity type badge with color coding from theme
            let badge_color = match entity.entity_type.as_str() {
                "Mesh" => theme.entity_mesh,
                "Particle Emitter" => theme.entity_particle,
                "Directional Light" | "Point Light" => theme.entity_light,
                _ => theme.entity_empty,
            };
            let badge_text = &entity.entity_type;
            let badge_size = ui.measure_text(badge_text, 10.0);
            let badge_pos = Vec2::new(item_bounds.max.x() - badge_size.x() - 8.0, cursor.y() + 5.0);
            ui.draw_text(badge_text, badge_pos, badge_color, 10.0);

            // Click to select (but not on triangle)
            let triangle_width = if entity.has_children { 18.0 } else { 0.0 };
            let select_bounds = Rect2D::from_origin_size(
                Vec2::new(item_x + triangle_width, cursor.y()),
                Vec2::new(item_width - triangle_width, item_height),
            );
            let select_hovered = ui.input.is_hovered(select_bounds);

            if ui.input.mouse_clicked(mouse_button::LEFT) && select_hovered {
                self.selected_entity = Some(entity.id);
                self.pending_actions.push(EditorAction::SelectEntity(entity.id));
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
            let empty_size = ui.measure_text(empty_text, 12.0);
            let empty_pos = Vec2::new(
                bounds.center().x() - empty_size.x() * 0.5,
                bounds.center().y() - empty_size.y() * 0.5,
            );
            ui.draw_text(empty_text, empty_pos, theme.text_muted, 12.0);
        }
    }

    fn build_inspector_panel(&mut self, ui: &mut UiContext, entities: &[EntityInfo], bounds: Rect2D) {
        let theme = &self.theme;
        // Panel background
        ui.draw_rect(bounds, theme.panel_bg);
        ui.draw_rect_border(bounds, theme.panel_bg, theme.panel_border, 1.0);

        // Panel header
        let header_height = 24.0;
        let header_bounds = Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), header_height));
        ui.draw_rect(header_bounds, theme.panel_header);

        let header_pos = Vec2::new(bounds.min.x() + 8.0, header_bounds.center().y() - 7.0);
        ui.draw_text("Inspector", header_pos, theme.text_primary, 12.0);

        // Find selected entity
        let selected = self.selected_entity.and_then(|id| {
            entities.iter().find(|e| e.id == id)
        });

        let mut cursor = Vec2::new(bounds.min.x() + 8.0, bounds.min.y() + header_height + 8.0);
        let line_height = 20.0;
        let label_width = 60.0;
        let _value_width = bounds.width() - label_width - 24.0;

        if let Some(entity) = selected {
            // Entity name header
            ui.draw_text(&entity.name, cursor, theme.text_primary, 14.0);
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
            ui.draw_text("Transform", cursor, theme.text_accent, 12.0);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            // Position
            let pos_label_bounds = Rect2D::from_origin_size(cursor, Vec2::new(label_width, line_height));
            ui.label("Position:", pos_label_bounds);
            let pos_value_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + label_width, cursor.y()),
                Vec2::new(_value_width, line_height),
            );
            let pos_text = format!("({:.2}, {:.2}, {:.2})", entity.position.x(), entity.position.y(), entity.position.z());
            ui.label(&pos_text, pos_value_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            // Rotation
            let rot_label_bounds = Rect2D::from_origin_size(cursor, Vec2::new(label_width, line_height));
            ui.label("Rotation:", rot_label_bounds);
            let rot_value_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + label_width, cursor.y()),
                Vec2::new(_value_width, line_height),
            );
            let rot_text = format!("({:.1}, {:.1}, {:.1})", entity.rotation.x(), entity.rotation.y(), entity.rotation.z());
            ui.label(&rot_text, rot_value_bounds);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            // Scale
            let scale_label_bounds = Rect2D::from_origin_size(cursor, Vec2::new(label_width, line_height));
            ui.label("Scale:", scale_label_bounds);
            let scale_value_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + label_width, cursor.y()),
                Vec2::new(_value_width, line_height),
            );
            let scale_text = format!("({:.2}, {:.2}, {:.2})", entity.scale.x(), entity.scale.y(), entity.scale.z());
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
            ui.draw_text("Type", cursor, theme.text_accent, 12.0);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            let type_text = format!("  {}", entity.entity_type);
            ui.draw_text(&type_text, cursor, theme.text_secondary, 12.0);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 8.0);

            // Components list
            ui.draw_text("Components", cursor, theme.text_accent, 12.0);
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            for component_name in &entity.components {
                let comp_text = format!("  {}", component_name);
                ui.draw_text(&comp_text, cursor, theme.text_secondary, 12.0);
                cursor = Vec2::new(cursor.x(), cursor.y() + line_height);
            }

            cursor = Vec2::new(cursor.x(), cursor.y() + 8.0);

            // Delete button
            let delete_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x() + 8.0, cursor.y()),
                Vec2::new(bounds.width() - 16.0, 28.0),
            );
            if ui.button("delete_entity", "Delete Entity", delete_bounds) {
                self.pending_actions.push(EditorAction::DeleteEntity(entity.id));
                self.selected_entity = None;
            }
        } else {
            // No selection
            let no_selection = "No entity selected";
            let no_sel_size = ui.measure_text(no_selection, 12.0);
            let no_sel_pos = Vec2::new(
                bounds.center().x() - no_sel_size.x() * 0.5,
                bounds.center().y() - no_sel_size.y() * 0.5,
            );
            ui.draw_text(no_selection, no_sel_pos, theme.text_muted, 12.0);
        }
    }

    fn build_viewport(&mut self, ui: &mut UiContext, bounds: Rect2D) {
        let theme = &self.theme;
        // Draw the viewport texture (rendered 3D scene)
        // UV x >= 1.0 signals viewport texture sampling in the shader
        // The shader subtracts 1.0 from x, so (1.0, 0.0) to (2.0, 1.0) maps to full texture
        ui.draw_image(
            bounds,
            Vec2::new(1.0, 0.0),  // uv_min: viewport texture starts at (0, 0) after -1.0 offset
            Vec2::new(2.0, 1.0),  // uv_max: viewport texture ends at (1, 1) after -1.0 offset
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
        ui.draw_text(vp_label, label_pos, theme.text_muted, 10.0);
    }

    fn build_status_bar(&mut self, ui: &mut UiContext, screen_size: Vec2, height: f32, fps: f32, frame_count: usize, entity_count: usize) {
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
        ui.draw_text(&fps_text, cursor, fps_color, 11.0);

        // Separator
        cursor = Vec2::new(cursor.x() + 70.0, cursor.y());
        ui.draw_text("|", cursor, theme.text_muted, 11.0);
        cursor = Vec2::new(cursor.x() + 15.0, cursor.y());

        // Frame count
        let frame_text = format!("Frame: {}", frame_count);
        ui.draw_text(&frame_text, cursor, theme.text_secondary, 11.0);

        // Separator
        cursor = Vec2::new(cursor.x() + 100.0, cursor.y());
        ui.draw_text("|", cursor, theme.text_muted, 11.0);
        cursor = Vec2::new(cursor.x() + 15.0, cursor.y());

        // Entity count
        let entity_text = format!("Entities: {}", entity_count);
        ui.draw_text(&entity_text, cursor, theme.text_secondary, 11.0);

        // Play mode indicator on right side
        let mode_text = if self.is_playing { "PLAYING" } else { "EDITING" };
        let mode_color = if self.is_playing {
            theme.success
        } else {
            theme.text_secondary
        };
        let mode_size = ui.measure_text(mode_text, 11.0);
        let mode_pos = Vec2::new(screen_size.x() - mode_size.x() - 8.0, cursor.y());
        ui.draw_text(mode_text, mode_pos, mode_color, 11.0);

        // Theme name display
        let theme_text = format!("Theme: {}", theme.name);
        let theme_size = ui.measure_text(&theme_text, 11.0);
        let theme_pos = Vec2::new(screen_size.x() - mode_size.x() - theme_size.x() - 100.0, cursor.y());
        ui.draw_text(&theme_text, theme_pos, theme.text_muted, 11.0);
    }

    fn build_spawn_menu(&mut self, ui: &mut UiContext, screen_size: Vec2) {
        let theme = &self.theme;
        let menu_width = 300.0;
        let menu_height = 340.0;  // Increased to fit all content
        let menu_bounds = Rect2D::from_origin_size(
            Vec2::new(screen_size.x() * 0.5 - menu_width * 0.5, 80.0),
            Vec2::new(menu_width, menu_height),
        );

        // Menu background with shadow
        let shadow_offset = Vec2::new(4.0, 4.0);
        let shadow_bounds = Rect2D::new(menu_bounds.min + shadow_offset, menu_bounds.max + shadow_offset);
        ui.draw_rect(shadow_bounds, Color::new(0.0, 0.0, 0.0, 0.5));
        ui.draw_rect(menu_bounds, theme.panel_bg);
        ui.draw_rect_border(menu_bounds, theme.panel_bg, theme.panel_border, 1.0);

        // Title
        let title_pos = Vec2::new(menu_bounds.min.x() + 12.0, menu_bounds.min.y() + 12.0);
        ui.draw_text("Spawn Model", title_pos, theme.text_primary, 14.0);

        let mut cursor = Vec2::new(menu_bounds.min.x() + 12.0, menu_bounds.min.y() + 40.0);
        let button_height = 28.0;
        let button_width = menu_width - 24.0;

        // Model selection
        ui.draw_text("Model Type:", cursor, theme.text_secondary, 12.0);
        cursor = Vec2::new(cursor.x(), cursor.y() + 20.0);

        // Model buttons in 2 columns
        let col_width = (button_width - 8.0) / 2.0;
        for (i, model) in SpawnableModel::all().iter().enumerate() {
            let col = i % 2;
            let row = i / 2;
            let btn_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + col as f32 * (col_width + 8.0), cursor.y() + row as f32 * (button_height + 4.0)),
                Vec2::new(col_width, button_height),
            );

            let is_selected = *model == self.selected_spawn;
            if is_selected {
                ui.draw_rect(btn_bounds, theme.selection);
            }
            if ui.selectable(&format!("spawn_{}", model.name()), model.name(), is_selected, btn_bounds) {
                self.selected_spawn = *model;
            }
        }
        cursor = Vec2::new(cursor.x(), cursor.y() + 3.0 * (button_height + 4.0) + 12.0);

        // Position input with +/- buttons
        ui.draw_text("Position (X, Y, Z):", cursor, theme.text_secondary, 12.0);
        cursor = Vec2::new(cursor.x(), cursor.y() + 20.0);

        let axis_labels = ["X", "Y", "Z"];
        let axis_colors = [theme.error, theme.success, theme.info]; // Red, Green, Blue
        let step = 1.0;
        let mini_btn_w = 24.0;
        let value_w = button_width - mini_btn_w * 2.0 - 24.0; // Account for label + two buttons

        for (i, (label, color)) in axis_labels.iter().zip(axis_colors.iter()).enumerate() {
            let row_y = cursor.y() + i as f32 * (button_height + 4.0);

            // Axis label
            ui.draw_text(label, Vec2::new(cursor.x(), row_y + 6.0), *color, 12.0);

            // - button
            let minus_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + 20.0, row_y),
                Vec2::new(mini_btn_w, button_height),
            );
            if ui.button(&format!("pos_{}_minus", i), "-", minus_bounds) {
                self.spawn_pos[i] -= step;
            }

            // Value display
            let value_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + 20.0 + mini_btn_w, row_y),
                Vec2::new(value_w, button_height),
            );
            ui.draw_rect(value_bounds, Color::new(0.15, 0.15, 0.15, 1.0));
            let value_text = format!("{:.1}", self.spawn_pos[i]);
            let text_size = ui.measure_text(&value_text, 12.0);
            let text_pos = Vec2::new(
                value_bounds.center().x() - text_size.x() * 0.5,
                value_bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(&value_text, text_pos, Color::new(0.9, 0.9, 0.9, 1.0), 12.0);

            // + button
            let plus_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor.x() + 20.0 + mini_btn_w + value_w, row_y),
                Vec2::new(mini_btn_w, button_height),
            );
            if ui.button(&format!("pos_{}_plus", i), "+", plus_bounds) {
                self.spawn_pos[i] += step;
            }
        }
        cursor = Vec2::new(cursor.x(), cursor.y() + 3.0 * (button_height + 4.0) + 8.0);

        // Spawn and Cancel buttons
        let spawn_btn_bounds = Rect2D::from_origin_size(cursor, Vec2::new(button_width * 0.48, button_height));
        if ui.button("do_spawn", "Spawn", spawn_btn_bounds) {
            let pos = Vec3::new(self.spawn_pos[0], self.spawn_pos[1], self.spawn_pos[2]);
            self.pending_actions.push(EditorAction::SpawnModel(self.selected_spawn, pos));
            self.show_spawn_menu = false;
        }

        let cancel_btn_bounds = Rect2D::from_origin_size(
            Vec2::new(cursor.x() + button_width * 0.52, cursor.y()),
            Vec2::new(button_width * 0.48, button_height),
        );
        if ui.button("cancel_spawn", "Cancel", cancel_btn_bounds) {
            self.show_spawn_menu = false;
        }

        // Click outside to close (but not on the same frame it opened)
        if !self.spawn_menu_just_opened
            && ui.input.mouse_clicked(mouse_button::LEFT)
            && !ui.input.is_hovered(menu_bounds)
        {
            self.show_spawn_menu = false;
        }

        // Reset the just-opened flag for next frame
        self.spawn_menu_just_opened = false;
    }

    /// Render the editor UI and return the draw list.
    pub fn render<'a>(
        &'a mut self,
        ui: &'a mut UiContext,
        screen_size: Vec2,
        entities: &'a [EntityInfo],
        fps: f32,
        frame_count: usize,
    ) -> &'a DrawList {
        ui.begin(screen_size);
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
