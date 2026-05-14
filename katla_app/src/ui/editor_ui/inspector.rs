use katla_ecs::EntityId;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::{
    FontSize, Response, ScrollArea, ScrollAreaState, UiContext, Widget,
    widgets::{Button, LabeledSlider, Panel, Vec3Slider},
};

use super::{ColorScheme, EditorAction, EntityInfo};

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
}

pub struct Inspector<'a> {
    pub bounds: Rect2D,
    pub selected_entity: &'a mut Option<EntityId>,
    pub entities: &'a [EntityInfo],
    pub pending_actions: &'a mut Vec<EditorAction>,
    pub theme: &'a ColorScheme,
    pub edit: &'a mut InspectorEditState,
    pub scroll_state: &'a mut ScrollAreaState,
}

impl<'a> Inspector<'a> {
    pub fn new(
        bounds: Rect2D,
        selected_entity: &'a mut Option<EntityId>,
        entities: &'a [EntityInfo],
        pending_actions: &'a mut Vec<EditorAction>,
        theme: &'a ColorScheme,
        edit: &'a mut InspectorEditState,
        scroll_state: &'a mut ScrollAreaState,
    ) -> Self {
        Self {
            bounds,
            selected_entity,
            entities,
            pending_actions,
            theme,
            edit,
            scroll_state,
        }
    }
}

const RGB_AXIS_COLORS: [Color; 3] = [
    Color::rgb(1.0, 0.3, 0.3),
    Color::rgb(0.3, 1.0, 0.3),
    Color::rgb(0.3, 0.5, 1.0),
];

fn section_header(ui: &mut UiContext, text: &str, theme: &ColorScheme) {
    let cursor = ui.cursor();
    let y = cursor.y() + 2.0;
    ui.draw_line(
        Vec2::new(cursor.x(), y),
        Vec2::new(cursor.x() + 2000.0, y),
        theme.separator,
        1.0,
    );
    ui.draw_text(
        text,
        Vec2::new(cursor.x(), y + 4.0),
        theme.text_accent,
        ui.scaled_font_size(FontSize::Small),
    );
    let text_h = ui
        .measure_text(text, ui.scaled_font_size(FontSize::Small))
        .y();
    ui.set_cursor(Vec2::new(cursor.x(), y + 4.0 + text_h + 4.0));
}

fn section_gap(ui: &mut UiContext) {
    let cursor = ui.cursor();
    ui.set_cursor(Vec2::new(cursor.x(), cursor.y() + 6.0));
}

fn vec3_row(
    ui: &mut UiContext,
    label: &str,
    values: &mut [f32; 3],
    range: std::ops::RangeInclusive<f32>,
    axis_labels: [&str; 3],
    axis_colors: Option<[Color; 3]>,
    content_width: f32,
) {
    let row_height = ui.style().slider_default_height;
    let bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, row_height * 3.0));
    let mut slider = Vec3Slider::new(label, values, range)
        .bounds(bounds)
        .precision(2);
    if let Some(colors) = axis_colors {
        slider = slider.axis_labels(axis_labels).axis_colors(colors);
    }
    ui.add(slider);
    ui.set_cursor(Vec2::new(
        ui.cursor().x(),
        ui.cursor().y() + row_height * 3.0,
    ));
}

fn scalar_row(
    ui: &mut UiContext,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    content_width: f32,
) {
    let row_height = ui.style().slider_default_height;
    let bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, row_height));
    ui.add(
        LabeledSlider::new(label, value, range)
            .bounds(bounds)
            .label_width(80.0)
            .show_value(true)
            .precision(2),
    );
    ui.set_cursor(Vec2::new(ui.cursor().x(), ui.cursor().y() + row_height));
}

