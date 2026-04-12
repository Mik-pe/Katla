use std::collections::HashSet;

use katla_ecs::EntityId;
use katla_math::{Rect2D, Vec2};
use katla_ui::widgets::{Panel, RowInfo, TreeItem, TreeState, TreeView};
use katla_ui::{FontId, FontSize, ForkAwesome, Response, ScrollAreaState, UiContext, Widget};

use super::{ColorScheme, EditorAction, EntityInfo};

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
    pub pending_actions: &'a mut Vec<EditorAction>,
    pub theme: &'a ColorScheme,
}

impl<'a> Hierarchy<'a> {
    pub fn new(
        bounds: Rect2D,
        state: &'a mut HierarchyState,
        selected_entity: &'a mut Option<EntityId>,
        entities: &'a [EntityInfo],
        pending_actions: &'a mut Vec<EditorAction>,
        theme: &'a ColorScheme,
    ) -> Self {
        Self {
            bounds,
            state,
            selected_entity,
            entities,
            pending_actions,
            theme,
        }
    }
}

impl<'a> Widget for Hierarchy<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let visible_count = self
            .entities
            .iter()
            .filter(|e| is_entity_visible(e, self.entities, &self.state.expanded_entities))
            .count();

        let header_text = format!("Hierarchy ({} entities)", visible_count);
        let content_bounds = {
            let guard = Panel::new(&header_text)
                .bounds(self.bounds)
                .header_height(24.0)
                .show(ui);
            guard.content_bounds()
        };

        let entities = self.entities;
        let theme = self.theme;
        let bounds_width = self.bounds.width();

        let items: Vec<TreeItem> = entities
            .iter()
            .map(|e| TreeItem {
                id: e.id.id(),
                label: e.name.clone(),
                depth: e.depth,
                has_children: e.has_children,
            })
            .collect();

        let mut tree_state = TreeState {
            expanded: self
                .state
                .expanded_entities
                .iter()
                .map(|id| id.id())
                .collect(),
            selected: self.selected_entity.map(|id| id.id()),
            scroll_offset: self.state.scroll_state.scroll_offset,
        };

        let response = if !items.is_empty() {
            ui.add(
                TreeView::new("hierarchy_tree", &mut tree_state)
                    .bounds(content_bounds)
                    .data(items)
                    .row_height(22.0)
                    .indent_per_level(16.0)
                    .render_item(move |ui: &mut UiContext, item: &TreeItem, info: &RowInfo| {
                        let entity = entities
                            .iter()
                            .find(|e| e.id.id() == item.id)
                            .expect("TreeItem id must correspond to an EntityInfo");

                        if entity.depth > 0 {
                            let line_x = info.bounds.min.x() - 8.0;
                            ui.draw_line(
                                Vec2::new(line_x, info.bounds.min.y()),
                                Vec2::new(line_x, info.bounds.max.y()),
                                theme.separator,
                                1.0,
                            );
                        }

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

                        ui.draw_icon_aligned(
                            entity_icon,
                            Vec2::new(info.content_x, info.bounds.min.y() + 3.0),
                            ui.scaled_font_size(FontSize::Medium),
                            entity_icon_color,
                            FontId::DEFAULT,
                        );

                        let badge_color = match entity.entity_type.as_str() {
                            "Mesh" => theme.entity_mesh,
                            "Particle Emitter" => theme.entity_particle,
                            "Directional Light" | "Point Light" => theme.entity_light,
                            _ => theme.entity_empty,
                        };
                        let badge_text = &entity.entity_type;
                        let badge_size =
                            ui.measure_text(badge_text, ui.scaled_font_size(FontSize::XSmall));
                        let badge_x = info.bounds.min.x() + bounds_width - badge_size.x() - 8.0;

                        let name_x = info.content_x + 16.0;
                        let name_font_size = ui.scaled_font_size(FontSize::Medium);
                        let max_name_width = (badge_x - name_x - 8.0).max(0.0);
                        let display_name =
                            ui.truncate_text(&entity.name, max_name_width, name_font_size);

                        let name_pos = Vec2::new(name_x, info.bounds.min.y() + 3.0);
                        ui.draw_text(
                            &display_name,
                            name_pos,
                            theme.text_secondary,
                            name_font_size,
                        );

                        let badge_pos = Vec2::new(badge_x, info.bounds.min.y() + 5.0);
                        ui.draw_text(
                            badge_text,
                            badge_pos,
                            badge_color,
                            ui.scaled_font_size(FontSize::XSmall),
                        );
                    }),
            )
        } else {
            ui.draw_empty_state(self.bounds, "No entities in scene");
            Response::default()
        };

        self.state.expanded_entities = tree_state
            .expanded
            .iter()
            .map(|&id| EntityId::from_raw(id))
            .collect();
        self.state.scroll_state.scroll_offset = tree_state.scroll_offset;

        if let Some(selected_u64) = tree_state.selected {
            let new_selected = EntityId::from_raw(selected_u64);
            if *self.selected_entity != Some(new_selected) {
                *self.selected_entity = Some(new_selected);
                self.pending_actions
                    .push(EditorAction::SelectEntity(new_selected));
            }
        }

        if response.right_clicked {
            if let Some(selected_u64) = tree_state.selected {
                let entity_id = EntityId::from_raw(selected_u64);
                *self.selected_entity = Some(entity_id);
                self.state.context_entity = Some(entity_id);
                self.state.context_menu_open = true;
            }
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
