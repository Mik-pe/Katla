use katla_math::{Color, Rect2D, Vec2};

use crate::context::UiContext;
use crate::icons::ForkAwesome;

use super::animation::AnimationState;
use super::descriptor::ViewDescriptor;
use super::state::StateArena;
use super::tree::InteractionState;

pub(crate) fn draw_descriptor(
    descriptor: &ViewDescriptor,
    ui: &mut UiContext,
    bounds: Rect2D,
    state_arena: &StateArena,
    children_bounds: &[Rect2D],
    interaction: &InteractionState,
    _anim_state: &AnimationState,
) {
    match descriptor {
        ViewDescriptor::Empty => {}

        ViewDescriptor::Text {
            content,
            color,
            font_size,
        } => {
            let text_color = color.unwrap_or(ui.style().text_color);
            let size = font_size
                .map(|fs| ui.scaled_font_size(fs))
                .unwrap_or(ui.style().font_size);
            ui.draw_text(content, bounds.min, text_color, size);
        }

        ViewDescriptor::Button {
            label,
            fill_color,
            hover_color: _,
            border_color,
            on_click: _,
        } => {
            let bg = fill_color.unwrap_or(ui.style().button_normal);
            let radius = ui.style().button_rounding;
            ui.draw_rounded_rect(bounds, bg, radius);

            if let Some(border) = border_color {
                ui.draw_rounded_selection_border(bounds, *border, 1.0, radius);
            }

            let font_size = ui.style().font_size;
            let text_size = ui.measure_text(label, font_size);
            let text_pos = Vec2::new(
                bounds.center().x() - text_size.x() * 0.5,
                bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(label, text_pos, ui.style().button_text, font_size);
        }

        ViewDescriptor::Slider {
            label,
            value_id,
            range,
            show_value,
            precision,
        } => {
            let value = state_arena.get::<f32>(*value_id);
            let start = *range.start();
            let end = *range.end();
            let t = if end > start {
                ((value - start) / (end - start)).clamp(0.0, 1.0)
            } else {
                0.0
            };

            // Draw background
            let bg = ui.style().input_bg;
            let border = ui.style().input_border;
            let radius = ui.style().input_rounding;
            ui.draw_rounded_rect(bounds, bg, radius);
            ui.draw_rounded_selection_border(bounds, border, 1.0, radius);

            // Track
            let track_height = ui.style().slider_track_height;
            let track_bounds = Rect2D::from_center_size(
                Vec2::new(bounds.center().x(), bounds.center().y()),
                Vec2::new(bounds.width(), track_height),
            );
            ui.draw_rounded_rect(track_bounds, ui.style().slider_track, track_height * 0.5);

            // Filled portion
            let fill_width = t * bounds.width();
            if fill_width > 0.0 {
                let fill_bounds =
                    Rect2D::from_origin_size(track_bounds.min, Vec2::new(fill_width, track_height));
                ui.draw_rounded_rect(fill_bounds, ui.style().slider_grab, track_height * 0.5);
            }

            // Grab circle
            let grab_center_x = bounds.min.x() + t * bounds.width();
            let grab_center = Vec2::new(grab_center_x, bounds.center().y());
            let base_radius = ui.style().slider_grab_size * 0.5;
            let grab_radius = base_radius * 1.15;

            ui.draw_circle(
                Vec2::new(grab_center.x(), grab_center.y() + 1.0),
                grab_radius,
                Color::new(0.0, 0.0, 0.0, 0.3),
            );
            ui.draw_circle(grab_center, grab_radius, ui.style().slider_grab);

            // Label (if not an ID-only label)
            let padding = 4.0;
            if label.strip_prefix("##").is_none() && !label.is_empty() {
                let font_size = ui.style().font_size;
                let text_color = ui.style().text_color;
                ui.draw_text(
                    label,
                    Vec2::new(bounds.min.x() + padding, bounds.min.y()),
                    text_color,
                    font_size,
                );
            }

            // Value text
            if *show_value {
                let value_text = format!("{:.1$}", value, precision);
                let font_size = ui.style().font_size;
                let text_size = ui.measure_text(&value_text, font_size);
                let text_pos = Vec2::new(
                    bounds.max.x() - text_size.x() - padding,
                    bounds.center().y() - text_size.y() * 0.5,
                );
                ui.draw_text(&value_text, text_pos, ui.style().text_color, font_size);
            }
        }

        ViewDescriptor::Toggle { label, value_id } => {
            let checked = state_arena.get::<bool>(*value_id);

            // Checkbox box
            let check_size = bounds.height().min(20.0);
            let check_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.center().y() - check_size * 0.5),
                Vec2::new(check_size, check_size),
            );
            let check_rounding = 3.0;

            let bg_color = if checked {
                ui.style().checkbox_check
            } else {
                ui.style().checkbox_bg
            };
            ui.draw_rounded_rect(check_bounds, bg_color, check_rounding);
            if !checked {
                ui.draw_rounded_selection_border(
                    check_bounds,
                    ui.style().checkbox_border,
                    1.0,
                    check_rounding,
                );
            }

            if checked {
                let icon_size = check_size * 0.7;
                ui.draw_icon_centered(ForkAwesome::CHECK, check_bounds, icon_size, Color::WHITE);
            }

            // Label
            let font_size = ui.style().font_size;
            let text_size = ui.measure_text(label, font_size);
            let label_pos = Vec2::new(
                check_bounds.max.x() + ui.style().item_inner_spacing,
                bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(label, label_pos, ui.style().text_color, font_size);
        }

        ViewDescriptor::TextField {
            placeholder,
            value_id,
            on_submit: _,
        } => {
            let text: String = state_arena.get::<String>(*value_id);
            let is_focused = interaction
                .focused_id
                .is_some_and(|id| state_arena_cell_contains_id(state_arena, id));

            // Background
            let bg = ui.style().input_bg;
            let border = if is_focused {
                ui.style().input_border_focused
            } else {
                ui.style().input_border
            };
            let radius = ui.style().input_rounding;
            ui.draw_rounded_rect(bounds, bg, radius);
            ui.draw_rounded_selection_border(bounds, border, 1.0, radius);

            // Focus ring
            if is_focused {
                let focus_ring_width = ui.style().focus_ring_width;
                let focus_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        bounds.min.x() - focus_ring_width,
                        bounds.min.y() - focus_ring_width,
                    ),
                    Vec2::new(
                        bounds.width() + focus_ring_width * 2.0,
                        bounds.height() + focus_ring_width * 2.0,
                    ),
                );
                ui.draw_rounded_selection_border(
                    focus_bounds,
                    ui.style().focus_ring_color,
                    focus_ring_width,
                    radius,
                );
            }

            let font_size = ui.style().font_size;
            let padding = ui.style().text_input_padding;
            let text_pos = Vec2::new(
                bounds.min.x() + padding,
                bounds.center().y() - font_size * 0.5,
            );

            // Clip to bounds
            let text_bounds = Rect2D::from_origin_size(
                bounds.min,
                Vec2::new(bounds.width() - padding, bounds.height()),
            );
            ui.push_clip(text_bounds);

            if text.is_empty() && !is_focused {
                ui.draw_text(placeholder, text_pos, ui.style().text_hint, font_size);
            } else {
                ui.draw_text(&text, text_pos, ui.style().input_text, font_size);

                // Cursor (blink)
                if is_focused {
                    let cursor_x = text_pos.x() + ui.measure_text(&text, font_size).x();
                    let blink_on = (ui.time * 2.0 * std::f64::consts::PI).sin() > 0.0;
                    if blink_on {
                        ui.draw_line(
                            Vec2::new(cursor_x, text_pos.y()),
                            Vec2::new(cursor_x, text_pos.y() + font_size),
                            ui.style().input_cursor,
                            ui.style().text_input_cursor_width,
                        );
                    }
                }
            }

            ui.pop_clip();
        }

        ViewDescriptor::Progress {
            value,
            range,
            fill_color,
        } => {
            let start = *range.start();
            let end = *range.end();
            let fraction = if end > start {
                ((value - start) / (end - start)).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let track_color = ui.style().slider_track;
            ui.draw_rect(bounds, track_color);

            let fill_width = bounds.width() * fraction;
            let fill_bounds = Rect2D::new(
                bounds.min,
                Vec2::new(bounds.min.x() + fill_width, bounds.max.y()),
            );
            let fill = fill_color.unwrap_or(ui.style().slider_grab);
            ui.draw_rect(fill_bounds, fill);
        }

        ViewDescriptor::ColorPicker { label, value_id } => {
            let is_open = state_arena.get::<bool>(*value_id);
            let bg = ui.style().input_bg;
            let border = ui.style().input_border;
            let radius = ui.style().input_rounding;
            ui.draw_rounded_rect(bounds, bg, radius);
            ui.draw_rounded_selection_border(bounds, border, 1.0, radius);

            // Color swatch
            let swatch_padding = 4.0;
            let swatch_size = bounds.height() - swatch_padding * 2.0;
            let swatch_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    bounds.min.x() + swatch_padding,
                    bounds.min.y() + swatch_padding,
                ),
                Vec2::new(swatch_size, swatch_size),
            );
            let swatch_color = Color::rgb(0.5, 0.5, 0.5);
            ui.draw_rounded_rect(swatch_bounds, swatch_color, 2.0);

            // Label
            let font_size = ui.style().font_size;
            let label_x = swatch_bounds.max.x() + swatch_padding;
            let text_size = ui.measure_text(label, font_size);
            let label_y = bounds.center().y() - text_size.y() * 0.5;
            if label.strip_prefix("##").is_none() && !label.is_empty() {
                ui.draw_text(
                    label,
                    Vec2::new(label_x, label_y),
                    ui.style().text_color,
                    font_size,
                );
            }

            // Open/closed indicator
            let indicator = if is_open { "▲" } else { "▼" };
            let ind_size = ui.measure_text(indicator, font_size);
            let ind_x = bounds.max.x() - ind_size.x() - swatch_padding;
            ui.draw_text(
                indicator,
                Vec2::new(ind_x, label_y),
                ui.style().text_disabled,
                font_size,
            );
        }

        ViewDescriptor::Image { texture, uv, tint } => {
            let uv_rect =
                uv.unwrap_or_else(|| Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)));
            ui.draw_image(bounds, uv_rect.min, uv_rect.max, *tint, *texture);
        }

        ViewDescriptor::HStack(_) | ViewDescriptor::VStack(_) | ViewDescriptor::ZStack(_) => {}

        ViewDescriptor::ScrollView(_) => {
            let bg = ui.style().window_bg;
            ui.draw_rect(bounds, bg);
        }

        ViewDescriptor::Panel(desc) => {
            let bg = ui.style().window_bg;
            ui.draw_rect(bounds, bg);

            let header_bounds = Rect2D::new(
                bounds.min,
                Vec2::new(bounds.max.x(), bounds.min.y() + desc.header_height),
            );
            ui.draw_rect(header_bounds, ui.style().window_title_bg);

            let font_size = ui.style().font_size;
            let padding = ui.style().window_padding;
            let text_pos = Vec2::new(
                header_bounds.min.x() + padding,
                header_bounds.min.y() + (desc.header_height - font_size) * 0.5,
            );
            ui.draw_text(
                &desc.title,
                text_pos,
                ui.style().window_title_text,
                font_size,
            );

            if children_bounds.len() > 1 {
                ui.draw_line(
                    Vec2::new(bounds.min.x(), header_bounds.max.y()),
                    Vec2::new(bounds.max.x(), header_bounds.max.y()),
                    ui.style().window_border,
                    1.0,
                );
            }
        }

        ViewDescriptor::Overlay(_) => {}

        ViewDescriptor::Custom(draw_fn) => {
            draw_fn(ui, bounds);
        }

        ViewDescriptor::TransitionContainer { .. } => {}
    }
}

