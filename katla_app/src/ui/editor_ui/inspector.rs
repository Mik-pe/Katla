use katla_ecs::EntityId;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::{
    FontSize, Response, UiContext, Widget, mouse_button,
    widgets::{Button, Slider},
};

use super::{EditorAction, EntityInfo, FocusedPanel, Theme};

const ROW_HEIGHT: f32 = 18.0;

/// Mutable inspector editing state for all editable properties of the selected entity.
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
    pub focused_panel: &'a mut FocusedPanel,
    pub pending_actions: &'a mut Vec<EditorAction>,
    pub theme: &'a Theme,
    pub edit: &'a mut InspectorEditState,
}

impl<'a> Inspector<'a> {
    pub fn new(
        bounds: Rect2D,
        selected_entity: &'a mut Option<EntityId>,
        entities: &'a [EntityInfo],
        focused_panel: &'a mut FocusedPanel,
        pending_actions: &'a mut Vec<EditorAction>,
        theme: &'a Theme,
        edit: &'a mut InspectorEditState,
    ) -> Self {
        Self {
            bounds,
            selected_entity,
            entities,
            focused_panel,
            pending_actions,
            theme,
            edit,
        }
    }
}

/// Draw a labeled slider row: label on the left, value display + slider on the right.
fn vec3_slider_row(
    ui: &mut UiContext,
    theme: &Theme,
    label: &str,
    values: &mut [f32; 3],
    axis_labels: [&str; 3],
    range: std::ops::RangeInclusive<f32>,
    slider_width: f32,
) {
    let row_height = ROW_HEIGHT;
    let font_size = ui.scaled_font_size(FontSize::Small);
    ui.draw_text(label, ui.cursor(), theme.text_accent, font_size);
    ui.spacing(row_height);

    let indent = 8.0;
    let value_label_width = 18.0;
    let slider_area = slider_width - indent - value_label_width;

    for (i, (axis_label, val)) in axis_labels.iter().zip(values.iter_mut()).enumerate() {
        let cursor = ui.cursor();
        let label_pos = Vec2::new(cursor.x() + indent, cursor.y());
        ui.draw_text(axis_label, label_pos, theme.text_muted, font_size);

        let slider_x = cursor.x() + indent + value_label_width;
        let slider_bounds = Rect2D::from_origin_size(
            Vec2::new(slider_x, cursor.y()),
            Vec2::new(slider_area, row_height),
        );

        let id = format!("{}_{}", label.to_lowercase(), i);
        ui.add(
            Slider::new(&id, val, range.clone())
                .bounds(slider_bounds)
                .id(&id),
        );

        // Display current value next to slider
        let val_text = format!("{:.2}", val);
        let val_pos = Vec2::new(slider_x + slider_area + 4.0, cursor.y());
        ui.draw_text(&val_text, val_pos, theme.text_primary, font_size);

        ui.spacing(row_height);
    }
}

/// Draw a single labeled slider row with value display.
fn scalar_slider_row(
    ui: &mut UiContext,
    theme: &Theme,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    slider_width: f32,
    row_height: f32,
) {
    let font_size = ui.scaled_font_size(FontSize::Small);
    let cursor = ui.cursor();

    let label_width = 90.0;
    let value_label_width = 50.0;
    let slider_area = slider_width - label_width - value_label_width;

    ui.draw_text(label, cursor, theme.text_muted, font_size);

    let slider_x = cursor.x() + label_width;
    let slider_bounds = Rect2D::from_origin_size(
        Vec2::new(slider_x, cursor.y()),
        Vec2::new(slider_area, row_height),
    );

    let id = format!("slider_{}", label.to_lowercase().replace(' ', "_"));
    let _response = ui.add(Slider::new(&id, value, range).bounds(slider_bounds).id(&id));

    let val_text = format!("{:.2}", value);
    let val_pos = Vec2::new(slider_x + slider_area + 4.0, cursor.y());
    ui.draw_text(&val_text, val_pos, theme.text_primary, font_size);

    ui.spacing(row_height);
}

