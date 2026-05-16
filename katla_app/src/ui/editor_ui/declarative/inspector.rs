use std::cell::RefCell;

use katla_ecs::EntityId;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};
use katla_ui::{
    FontSize, ScrollArea, ScrollAreaState, UiContext,
    widgets::{Button, ColorPickerButton, LabeledSlider, Panel, Vec3Slider},
};

use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::{EditorAction, EntityInfo, InspectorEditState};

thread_local! {
    static INSPECTOR_CTX: RefCell<Option<InspectorDrawCtx>> = const { RefCell::new(None) };
}

pub(crate) struct InspectorDrawCtx {
    pub bounds: Rect2D,
    pub selected_entity: Option<EntityId>,
    pub entities: Vec<EntityInfo>,
    pub edit: InspectorEditState,
    pub scroll_state: ScrollAreaState,
    pub theme: ColorScheme,
    pub pending_actions: Vec<EditorAction>,
}

pub(crate) fn set_inspector_ctx(ctx: InspectorDrawCtx) {
    INSPECTOR_CTX.with(|c| *c.borrow_mut() = Some(ctx));
}

pub(crate) fn take_inspector_ctx() -> Option<InspectorDrawCtx> {
    INSPECTOR_CTX.with(|c| c.borrow_mut().take())
}

pub(crate) struct InspectorView;

impl Build for InspectorView {
    fn build(&self, _ctx: &mut BuildContext) -> ViewDescriptor {
        ViewDescriptor::Custom(draw_inspector)
    }
}

const AXIS_COLORS: [Color; 3] = [
    Color::rgb(0.9, 0.3, 0.3),
    Color::rgb(0.3, 0.9, 0.3),
    Color::rgb(0.3, 0.5, 0.9),
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
    let font_size = ui.scaled_font_size(FontSize::Small);
    let text_h = ui.measure_text(text, font_size).y();
    ui.draw_text(
        text,
        Vec2::new(cursor.x(), y + 4.0),
        theme.text_accent,
        font_size,
    );
    ui.set_cursor(Vec2::new(cursor.x(), y + 4.0 + text_h + 4.0));
}

fn section_gap(ui: &mut UiContext) {
    let cursor = ui.cursor();
    ui.set_cursor(Vec2::new(cursor.x(), cursor.y() + ui.style().item_spacing));
}

fn vec3_row(
    ui: &mut UiContext,
    label: &str,
    values: &mut [f32; 3],
    range: std::ops::RangeInclusive<f32>,
    axis_labels: [&str; 3],
    axis_colors: [Color; 3],
    width: f32,
    theme: &ColorScheme,
) {
    let font_size = ui.style().font_size;
    let label_h = ui.measure_text(label, font_size).y();
    ui.draw_text(label, ui.cursor(), theme.text_muted, font_size);
    ui.set_cursor(Vec2::new(ui.cursor().x(), ui.cursor().y() + label_h + 2.0));

    let row_h = ui.style().slider_default_height;
    let bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(width, row_h * 3.0));
    ui.add(
        Vec3Slider::new(label, values, range)
            .bounds(bounds)
            .axis_labels(axis_labels)
            .axis_colors(axis_colors)
            .precision(2),
    );
}

fn scalar_row(
    ui: &mut UiContext,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    width: f32,
) {
    let row_h = ui.style().slider_default_height;
    let bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(width, row_h));
    ui.add(
        LabeledSlider::new(label, value, range)
            .bounds(bounds)
            .label_width(80.0)
            .show_value(true)
            .precision(2),
    );
}