/// Stand-in: we cannot determine the ViewId from the draw context alone.
/// The interactive draw variant (below) passes the ViewId through.
/// This is kept as a compatibility shim but should be replaced by
/// `draw_descriptor_with_id` in the tree walk.
fn state_arena_cell_contains_id(_state_arena: &StateArena, _id: super::state::ViewId) -> bool {
    // This function is intentionally a no-op placeholder.
    // The real hover/active/focused detection is done via ViewId comparison
    // in `draw_descriptor_with_id` below.
    false
}

/// Draw a descriptor with explicit ViewId for interaction state checks.
pub(crate) fn draw_descriptor_with_id(
    descriptor: &ViewDescriptor,
    ui: &mut UiContext,
    bounds: Rect2D,
    state_arena: &StateArena,
    children_bounds: &[Rect2D],
    interaction: &InteractionState,
    view_id: super::state::ViewId,
    anim_state: &AnimationState,
) {
    let is_hovered = interaction.hovered_id == Some(view_id);
    let is_active = interaction.active_id == Some(view_id);
    let is_focused = interaction.focused_id == Some(view_id);

    match descriptor {
        ViewDescriptor::Button {
            label,
            fill_color,
            hover_color,
            border_color,
            on_click: _,
        } => {
            let bg = if is_active {
                hover_color.unwrap_or(ui.style().button_active)
            } else if is_hovered {
                hover_color.unwrap_or(ui.style().button_hovered)
            } else {
                fill_color.unwrap_or(ui.style().button_normal)
            };
            let bg = anim_state.apply_to_color(bg);
            let radius = anim_state.apply_to_corner_radius(ui.style().button_rounding);
            ui.draw_rounded_rect(bounds, bg, radius);

            // Subtle top highlight (only when not pressed)
            if !is_active {
                let highlight = Color::new(
                    (bg.r + 0.04).min(1.0),
                    (bg.g + 0.04).min(1.0),
                    (bg.b + 0.04).min(1.0),
                    bg.a,
                );
                ui.draw_line(
                    Vec2::new(bounds.min.x() + radius, bounds.min.y() + 0.5),
                    Vec2::new(bounds.max.x() - radius, bounds.min.y() + 0.5),
                    highlight,
                    1.0,
                );
            }

            if let Some(border) = border_color {
                ui.draw_rounded_selection_border(bounds, *border, 1.0, radius);
            }

            let font_size = ui.style().font_size;
            let text_size = ui.measure_text(label, font_size);
            let text_pos = Vec2::new(
                bounds.center().x() - text_size.x() * 0.5,
                bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(
                label,
                text_pos,
                anim_state.apply_to_color(ui.style().button_text),
                font_size,
            );
        }

        ViewDescriptor::Slider {
            label,
            value_id,
            range,
            show_value,
            precision,
        } => {
            let value = state_arena.get::<f32>(*value_id);
            let start = *range.start();
            let end = *range.end();
            let t = if end > start {
                ((value - start) / (end - start)).clamp(0.0, 1.0)
            } else {
                0.0
            };

            // Background
            let bg = ui.style().input_bg;
            let border = ui.style().input_border;
            let radius = ui.style().input_rounding;
            ui.draw_rounded_rect(bounds, bg, radius);
            ui.draw_rounded_selection_border(bounds, border, 1.0, radius);

            // Track
            let track_height = ui.style().slider_track_height;
            let track_bounds = Rect2D::from_center_size(
                Vec2::new(bounds.center().x(), bounds.center().y()),
                Vec2::new(bounds.width(), track_height),
            );
            ui.draw_rounded_rect(track_bounds, ui.style().slider_track, track_height * 0.5);

            // Filled portion
            let fill_width = t * bounds.width();
            if fill_width > 0.0 {
                let fill_bounds =
                    Rect2D::from_origin_size(track_bounds.min, Vec2::new(fill_width, track_height));
                ui.draw_rounded_rect(fill_bounds, ui.style().slider_grab, track_height * 0.5);
            }

            // Grab circle
            let grab_color = if is_active {
                ui.style().slider_grab_active
            } else if is_hovered {
                ui.style().slider_grab_hovered
            } else {
                ui.style().slider_grab
            };
            let grab_center_x = bounds.min.x() + t * bounds.width();
            let grab_center = Vec2::new(grab_center_x, bounds.center().y());
            let base_radius = ui.style().slider_grab_size * 0.5;
            let grab_radius = if is_active {
                base_radius * 1.25
            } else if is_hovered {
                base_radius * 1.15
            } else {
                base_radius
            };

            // Shadow
            ui.draw_circle(
                Vec2::new(grab_center.x(), grab_center.y() + 1.0),
                grab_radius,
                Color::new(0.0, 0.0, 0.0, 0.3),
            );
            // Main grab
            ui.draw_circle(grab_center, grab_radius, grab_color);

            // Label
            let padding = 4.0;
            if label.strip_prefix("##").is_none() && !label.is_empty() {
                let font_size = ui.style().font_size;
                let text_color = ui.style().text_color;
                ui.draw_text(
                    label,
                    Vec2::new(bounds.min.x() + padding, bounds.min.y()),
                    text_color,
                    font_size,
                );
            }

            if *show_value {
                let value_text = format!("{:.1$}", value, precision);
                let font_size = ui.style().font_size;
                let text_size = ui.measure_text(&value_text, font_size);
                let text_pos = Vec2::new(
                    bounds.max.x() - text_size.x() - padding,
                    bounds.center().y() - text_size.y() * 0.5,
                );
                ui.draw_text(&value_text, text_pos, ui.style().text_color, font_size);
            }
        }

        ViewDescriptor::Toggle { label, value_id } => {
            let checked = state_arena.get::<bool>(*value_id);

            // Checkbox box
            let check_size = bounds.height().min(20.0);
            let check_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.center().y() - check_size * 0.5),
                Vec2::new(check_size, check_size),
            );
            let check_rounding = 3.0;

            let bg_color = if checked {
                ui.style().checkbox_check
            } else if is_hovered {
                Color::new(
                    (ui.style().checkbox_bg.r + 0.06).min(1.0),
                    (ui.style().checkbox_bg.g + 0.06).min(1.0),
                    (ui.style().checkbox_bg.b + 0.06).min(1.0),
                    ui.style().checkbox_bg.a,
                )
            } else {
                ui.style().checkbox_bg
            };
            ui.draw_rounded_rect(check_bounds, bg_color, check_rounding);
            if !checked {
                ui.draw_rounded_selection_border(
                    check_bounds,
                    ui.style().checkbox_border,
                    1.0,
                    check_rounding,
                );
            }

            if checked {
                let icon_size = check_size * 0.7;
                ui.draw_icon_centered(ForkAwesome::CHECK, check_bounds, icon_size, Color::WHITE);
            }

            let font_size = ui.style().font_size;
            let text_size = ui.measure_text(label, font_size);
            let label_pos = Vec2::new(
                check_bounds.max.x() + ui.style().item_inner_spacing,
                bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(label, label_pos, ui.style().text_color, font_size);
        }

        ViewDescriptor::TextField {
            placeholder,
            value_id,
            on_submit: _,
        } => {
            let text: String = state_arena.get::<String>(*value_id);

            // Background
            let bg = ui.style().input_bg;
            let border = if is_focused {
                ui.style().input_border_focused
            } else if is_hovered {
                Color::new(
                    (ui.style().input_border.r + 0.1).min(1.0),
                    (ui.style().input_border.g + 0.1).min(1.0),
                    (ui.style().input_border.b + 0.1).min(1.0),
                    ui.style().input_border.a,
                )
            } else {
                ui.style().input_border
            };
            let radius = ui.style().input_rounding;
            ui.draw_rounded_rect(bounds, bg, radius);
            ui.draw_rounded_selection_border(bounds, border, 1.0, radius);

            if is_focused {
                let focus_ring_width = ui.style().focus_ring_width;
                let focus_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        bounds.min.x() - focus_ring_width,
                        bounds.min.y() - focus_ring_width,
                    ),
                    Vec2::new(
                        bounds.width() + focus_ring_width * 2.0,
                        bounds.height() + focus_ring_width * 2.0,
                    ),
                );
                ui.draw_rounded_selection_border(
                    focus_bounds,
                    ui.style().focus_ring_color,
                    focus_ring_width,
                    radius,
                );
            }

            let font_size = ui.style().font_size;
            let padding = ui.style().text_input_padding;
            let text_pos = Vec2::new(
                bounds.min.x() + padding,
                bounds.center().y() - font_size * 0.5,
            );

            let text_bounds = Rect2D::from_origin_size(
                bounds.min,
                Vec2::new(bounds.width() - padding, bounds.height()),
            );
            ui.push_clip(text_bounds);

            if text.is_empty() && !is_focused {
                ui.draw_text(placeholder, text_pos, ui.style().text_hint, font_size);
            } else {
                ui.draw_text(&text, text_pos, ui.style().input_text, font_size);

                if is_focused {
                    let cursor_x = text_pos.x() + ui.measure_text(&text, font_size).x();
                    let blink_on = (ui.time * 2.0 * std::f64::consts::PI).sin() > 0.0;
                    if blink_on {
                        ui.draw_line(
                            Vec2::new(cursor_x, text_pos.y()),
                            Vec2::new(cursor_x, text_pos.y() + font_size),
                            ui.style().input_cursor,
                            ui.style().text_input_cursor_width,
                        );
                    }
                }
            }

            ui.pop_clip();
        }

        ViewDescriptor::ColorPicker { label, value_id } => {
            let is_open = state_arena.get::<bool>(*value_id);
            let bg = if is_open || is_hovered {
                ui.style().combo_hovered
            } else {
                ui.style().combo_bg
            };
            let radius = ui.style().button_rounding;
            ui.draw_rounded_rect(bounds, bg, radius);
            ui.draw_rounded_selection_border(bounds, ui.style().combo_border, 1.0, radius);

            // Color swatch
            let swatch_padding = 4.0;
            let swatch_size = bounds.height() - swatch_padding * 2.0;
            let swatch_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    bounds.min.x() + swatch_padding,
                    bounds.min.y() + swatch_padding,
                ),
                Vec2::new(swatch_size, swatch_size),
            );
            let swatch_color = Color::rgb(0.5, 0.5, 0.5);
            ui.draw_rounded_rect(swatch_bounds, swatch_color, 2.0);
            ui.draw_rounded_selection_border(swatch_bounds, ui.style().border, 1.0, 2.0);

            let font_size = ui.style().font_size;
            if label.strip_prefix("##").is_none() && !label.is_empty() {
                let label_size = ui.measure_text(label, font_size);
                let label_x = swatch_bounds.max.x() + swatch_padding;
                let label_y = bounds.center().y() - label_size.y() * 0.5;
                ui.draw_text(
                    label,
                    Vec2::new(label_x, label_y),
                    ui.style().text_color,
                    font_size,
                );
            }

            let indicator = if is_open { "▲" } else { "▼" };
            let ind_size = ui.measure_text(indicator, font_size);
            let ind_x = bounds.max.x() - ind_size.x() - swatch_padding;
            let label_y = bounds.center().y() - ind_size.y() * 0.5;
            ui.draw_text(
                indicator,
                Vec2::new(ind_x, label_y),
                ui.style().text_disabled,
                font_size,
            );
        }

        // All other variants fall through to the basic (non-interactive) draw
        _ => {
            draw_descriptor(
                descriptor,
                ui,
                bounds,
                state_arena,
                children_bounds,
                interaction,
                anim_state,
            );
        }
    }
}
