use katla_ecs::EntityId;
use katla_math::{Rect2D, Vec2};
use katla_ui::{widgets::Button, widgets::Label, FontSize, Response, UiContext, Widget};

use super::{EditorAction, EntityInfo, FocusedPanel, Theme};

pub struct Inspector<'a> {
    pub bounds: Rect2D,
    pub selected_entity: &'a mut Option<EntityId>,
    pub entities: &'a [EntityInfo],
    pub focused_panel: &'a mut FocusedPanel,
    pub pending_actions: &'a mut Vec<EditorAction>,
    pub theme: &'a Theme,
}

impl<'a> Inspector<'a> {
    pub fn new(
        bounds: Rect2D,
        selected_entity: &'a mut Option<EntityId>,
        entities: &'a [EntityInfo],
        focused_panel: &'a mut FocusedPanel,
        pending_actions: &'a mut Vec<EditorAction>,
        theme: &'a Theme,
    ) -> Self {
        Self {
            bounds,
            selected_entity,
            entities,
            focused_panel,
            pending_actions,
            theme,
        }
    }
}

impl<'a> Widget for Inspector<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        if ui.is_hovered(self.bounds) && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT)
        {
            *self.focused_panel = FocusedPanel::Inspector;
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

        let header_pos = Vec2::new(self.bounds.min.x() + 8.0, header_bounds.center().y() - 7.0);
        ui.draw_text(
            "Inspector",
            header_pos,
            self.theme.text_primary,
            ui.scaled_font_size(FontSize::Medium),
        );

        let selected = self
            .selected_entity
            .and_then(|id| self.entities.iter().find(|e| e.id == id));

        let mut cursor = Vec2::new(
            self.bounds.min.x() + 8.0,
            self.bounds.min.y() + header_height + 8.0,
        );
        let line_height = 20.0;
        let label_width = 60.0;
        let _value_width = self.bounds.width() - label_width - 24.0;

        if let Some(entity) = selected {
            ui.draw_text(
                &entity.name,
                cursor,
                self.theme.text_primary,
                ui.scaled_font_size(FontSize::Large),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 8.0);

            ui.draw_line(
                Vec2::new(self.bounds.min.x() + 8.0, cursor.y()),
                Vec2::new(self.bounds.max.x() - 8.0, cursor.y()),
                self.theme.separator,
                1.0,
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + 8.0);

            ui.draw_text(
                "Transform",
                cursor,
                self.theme.text_accent,
                ui.scaled_font_size(FontSize::Medium),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            let pos_label_bounds =
                Rect2D::from_origin_size(cursor, Vec2::new(label_width, line_height));
            ui.add(Label::new("Position:").bounds(pos_label_bounds));
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
            ui.add(Label::new(&pos_text).bounds(pos_value_bounds));
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            let rot_label_bounds =
                Rect2D::from_origin_size(cursor, Vec2::new(label_width, line_height));
            ui.add(Label::new("Rotation:").bounds(rot_label_bounds));
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
            ui.add(Label::new(&rot_text).bounds(rot_value_bounds));
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            let scale_label_bounds =
                Rect2D::from_origin_size(cursor, Vec2::new(label_width, line_height));
            ui.add(Label::new("Scale:").bounds(scale_label_bounds));
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
            ui.add(Label::new(&scale_text).bounds(scale_value_bounds));
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 8.0);

            ui.draw_line(
                Vec2::new(self.bounds.min.x() + 8.0, cursor.y()),
                Vec2::new(self.bounds.max.x() - 8.0, cursor.y()),
                self.theme.separator,
                1.0,
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + 8.0);

            ui.draw_text(
                "Type",
                cursor,
                self.theme.text_accent,
                ui.scaled_font_size(FontSize::Medium),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            let type_text = format!("  {}", entity.entity_type);
            ui.draw_text(
                &type_text,
                cursor,
                self.theme.text_secondary,
                ui.scaled_font_size(FontSize::Medium),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height + 8.0);

            ui.draw_text(
                "Components",
                cursor,
                self.theme.text_accent,
                ui.scaled_font_size(FontSize::Medium),
            );
            cursor = Vec2::new(cursor.x(), cursor.y() + line_height);

            for component_name in &entity.components {
                let comp_text = format!("  {}", component_name);
                ui.draw_text(
                    &comp_text,
                    cursor,
                    self.theme.text_secondary,
                    ui.scaled_font_size(FontSize::Medium),
                );
                cursor = Vec2::new(cursor.x(), cursor.y() + line_height);
            }

            cursor = Vec2::new(cursor.x(), cursor.y() + 8.0);

            let delete_bounds = Rect2D::from_origin_size(
                Vec2::new(self.bounds.min.x() + 8.0, cursor.y()),
                Vec2::new(self.bounds.width() - 16.0, 28.0),
            );
            if ui
                .add(
                    Button::new("Delete Entity")
                        .bounds(delete_bounds)
                        .id("delete_entity"),
                )
                .clicked
            {
                self.pending_actions
                    .push(EditorAction::DeleteEntity(entity.id));
                *self.selected_entity = None;
            }
        } else {
            let no_selection = "No entity selected";
            let no_sel_size = ui.measure_text(no_selection, ui.scaled_font_size(FontSize::Medium));
            let no_sel_pos = Vec2::new(
                self.bounds.center().x() - no_sel_size.x() * 0.5,
                self.bounds.center().y() - no_sel_size.y() * 0.5,
            );
            ui.draw_text(
                no_selection,
                no_sel_pos,
                self.theme.text_muted,
                ui.scaled_font_size(FontSize::Medium),
            );
        }

        Response::default()
    }
}
