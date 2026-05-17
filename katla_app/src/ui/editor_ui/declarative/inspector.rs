use std::cell::RefCell;

use katla_ecs::EntityId;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};
use katla_ui::{
    FontSize, ForkAwesome, ScrollArea, ScrollAreaState, UiContext,
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
    pub available_components: Vec<&'static str>,
    pub add_component_open: bool,
    pub add_component_filter: String,
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

const COMPONENT_CATEGORIES: &[(&str, &[&str])] = &[
    ("Lighting", &["PointLight", "DirectionalLight"]),
    (
        "Physics",
        &["MassComponent", "DragComponent", "VelocityComponent"],
    ),
    ("Scripting", &["ScriptComponent"]),
    ("Particles", &["ParticleEmitterComponent"]),
    ("Camera", &["PerspectiveComponent"]),
    ("General", &["NameComponent"]),
];

fn component_category(name: &str) -> &'static str {
    for (category, components) in COMPONENT_CATEGORIES {
        if components.contains(&name) {
            return category;
        }
    }
    "General"
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

fn section_header_with_remove(
    ui: &mut UiContext,
    text: &str,
    _entity_id: EntityId,
    _component_type: &str,
    theme: &ColorScheme,
    w: f32,
) -> bool {
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
    let text_y = y + 4.0;
    ui.draw_text(
        text,
        Vec2::new(cursor.x(), text_y),
        theme.text_accent,
        font_size,
    );

    let remove_size = 16.0;
    let remove_bounds = Rect2D::from_origin_size(
        Vec2::new(cursor.x() + w - remove_size, text_y - 1.0),
        Vec2::new(remove_size, remove_size),
    );
    let hovered = ui.is_hovered(remove_bounds);
    let icon_color = if hovered {
        theme.error
    } else {
        theme.text_muted
    };
    ui.draw_icon(
        ForkAwesome::TIMES,
        remove_bounds.min,
        remove_size,
        icon_color,
    );
    let mut remove_clicked = false;
    if hovered && ui.mouse_clicked(katla_ui::mouse_button::LEFT) {
        remove_clicked = true;
    }

    ui.set_cursor(Vec2::new(cursor.x(), text_y + text_h + 4.0));
    remove_clicked
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
                    if section_header_with_remove(
                        ui,
                        "Point Light",
                        entity_id,
                        "PointLight",
                        theme,
                        w,
                    ) {
                        ctx.pending_actions.push(EditorAction::RemoveComponent {
                            entity: entity_id,
                            component_type: "PointLight".to_string(),
                        });
                    }

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
                    if section_header_with_remove(
                        ui,
                        "Particle Emitter",
                        entity_id,
                        "ParticleEmitterComponent",
                        theme,
                        w,
                    ) {
                        ctx.pending_actions.push(EditorAction::RemoveComponent {
                            entity: entity_id,
                            component_type: "ParticleEmitterComponent".to_string(),
                        });
                    }
                    scalar_row(ui, "Emit Rate", &mut edit.emit_rate, 0.0..=1000.0, w);
                    scalar_row(ui, "Velocity", &mut edit.velocity, 0.0..=50.0, w);
                    scalar_row(ui, "Lifetime", &mut edit.lifetime, 0.1..=30.0, w);
                    scalar_row(ui, "Gravity", &mut edit.gravity, -30.0..=30.0, w);
                    scalar_row(ui, "Scale", &mut edit.particle_scale, 0.01..=5.0, w);
                    section_gap(ui);
                }

                if entity.script_path.is_some() {
                    if section_header_with_remove(
                        ui,
                        "Script",
                        entity_id,
                        "ScriptComponent",
                        theme,
                        w,
                    ) {
                        ctx.pending_actions.push(EditorAction::RemoveComponent {
                            entity: entity_id,
                            component_type: "ScriptComponent".to_string(),
                        });
                    }

                    let path_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(w, 22.0));
                    let resp = ui.add(
                        katla_ui::widgets::TextInput::new("script_path", &mut edit.script_path)
                            .bounds(path_bounds)
                            .placeholder("scripts/example.lua"),
                    );
                    if resp.changed || resp.enter_pressed {
                        ctx.pending_actions.push(EditorAction::SetScriptPath {
                            entity: entity_id,
                            path: edit.script_path.clone(),
                        });
                    }
                    ui.set_cursor(Vec2::new(x, ui.cursor().y() + 26.0));
                    section_gap(ui);
                }

                section_header(ui, "Info", theme);
                ui.property_row("Type", &entity.entity_type);
                for component_name in &entity.components {
                    ui.property_row("", component_name);
                }

                ui.set_cursor(Vec2::new(x, ui.cursor().y() + 8.0));

                let add_btn_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(w, 24.0));
                let add_clicked = ui
                    .add(
                        Button::new("Add Component")
                            .bounds(add_btn_bounds)
                            .id("add_component_btn")
                            .fill_color(theme.button_bg)
                            .hover_color(theme.button_hover)
                            .border(theme.border),
                    )
                    .clicked;

                if add_clicked && !ui.has_open_popup() {
                    ctx.add_component_open = true;
                    ctx.add_component_filter.clear();
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

        if ctx.add_component_open {
            let entity_existing: Vec<String> = entity.components.clone();
            let filter = ctx.add_component_filter.to_lowercase();
            let filtered: Vec<&str> = ctx
                .available_components
                .iter()
                .copied()
                .filter(|name| {
                    !entity_existing.iter().any(|e| e == *name)
                        && (filter.is_empty() || name.to_lowercase().contains(&filter))
                })
                .collect();

            let popup_width = 250.0;
            let popup_height = 200.0;
            let popup_x = content_bounds.max.x() - popup_width - 4.0;
            let popup_y = content_bounds.min.y() + 40.0;

            ui.modal(
                "add_component_popup",
                popup_width,
                popup_height,
                &mut ctx.add_component_open,
                |ui, open| {
                    let dialog_bounds = ui.get_popup_bounds();
                    let dialog_pos = dialog_bounds.min;

                    let search_bounds =
                        Rect2D::from_origin_size(dialog_pos, Vec2::new(popup_width - 8.0, 22.0));
                    ui.add(
                        katla_ui::widgets::TextInput::new(
                            "component_search",
                            &mut ctx.add_component_filter,
                        )
                        .bounds(search_bounds)
                        .placeholder("Search components..."),
                    );

                    ui.set_cursor(Vec2::new(dialog_pos.x(), dialog_pos.y() + 28.0));

                    let list_bounds = Rect2D::from_origin_size(
                        Vec2::new(dialog_pos.x(), dialog_pos.y() + 28.0),
                        Vec2::new(popup_width, popup_height - 60.0),
                    );

                    let font_size = ui.scaled_font_size(FontSize::Small);
                    let item_h = 20.0;

                    ui.push_clip(list_bounds);

                    let header_h = 16.0;
                    let mut y = list_bounds.min.y();

                    for &(category_name, _) in COMPONENT_CATEGORIES {
                        let category_items: Vec<&&str> = filtered
                            .iter()
                            .filter(|name| component_category(*name) == category_name)
                            .collect();
                        if category_items.is_empty() {
                            continue;
                        }

                        let header_bounds = Rect2D::from_origin_size(
                            Vec2::new(list_bounds.min.x(), y),
                            Vec2::new(list_bounds.width(), header_h),
                        );
                        ui.draw_text(
                            category_name,
                            Vec2::new(header_bounds.min.x() + 4.0, y + 1.0),
                            theme.text_muted,
                            font_size,
                        );
                        let header_text_w = ui.measure_text(category_name, font_size).x();
                        let line_y = y + header_h * 0.5;
                        ui.draw_line(
                            Vec2::new(header_bounds.min.x() + header_text_w + 8.0, line_y),
                            Vec2::new(header_bounds.max.x() - 4.0, line_y),
                            theme.separator,
                            1.0,
                        );
                        y += header_h;

                        for name in &category_items {
                            let item_bounds = Rect2D::from_origin_size(
                                Vec2::new(list_bounds.min.x(), y),
                                Vec2::new(list_bounds.width(), item_h),
                            );

                            let hovered = ui.is_hovered(item_bounds);
                            if hovered {
                                ui.draw_rect(item_bounds, theme.selection_hover);
                            }

                            ui.draw_text(
                                name,
                                Vec2::new(item_bounds.min.x() + 6.0, y + 2.0),
                                if hovered {
                                    theme.text_primary
                                } else {
                                    theme.text_secondary
                                },
                                font_size,
                            );

                            if hovered && ui.mouse_clicked(katla_ui::mouse_button::LEFT) {
                                ctx.pending_actions.push(EditorAction::AddComponent {
                                    entity: entity_id,
                                    component_type: (**name).to_string(),
                                });
                                *open = false;
                            }

                            y += item_h;
                        }

                        y += 4.0;
                    }

                    if filtered.is_empty() {
                        let msg = if filter.is_empty() {
                            "All components added"
                        } else {
                            "No matching components"
                        };
                        ui.draw_text(
                            msg,
                            Vec2::new(list_bounds.min.x() + 6.0, list_bounds.min.y() + 4.0),
                            theme.text_muted,
                            font_size,
                        );
                    }

                    ui.pop_clip();

                    ui.set_cursor(Vec2::new(
                        dialog_pos.x(),
                        dialog_pos.y() + popup_height - 28.0,
                    ));
                    let cancel_bounds =
                        Rect2D::from_origin_size(ui.cursor(), Vec2::new(popup_width - 8.0, 22.0));
                    if ui
                        .add(Button::new("Cancel").bounds(cancel_bounds).id("cancel_add"))
                        .clicked
                    {
                        *open = false;
                    }
                },
            );
        }
    } else {
        ui.draw_empty_state(ctx.bounds, "No entity selected");
    }

    set_inspector_ctx(ctx);
}