impl<'a> Widget for Inspector<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let content_bounds = {
            let guard = Panel::new("Inspector")
                .bounds(self.bounds)
                .header_height(24.0)
                .show(ui);
            guard.content_bounds()
        };

        let selected = self
            .selected_entity
            .and_then(|id| self.entities.iter().find(|e| e.id == id));

        let padding = ui.style().panel_padding;
        let content_x = content_bounds.min.x() + padding;
        let content_width = content_bounds.width() - 2.0 * padding;

        if let Some(entity) = selected {
            let theme = self.theme;
            let edit = self.edit;
            let pending_actions = self.pending_actions;
            let selected_entity = self.selected_entity;

            *self.scroll_state = ui.scroll_area(
                ScrollArea::new("inspector_scroll").max_height(content_bounds.height()),
                *self.scroll_state,
                content_bounds,
                |ui| {
                    let mut y = content_bounds.min.y() + 2.0;

                    ui.draw_text(
                        &entity.name,
                        Vec2::new(content_x, y),
                        theme.text_primary,
                        ui.scaled_font_size(FontSize::Medium),
                    );
                    let name_h = ui
                        .measure_text(&entity.name, ui.scaled_font_size(FontSize::Medium))
                        .y();
                    y += name_h + 6.0;
                    ui.set_cursor(Vec2::new(content_x, y));

                    section_header(ui, "Transform", theme);
                    vec3_row(
                        ui,
                        "Position",
                        &mut edit.pos,
                        -100.0..=100.0,
                        ["X", "Y", "Z"],
                        None,
                        content_width,
                    );
                    vec3_row(
                        ui,
                        "Rotation",
                        &mut edit.rot,
                        -180.0..=180.0,
                        ["X", "Y", "Z"],
                        None,
                        content_width,
                    );
                    vec3_row(
                        ui,
                        "Scale",
                        &mut edit.scale,
                        0.01..=100.0,
                        ["X", "Y", "Z"],
                        None,
                        content_width,
                    );

                    section_gap(ui);

                    if entity.point_light.is_some() {
                        section_header(ui, "Point Light", theme);
                        vec3_row(
                            ui,
                            "Color",
                            &mut edit.light_color,
                            0.0..=1.0,
                            ["R", "G", "B"],
                            Some(RGB_AXIS_COLORS),
                            content_width,
                        );
                        scalar_row(
                            ui,
                            "Intensity",
                            &mut edit.light_intensity,
                            0.0..=100.0,
                            content_width,
                        );
                        scalar_row(
                            ui,
                            "Range",
                            &mut edit.light_range,
                            0.1..=100.0,
                            content_width,
                        );
                        section_gap(ui);
                    }

                    if entity.particle_emitter.is_some() {
                        section_header(ui, "Particle Emitter", theme);
                        scalar_row(
                            ui,
                            "Emit Rate",
                            &mut edit.emit_rate,
                            0.0..=1000.0,
                            content_width,
                        );
                        scalar_row(
                            ui,
                            "Velocity",
                            &mut edit.velocity,
                            0.0..=50.0,
                            content_width,
                        );
                        scalar_row(
                            ui,
                            "Lifetime",
                            &mut edit.lifetime,
                            0.1..=30.0,
                            content_width,
                        );
                        scalar_row(
                            ui,
                            "Gravity",
                            &mut edit.gravity,
                            -30.0..=30.0,
                            content_width,
                        );
                        scalar_row(
                            ui,
                            "Scale",
                            &mut edit.particle_scale,
                            0.01..=5.0,
                            content_width,
                        );
                        section_gap(ui);
                    }

                    section_header(ui, "Info", theme);
                    ui.property_row("Type", &entity.entity_type);
                    for component_name in &entity.components {
                        ui.property_row("", component_name);
                    }

                    ui.set_cursor(Vec2::new(content_x, ui.cursor().y() + 8.0));

                    let delete_bounds =
                        Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, 24.0));
                    if ui
                        .add(
                            Button::new("Delete Entity")
                                .bounds(delete_bounds)
                                .id("delete_entity"),
                        )
                        .clicked
                    {
                        pending_actions.push(EditorAction::DeleteEntity(entity.id));
                        *selected_entity = None;
                    }

                    ui.cursor().y() - content_bounds.min.y() + 40.0
                },
            );
        } else {
            ui.draw_empty_state(self.bounds, "No entity selected");
        }

        Response::default()
    }
}