impl<'a> Widget for Inspector<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        if ui.is_hovered(self.bounds)
            && (ui.mouse_down(mouse_button::LEFT)
                || ui.mouse_down(mouse_button::RIGHT)
                || ui.mouse_down(mouse_button::MIDDLE))
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

        let row_height = 18.0;
        let content_x = self.bounds.min.x() + 8.0;
        let content_width = self.bounds.width() - 16.0;

        ui.begin_column();
        ui.set_cursor(Vec2::new(
            content_x,
            self.bounds.min.y() + header_height + 8.0,
        ));

        if let Some(entity) = selected {
            // Entity name (read-only)
            ui.draw_text(
                &entity.name,
                ui.cursor(),
                self.theme.text_primary,
                ui.scaled_font_size(FontSize::Large),
            );
            ui.spacing(row_height + 8.0);

            // Transform section with interactive sliders
            ui.draw_text(
                "Transform",
                ui.cursor(),
                self.theme.text_accent,
                ui.scaled_font_size(FontSize::Medium),
            );
            ui.spacing(row_height);

            vec3_slider_row(
                ui,
                self.theme,
                "Position",
                &mut self.edit.pos,
                ["X", "Y", "Z"],
                -100.0..=100.0,
                content_width,
            );

            vec3_slider_row(
                ui,
                self.theme,
                "Rotation",
                &mut self.edit.rot,
                ["X", "Y", "Z"],
                -180.0..=180.0,
                content_width,
            );

            vec3_slider_row(
                ui,
                self.theme,
                "Scale",
                &mut self.edit.scale,
                ["X", "Y", "Z"],
                0.01..=100.0,
                content_width,
            );

            ui.spacing(4.0);
            ui.separator_line();
            ui.spacing(4.0);

            // PointLight section (if entity has PointLight)
            if entity.point_light.is_some() {
                ui.draw_text(
                    "Point Light",
                    ui.cursor(),
                    self.theme.text_accent,
                    ui.scaled_font_size(FontSize::Medium),
                );
                ui.spacing(row_height);

                // Color sliders (R, G, B)
                let font_size = ui.scaled_font_size(FontSize::Small);
                let indent = 8.0;
                let value_label_width = 18.0;
                let slider_area = content_width - indent - value_label_width;

                for (i, axis_label) in ["R", "G", "B"].iter().enumerate() {
                    let cursor = ui.cursor();
                    let label_pos = Vec2::new(cursor.x() + indent, cursor.y());
                    let color = match i {
                        0 => Color::new(1.0, 0.3, 0.3, 1.0),
                        1 => Color::new(0.3, 1.0, 0.3, 1.0),
                        _ => Color::new(0.3, 0.3, 1.0, 1.0),
                    };
                    ui.draw_text(axis_label, label_pos, color, font_size);

                    let slider_x = cursor.x() + indent + value_label_width;
                    let slider_bounds = Rect2D::from_origin_size(
                        Vec2::new(slider_x, cursor.y()),
                        Vec2::new(slider_area, row_height),
                    );
                    let id = format!("light_color_{}", i);
                    let _response = ui.add(
                        Slider::new(&id, &mut self.edit.light_color[i], 0.0..=1.0)
                            .bounds(slider_bounds)
                            .id(&id),
                    );

                    let val_text = format!("{:.2}", self.edit.light_color[i]);
                    let val_pos = Vec2::new(slider_x + slider_area + 4.0, cursor.y());
                    ui.draw_text(&val_text, val_pos, self.theme.text_primary, font_size);
                    ui.spacing(row_height);
                }

                scalar_slider_row(
                    ui,
                    self.theme,
                    "Intensity",
                    &mut self.edit.light_intensity,
                    0.0..=100.0,
                    content_width,
                    row_height,
                );

                scalar_slider_row(
                    ui,
                    self.theme,
                    "Range",
                    &mut self.edit.light_range,
                    0.1..=100.0,
                    content_width,
                    row_height,
                );

                ui.spacing(4.0);
                ui.separator_line();
                ui.spacing(4.0);
            }

            // ParticleEmitter section (if entity has ParticleEmitter)
            if entity.particle_emitter.is_some() {
                ui.draw_text(
                    "Particle Emitter",
                    ui.cursor(),
                    self.theme.text_accent,
                    ui.scaled_font_size(FontSize::Medium),
                );
                ui.spacing(row_height);

                scalar_slider_row(
                    ui,
                    self.theme,
                    "Emit Rate",
                    &mut self.edit.emit_rate,
                    0.0..=1000.0,
                    content_width,
                    row_height,
                );

                scalar_slider_row(
                    ui,
                    self.theme,
                    "Velocity",
                    &mut self.edit.velocity,
                    0.0..=50.0,
                    content_width,
                    row_height,
                );

                scalar_slider_row(
                    ui,
                    self.theme,
                    "Lifetime",
                    &mut self.edit.lifetime,
                    0.1..=30.0,
                    content_width,
                    row_height,
                );

                scalar_slider_row(
                    ui,
                    self.theme,
                    "Gravity",
                    &mut self.edit.gravity,
                    -30.0..=30.0,
                    content_width,
                    row_height,
                );

                scalar_slider_row(
                    ui,
                    self.theme,
                    "Particle Scale",
                    &mut self.edit.particle_scale,
                    0.01..=5.0,
                    content_width,
                    row_height,
                );

                ui.spacing(4.0);
                ui.separator_line();
                ui.spacing(4.0);
            }

            // Type and Components (read-only)
            ui.draw_text(
                "Type",
                ui.cursor(),
                self.theme.text_accent,
                ui.scaled_font_size(FontSize::Medium),
            );
            ui.spacing(row_height);
            ui.label(&entity.entity_type);

            ui.draw_text(
                "Components",
                ui.cursor(),
                self.theme.text_accent,
                ui.scaled_font_size(FontSize::Medium),
            );
            ui.spacing(row_height);
            for component_name in &entity.components {
                ui.label(component_name);
            }

            ui.spacing(8.0);

            let delete_bounds = Rect2D::from_origin_size(
                Vec2::new(content_x, ui.cursor().y()),
                Vec2::new(content_width, 28.0),
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
            ui.draw_empty_state(self.bounds, "No entity selected");
        }

        ui.end_column();

        Response::default()
    }
}
