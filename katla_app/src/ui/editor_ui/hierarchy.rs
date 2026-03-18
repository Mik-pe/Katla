use std::collections::HashSet;

use katla_ecs::EntityId;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::{
    input::mouse_button, FontId, FontSize, ForkAwesome, Response, ScrollArea, ScrollAreaState,
    UiContext, Widget,
};

use super::{EditorAction, EntityInfo, FocusedPanel, Theme};

#[derive(Debug, Clone, Default)]
pub struct HierarchyState {
    pub scroll_state: ScrollAreaState,
    pub expanded_entities: HashSet<EntityId>,
    pub context_menu_open: bool,
    pub context_entity: Option<EntityId>,
}

pub fn is_entity_visible(
    entity: &EntityInfo,
    all_entities: &[EntityInfo],
    expanded: &HashSet<EntityId>,
) -> bool {
    let mut current = entity.parent_id;
    while let Some(parent_id) = current {
        if !expanded.contains(&parent_id) {
            return false;
        }
        current = all_entities
            .iter()
            .find(|e| e.id == parent_id)
            .and_then(|e| e.parent_id);
    }
    true
}

pub struct Hierarchy<'a> {
    pub bounds: Rect2D,
    pub state: &'a mut HierarchyState,
    pub selected_entity: &'a mut Option<EntityId>,
    pub entities: &'a [EntityInfo],
    pub focused_panel: &'a mut FocusedPanel,
    pub pending_actions: &'a mut Vec<EditorAction>,
    pub theme: &'a Theme,
}

impl<'a> Hierarchy<'a> {
    pub fn new(
        bounds: Rect2D,
        state: &'a mut HierarchyState,
        selected_entity: &'a mut Option<EntityId>,
        entities: &'a [EntityInfo],
        focused_panel: &'a mut FocusedPanel,
        pending_actions: &'a mut Vec<EditorAction>,
        theme: &'a Theme,
    ) -> Self {
        Self {
            bounds,
            state,
            selected_entity,
            entities,
            focused_panel,
            pending_actions,
            theme,
        }
    }
}