fn draw_inspector(ui: &mut UiContext, _bounds: Rect2D) {
    let mut ctx = match take_inspector_ctx() {
        Some(ctx) => ctx,
        None => return,
    };

    let content_bounds = {
        let guard = Panel::new("Inspector")
            .bounds(ctx.bounds)
            .header_height(24.0)
            .show(ui);
        guard.content_bounds()
    };

    let selected = ctx
        .selected_entity
        .and_then(|id| ctx.entities.iter().find(|e| e.id == id));

    if let Some(entity) = selected {
        let theme = &ctx.theme;
        let edit = &mut ctx.edit;
        let entity_id = entity.id;

        ctx.scroll_state = ui.scroll_area(
            ScrollArea::new("inspector_scroll").max_height(content_bounds.height()),
            ctx.scroll_state,
            content_bounds,
            |ui| {
                let padding = ui.style().panel_padding;
                let scrollbar_w = ui.style().scrollbar_width;
                let x = content_bounds.min.x() + padding;
                let w = content_bounds.width() - scrollbar_w - 2.0 * padding;

                ui.set_cursor(Vec2::new(x, content_bounds.min.y() + 2.0));

                ui.draw_text(
                    &entity.name,
                    ui.cursor(),
                    theme.text_primary,
                    ui.scaled_font_size(FontSize::Medium),
                );
                let name_h = ui
                    .measure_text(&entity.name, ui.scaled_font_size(FontSize::Medium))
                    .y();
                ui.set_cursor(Vec2::new(x, ui.cursor().y() + name_h + 6.0));

                section_header(ui, "Transform", theme);
                vec3_row(
                    ui,
                    "Position",
                    &mut edit.pos,
                    -100.0..=100.0,
                    ["X", "Y", "Z"],
                    AXIS_COLORS,
                    w,
                    theme,
                );
                vec3_row(
                    ui,
                    "Rotation",
                    &mut edit.rot,
                    -180.0..=180.0,
                    ["X", "Y", "Z"],
                    AXIS_COLORS,
                    w,
                    theme,
                );
                vec3_row(
                    ui,
                    "Scale",
                    &mut edit.scale,
                    0.01..=100.0,
                    ["X", "Y", "Z"],
                    AXIS_COLORS,
                    w,
                    theme,
                );

                section_gap(ui);

                if entity.point_light.is_some() {
                    section_header(ui, "Point Light", theme);

                    let picker_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(w, 28.0));
                    ui.add_overlay(
                        ColorPickerButton::new(
                            "Color",
                            &mut edit.light_color,
                            &mut edit.light_color_picker,
                        )
                        .bounds(picker_bounds)
                        .id("light_color_picker"),
                    );
                    ui.set_cursor(Vec2::new(x, ui.cursor().y() + 32.0));

                    scalar_row(ui, "Intensity", &mut edit.light_intensity, 0.0..=100.0, w);
                    scalar_row(ui, "Range", &mut edit.light_range, 0.1..=100.0, w);
                    section_gap(ui);
                }

                if entity.particle_emitter.is_some() {
                    section_header(ui, "Particle Emitter", theme);
                    scalar_row(ui, "Emit Rate", &mut edit.emit_rate, 0.0..=1000.0, w);
                    scalar_row(ui, "Velocity", &mut edit.velocity, 0.0..=50.0, w);
                    scalar_row(ui, "Lifetime", &mut edit.lifetime, 0.1..=30.0, w);
                    scalar_row(ui, "Gravity", &mut edit.gravity, -30.0..=30.0, w);
                    scalar_row(ui, "Scale", &mut edit.particle_scale, 0.01..=5.0, w);
                    section_gap(ui);
                }

                section_header(ui, "Info", theme);
                ui.property_row("Type", &entity.entity_type);
                for component_name in &entity.components {
                    ui.property_row("", component_name);
                }

                ui.set_cursor(Vec2::new(x, ui.cursor().y() + 8.0));

                let delete_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(w, 24.0));
                let clicked = ui
                    .add(
                        Button::new("Delete Entity")
                            .bounds(delete_bounds)
                            .id("delete_entity")
                            .fill_color(Color::new(0.4, 0.1, 0.1, 1.0))
                            .hover_color(Color::new(0.5, 0.15, 0.15, 1.0))
                            .border(Color::new(1.0, 0.3, 0.3, 0.2)),
                    )
                    .clicked;

                if clicked {
                    ctx.pending_actions
                        .push(EditorAction::DeleteEntity(entity_id));
                    ctx.selected_entity = None;
                }

                ui.cursor().y() - content_bounds.min.y() + 40.0
            },
        );
    } else {
        ui.draw_empty_state(ctx.bounds, "No entity selected");
    }

    set_inspector_ctx(ctx);
}