impl<'a> Widget for Hierarchy<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        if ui.is_hovered(self.bounds) && ui.mouse_clicked(mouse_button::LEFT) {
            *self.focused_panel = FocusedPanel::Hierarchy;
        }

        ui.draw_rect(self.bounds, self.theme.panel_bg);
        ui.draw_rect_border(
            self.bounds,
            self.theme.panel_bg,
            self.theme.panel_border,
            1.0,
        );

        let header_height = 24.0;
        let header_bounds = Rect2D::from_origin_size(
            self.bounds.min,
            Vec2::new(self.bounds.width(), header_height),
        );
        ui.draw_rect(header_bounds, self.theme.panel_header);

        let visible_count = self
            .entities
            .iter()
            .filter(|e| is_entity_visible(e, self.entities, &self.state.expanded_entities))
            .count();

        let header_text = format!("Hierarchy ({} entities)", visible_count);
        let header_pos = Vec2::new(self.bounds.min.x() + 8.0, header_bounds.center().y() - 7.0);
        ui.draw_text(
            &header_text,
            header_pos,
            self.theme.text_primary,
            ui.scaled_font_size(FontSize::Medium),
        );

        let content_bounds = Rect2D::from_origin_size(
            Vec2::new(self.bounds.min.x(), self.bounds.min.y() + header_height),
            Vec2::new(self.bounds.width(), self.bounds.height() - header_height),
        );

        let expanded_entities = self.state.expanded_entities.clone();
        let mut toggle_entity: Option<EntityId> = None;
        let mut clicked_entity: Option<EntityId> = None;
        let mut right_clicked_entity: Option<EntityId> = None;

        self.state.scroll_state = ui.scroll_area(
            ScrollArea::new("hierarchy_scroll").max_height(content_bounds.height()),
            self.state.scroll_state,
            content_bounds,
            |ui| {
                let scroll_offset = ui.scroll_offset();

                let mut cursor = Vec2::new(
                    self.bounds.min.x(),
                    self.bounds.min.y() + header_height + 4.0 - scroll_offset,
                );
                let item_height = 22.0;
                let indent_per_level = 16.0;

                for entity in self.entities {
                    if !is_entity_visible(entity, self.entities, &expanded_entities) {
                        continue;
                    }

                    let indent = entity.depth as f32 * indent_per_level;
                    let item_x = self.bounds.min.x() + indent;
                    let item_width = self.bounds.width() - indent;

                    let item_bounds = Rect2D::from_origin_size(
                        Vec2::new(item_x, cursor.y()),
                        Vec2::new(item_width, item_height),
                    );

                    let is_selected = Some(entity.id) == *self.selected_entity;
                    let is_hovered = ui.is_hovered(item_bounds);

                    let bg_color = if is_selected {
                        self.theme.selection
                    } else if is_hovered {
                        self.theme.selection_hover
                    } else {
                        Color::TRANSPARENT
                    };

                    if bg_color != Color::TRANSPARENT {
                        ui.draw_rect(item_bounds, bg_color);
                    }

                    if entity.depth > 0 {
                        let line_x = item_x - 8.0;
                        ui.draw_line(
                            Vec2::new(line_x, cursor.y()),
                            Vec2::new(line_x, cursor.y() + item_height),
                            self.theme.separator,
                            1.0,
                        );
                    }

                    let text_x = if entity.has_children {
                        let is_expanded = expanded_entities.contains(&entity.id);
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
                            self.theme.text_primary
                        } else {
                            self.theme.text_secondary
                        };

                        let triangle_pos = Vec2::new(item_x + 3.0, cursor.y() + 3.0);
                        ui.draw_icon_aligned(
                            icon,
                            triangle_pos,
                            ui.scaled_font_size(FontSize::Medium),
                            triangle_color,
                            FontId::DEFAULT,
                        );

                        if ui.mouse_clicked(mouse_button::LEFT) && triangle_hovered {
                            toggle_entity = Some(entity.id);
                        }

                        item_x + 18.0
                    } else {
                        let dot_pos = Vec2::new(item_x + 6.0, cursor.y() + 8.0);
                        ui.draw_rect(
                            Rect2D::from_origin_size(dot_pos, Vec2::new(4.0, 4.0)),
                            self.theme.text_muted,
                        );
                        item_x + 18.0
                    };

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
                        "Mesh" => self.theme.entity_mesh,
                        "Particle Emitter" => self.theme.entity_particle,
                        "Directional Light" | "Point Light" => self.theme.entity_light,
                        _ => self.theme.text_secondary,
                    };

                    ui.draw_icon_aligned(
                        entity_icon,
                        Vec2::new(text_x, cursor.y() + 3.0),
                        ui.scaled_font_size(FontSize::Medium),
                        entity_icon_color,
                        FontId::DEFAULT,
                    );

                    let name_text = &entity.name;
                    let name_pos = Vec2::new(text_x + 16.0, cursor.y() + 3.0);
                    ui.draw_text(
                        name_text,
                        name_pos,
                        self.theme.text_secondary,
                        ui.scaled_font_size(FontSize::Medium),
                    );

                    let badge_color = match entity.entity_type.as_str() {
                        "Mesh" => self.theme.entity_mesh,
                        "Particle Emitter" => self.theme.entity_particle,
                        "Directional Light" | "Point Light" => self.theme.entity_light,
                        _ => self.theme.entity_empty,
                    };
                    let badge_text = &entity.entity_type;
                    let badge_size =
                        ui.measure_text(badge_text, ui.scaled_font_size(FontSize::XSmall));
                    let badge_pos =
                        Vec2::new(item_bounds.max.x() - badge_size.x() - 8.0, cursor.y() + 5.0);
                    ui.draw_text(
                        badge_text,
                        badge_pos,
                        badge_color,
                        ui.scaled_font_size(FontSize::XSmall),
                    );

                    let triangle_width = if entity.has_children { 18.0 } else { 0.0 };
                    let select_bounds = Rect2D::from_origin_size(
                        Vec2::new(item_x + triangle_width, cursor.y()),
                        Vec2::new(item_width - triangle_width, item_height),
                    );
                    let select_hovered = ui.is_hovered(select_bounds);

                    if ui.mouse_clicked(mouse_button::LEFT)
                        && select_hovered
                        && !ui.has_open_popup()
                    {
                        clicked_entity = Some(entity.id);
                    }

                    if ui.mouse_clicked(mouse_button::RIGHT) && is_hovered && !ui.has_open_popup() {
                        right_clicked_entity = Some(entity.id);
                    }

                    cursor = Vec2::new(cursor.x(), cursor.y() + item_height);
                }

                visible_count as f32 * item_height + 8.0
            },
        );

        if let Some(entity_id) = toggle_entity {
            if self.state.expanded_entities.contains(&entity_id) {
                self.state.expanded_entities.remove(&entity_id);
            } else {
                self.state.expanded_entities.insert(entity_id);
            }
        }
        if let Some(entity_id) = clicked_entity {
            *self.selected_entity = Some(entity_id);
            self.pending_actions
                .push(EditorAction::SelectEntity(entity_id));
        }
        if let Some(entity_id) = right_clicked_entity {
            *self.selected_entity = Some(entity_id);
            self.state.context_entity = Some(entity_id);
            self.state.context_menu_open = true;
        }

        if self.entities.is_empty() {
            let empty_text = "No entities in scene";
            let empty_size = ui.measure_text(empty_text, ui.scaled_font_size(FontSize::Medium));
            let empty_pos = Vec2::new(
                self.bounds.center().x() - empty_size.x() * 0.5,
                self.bounds.center().y() - empty_size.y() * 0.5,
            );
            ui.draw_text(
                empty_text,
                empty_pos,
                self.theme.text_muted,
                ui.scaled_font_size(FontSize::Medium),
            );
        }

        let mut clicked_action: Option<&str> = None;
        ui.context_menu(
            "hierarchy_context",
            &mut self.state.context_menu_open,
            |ui, open| {
                if ui.menu_item_clicked_with_icon_and_shortcut(
                    "Duplicate",
                    ForkAwesome::COPY,
                    true,
                    "Ctrl+D",
                ) {
                    clicked_action = Some("Duplicate");
                    *open = false;
                    return;
                }
                if ui.menu_item_clicked_with_icon_and_shortcut(
                    "Rename",
                    ForkAwesome::PENCIL,
                    true,
                    "F2",
                ) {
                    clicked_action = Some("Rename");
                    *open = false;
                    return;
                }
                ui.menu_separator();
                if ui.menu_item_clicked_with_icon_and_shortcut(
                    "Delete",
                    ForkAwesome::TRASH,
                    true,
                    "Del",
                ) {
                    clicked_action = Some("Delete");
                    *open = false;
                }
            },
        );

        if !self.state.context_menu_open {
            self.state.context_entity = None;
        }

        if let Some(action) = clicked_action {
            match action {
                "Duplicate" => {
                    if let Some(entity_id) = self.state.context_entity {
                        self.pending_actions
                            .push(EditorAction::DuplicateEntity(entity_id));
                    }
                }
                "Rename" => {}
                "Delete" => {
                    if let Some(entity_id) = self.state.context_entity {
                        self.pending_actions
                            .push(EditorAction::DeleteEntity(entity_id));
                    }
                }
                _ => {}
            }
        }

        Response::default()
    }
}
