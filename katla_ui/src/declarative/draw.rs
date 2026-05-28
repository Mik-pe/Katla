use katla_math::{Color, Rect2D, Vec2};

use crate::context::UiContext;

use super::animation::AnimationState;
use super::descriptor::ViewDescriptor;
use super::state::StateArena;
use super::tree::InteractionState;

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

    match descriptor {
        ViewDescriptor::Empty => {}

        ViewDescriptor::TransitionContainer { .. } => {}

        ViewDescriptor::Text {
            content,
            color,
            font_size,
        } => {
            let text_color = color.unwrap_or(ui.style().text_color);
            let size = font_size
                .map(|fs| ui.scaled_font_size(fs))
                .unwrap_or(ui.style().font_size);
            ui.draw_text(
                content,
                bounds.min,
                anim_state.apply_to_color(text_color),
                size,
            );
        }

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

        ViewDescriptor::LabeledSlider {
            label,
            value_id,
            range,
            label_width,
            show_value,
            precision,
        } => {
            let value: f32 = state_arena.get(*value_id);
            let t = (value - range.start()) / (range.end() - range.start());

            let font_size = ui.style().font_size;
            let text_color = anim_state.apply_to_color(ui.style().text_color);

            // Label text on the left
            let label_size = ui.measure_text(label, font_size);
            let label_y = bounds.center().y() - label_size.y() * 0.5;
            ui.draw_text(
                label,
                Vec2::new(bounds.min.x(), label_y),
                text_color,
                font_size,
            );

            // Track region starts after label_width
            let track_x = bounds.min.x() + *label_width;

            // Value text width if showing
            let value_text_width = if *show_value {
                let value_text = format!("{:.1$}", value, precision);
                let size = ui.measure_text(&value_text, font_size);
                size.x() + 8.0
            } else {
                0.0
            };

            let track_end = bounds.max.x() - value_text_width;
            let track_width = (track_end - track_x).max(0.0);
            let track_height = ui.style().slider_track_height;
            let track_center_y = bounds.center().y();
            let track_bounds = Rect2D::from_center_size(
                Vec2::new(track_x + track_width * 0.5, track_center_y),
                Vec2::new(track_width, track_height),
            );

            // Track
            ui.draw_rounded_rect(track_bounds, ui.style().slider_track, track_height * 0.5);

            // Fill
            let fill_width = t * track_width;
            if fill_width > 0.0 {
                let fill_bounds =
                    Rect2D::from_origin_size(track_bounds.min, Vec2::new(fill_width, track_height));
                ui.draw_rounded_rect(
                    fill_bounds,
                    anim_state.apply_to_color(ui.style().slider_grab),
                    track_height * 0.5,
                );
            }

            // Grab handle
            let grab_color = if is_active {
                ui.style().slider_grab_active
            } else if is_hovered {
                ui.style().slider_grab_hovered
            } else {
                ui.style().slider_grab
            };
            let grab_center_x = track_x + t * track_width;
            let grab_center = Vec2::new(grab_center_x, track_center_y);
            let base_radius = ui.style().slider_grab_size * 0.5;
            let grab_radius = if is_active {
                base_radius * 1.25
            } else if is_hovered {
                base_radius * 1.15
            } else {
                base_radius
            };
            ui.draw_circle(
                Vec2::new(grab_center.x(), grab_center.y() + 1.0),
                grab_radius,
                Color::new(0.0, 0.0, 0.0, 0.3),
            );
            ui.draw_circle(grab_center, grab_radius, grab_color);

            // Value text on the right
            if *show_value {
                let value_text = format!("{:.1$}", value, precision);
                let text_size = ui.measure_text(&value_text, font_size);
                let value_x = bounds.max.x() - text_size.x();
                let value_y = bounds.center().y() - text_size.y() * 0.5;
                ui.draw_text(
                    &value_text,
                    Vec2::new(value_x, value_y),
                    text_color,
                    font_size,
                );
            }
        }

        ViewDescriptor::Vec3Slider {
            label: _,
            value_ids,
            range,
            axis_labels,
            axis_colors,
            precision,
        } => {
            let font_size = ui.style().font_size;
            let text_color = anim_state.apply_to_color(ui.style().text_color);
            let row_height = bounds.height() / 3.0;
            let axis_label_width = 20.0;
            let value_text_width = 40.0;

            for i in 0..3 {
                let row_y = bounds.min.y() + row_height * i as f32;
                let row_bounds = Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x(), row_y),
                    Vec2::new(bounds.width(), row_height),
                );

                // Axis label
                let axis_label = &axis_labels[i];
                let axis_color = axis_colors[i];
                let axis_label_size = ui.measure_text(axis_label, font_size);
                let axis_label_y = row_bounds.center().y() - axis_label_size.y() * 0.5;
                ui.draw_text(
                    axis_label,
                    Vec2::new(row_bounds.min.x(), axis_label_y),
                    axis_color,
                    font_size,
                );

                // Track
                let value: f32 = state_arena.get(value_ids[i]);
                let t = (value - range.start()) / (range.end() - range.start());
                let track_x = row_bounds.min.x() + axis_label_width;
                let track_end = row_bounds.max.x() - value_text_width;
                let track_width = (track_end - track_x).max(0.0);
                let track_height = ui.style().slider_track_height;
                let track_center_y = row_bounds.center().y();

                let track_bounds = Rect2D::from_center_size(
                    Vec2::new(track_x + track_width * 0.5, track_center_y),
                    Vec2::new(track_width, track_height),
                );
                ui.draw_rounded_rect(track_bounds, ui.style().slider_track, track_height * 0.5);

                // Fill
                let fill_width = t * track_width;
                if fill_width > 0.0 {
                    let fill_bounds = Rect2D::from_origin_size(
                        track_bounds.min,
                        Vec2::new(fill_width, track_height),
                    );
                    ui.draw_rounded_rect(fill_bounds, ui.style().slider_grab, track_height * 0.5);
                }

                // Grab
                let grab_center_x = track_x + t * track_width;
                let grab_center = Vec2::new(grab_center_x, track_center_y);
                let grab_radius = ui.style().slider_grab_size * 0.5;
                ui.draw_circle(grab_center, grab_radius, ui.style().slider_grab);

                // Value text
                let value_text = format!("{:.1$}", value, precision);
                let text_size = ui.measure_text(&value_text, font_size);
                let value_x = row_bounds.max.x() - text_size.x();
                let value_y = row_bounds.center().y() - text_size.y() * 0.5;
                ui.draw_text(
                    &value_text,
                    Vec2::new(value_x, value_y),
                    text_color,
                    font_size,
                );
            }
        }

        ViewDescriptor::ImageButton {
            icon,
            enabled,
            fill_color,
            on_click: _,
        } => {
            let bg = if !enabled {
                fill_color.unwrap_or(ui.style().button_normal)
            } else if is_active {
                fill_color.unwrap_or(ui.style().button_active)
            } else if is_hovered {
                fill_color.unwrap_or(ui.style().button_hovered)
            } else {
                fill_color.unwrap_or(ui.style().button_normal)
            };
            let bg = anim_state.apply_to_color(bg);
            let radius = anim_state.apply_to_corner_radius(ui.style().button_rounding);
            ui.draw_rounded_rect(bounds, bg, radius);

            let font_size = ui.style().icon_button_size * 0.6;
            let text_size = ui.measure_text(&icon.to_string(), font_size);
            let text_pos = Vec2::new(
                bounds.center().x() - text_size.x() * 0.5,
                bounds.center().y() - text_size.y() * 0.5,
            );
            let icon_color = if *enabled {
                ui.style().button_text
            } else {
                ui.style().text_hint
            };
            ui.draw_text(
                &icon.to_string(),
                text_pos,
                anim_state.apply_to_color(icon_color),
                font_size,
            );
        }

        ViewDescriptor::RadioButton {
            value_id,
            index,
            label,
        } => {
            let selected: usize = state_arena.get(*value_id);
            let is_selected = selected == *index;

            let bg = if is_selected {
                ui.style().selectable_selected
            } else if is_hovered {
                ui.style().button_hovered
            } else {
                ui.style().button_normal
            };
            let bg = anim_state.apply_to_color(bg);
            let radius = anim_state.apply_to_corner_radius(ui.style().button_rounding);
            ui.draw_rounded_rect(bounds, bg, radius);

            // Radio indicator circle
            let indicator_radius = bounds.height() * 0.15;
            let indicator_center =
                Vec2::new(bounds.min.x() + indicator_radius * 2.0, bounds.center().y());
            ui.draw_circle(
                indicator_center,
                indicator_radius,
                anim_state.apply_to_color(ui.style().window_border),
            );
            if is_selected {
                ui.draw_circle(
                    indicator_center,
                    indicator_radius * 0.6,
                    anim_state.apply_to_color(ui.style().text_color),
                );
            }

            // Label
            let font_size = ui.style().font_size;
            let text_size = ui.measure_text(label, font_size);
            let text_pos = Vec2::new(
                bounds.min.x() + indicator_radius * 4.0,
                bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(
                label,
                text_pos,
                anim_state.apply_to_color(ui.style().button_text),
                font_size,
            );
        }

        ViewDescriptor::PropertyRow { label, value } => {
            let font_size = ui.style().font_size;
            let text_color = ui.style().text_color;
            let label_size = ui.measure_text(label, font_size);
            let label_y = bounds.center().y() - label_size.y() * 0.5;
            ui.draw_text(
                label,
                Vec2::new(bounds.min.x(), label_y),
                anim_state.apply_to_color(text_color),
                font_size,
            );
            let value_size = ui.measure_text(value, font_size);
            let value_x = bounds.max.x() - value_size.x();
            let value_y = bounds.center().y() - value_size.y() * 0.5;
            ui.draw_text(
                value,
                Vec2::new(value_x, value_y),
                anim_state.apply_to_color(ui.style().text_hint),
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
            let value: f32 = state_arena.get(*value_id);
            let t = (value - range.start()) / (range.end() - range.start());

            // Track
            let track_height = ui.style().slider_track_height;
            let track_bounds =
                Rect2D::from_center_size(bounds.center(), Vec2::new(bounds.width(), track_height));
            ui.draw_rounded_rect(track_bounds, ui.style().slider_track, track_height * 0.5);

            // Fill
            let fill_width = t * bounds.width();
            if fill_width > 0.0 {
                let fill_bounds =
                    Rect2D::from_origin_size(track_bounds.min, Vec2::new(fill_width, track_height));
                ui.draw_rounded_rect(fill_bounds, ui.style().slider_grab, track_height * 0.5);
            }

            // Grab handle
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
            ui.draw_circle(
                Vec2::new(grab_center.x(), grab_center.y() + 1.0),
                grab_radius,
                Color::new(0.0, 0.0, 0.0, 0.3),
            );
            ui.draw_circle(grab_center, grab_radius, grab_color);

            // Label text on the left
            if !label.is_empty() {
                let label_size = ui.measure_text(label, ui.style().font_size);
                ui.draw_text(
                    label,
                    bounds.min,
                    anim_state.apply_to_color(ui.style().text_color),
                    ui.style().font_size,
                );
                let _ = label_size;
            }

            // Value text centered
            if *show_value {
                let value_text = format!("{:.1$}", value, precision);
                let text_size = ui.measure_text(&value_text, ui.style().font_size);
                let text_pos = Vec2::new(
                    bounds.center().x() - text_size.x() * 0.5,
                    bounds.center().y() - text_size.y() * 0.5,
                );
                ui.draw_text(
                    &value_text,
                    text_pos,
                    anim_state.apply_to_color(ui.style().text_color),
                    ui.style().font_size,
                );
            }
        }

        ViewDescriptor::Toggle { label, value_id } => {
            let checked: bool = state_arena.get(*value_id);
            let bg_color = if checked {
                ui.style().selectable_selected
            } else {
                ui.style().button_normal
            };
            let bg_color = anim_state.apply_to_color(bg_color);
            let radius = anim_state.apply_to_corner_radius(ui.style().button_rounding);
            ui.draw_rounded_rect(bounds, bg_color, radius);

            if is_hovered || is_active {
                let hover = ui.style().button_hovered;
                ui.draw_rounded_rect(bounds, anim_state.apply_to_color(hover), radius);
            }

            // Indicator
            let indicator_size = bounds.height() * 0.5;
            let indicator_center = if checked {
                Vec2::new(bounds.max.x() - indicator_size, bounds.center().y())
            } else {
                Vec2::new(bounds.min.x() + indicator_size, bounds.center().y())
            };
            ui.draw_circle(
                indicator_center,
                indicator_size * 0.5,
                anim_state.apply_to_color(ui.style().text_color),
            );

            // Label
            if !label.is_empty() {
                let font_size = ui.style().font_size;
                let text_size = ui.measure_text(label, font_size);
                let text_pos = Vec2::new(
                    bounds.min.x() + ui.style().item_inner_spacing,
                    bounds.center().y() - text_size.y() * 0.5,
                );
                ui.draw_text(
                    label,
                    text_pos,
                    anim_state.apply_to_color(ui.style().text_color),
                    font_size,
                );
            }
        }

        ViewDescriptor::TextField {
            placeholder,
            value_id,
            on_submit: _,
        } => {
            let text: String = state_arena.get(*value_id);
            let is_focused = interaction.focused_id == Some(view_id);

            // Background
            let border_color = if is_focused {
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

            ui.draw_rounded_rect(bounds, ui.style().input_bg, ui.style().input_rounding);
            ui.draw_rounded_selection_border(bounds, border_color, 1.0, ui.style().input_rounding);

            // Focus ring
            if is_focused {
                let fw = ui.style().focus_ring_width;
                let focus_bounds = Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x() - fw, bounds.min.y() - fw),
                    Vec2::new(bounds.width() + fw * 2.0, bounds.height() + fw * 2.0),
                );
                ui.draw_rounded_selection_border(
                    focus_bounds,
                    ui.style().focus_ring_color,
                    fw,
                    ui.style().input_rounding,
                );
            }

            // Text content
            let padding = 4.0;
            let text_bounds = Rect2D::from_origin_size(
                bounds.min,
                Vec2::new(bounds.width() - padding, bounds.height()),
            )
            .contract(padding);
            ui.push_clip(text_bounds);

            let font_size = ui.style().font_size;
            let text_size = ui.measure_text(&text, font_size);
            let text_pos = Vec2::new(
                bounds.min.x() + padding,
                bounds.center().y() - text_size.y() * 0.5,
            );

            if is_focused {
                // Draw cursor blink
                let blink_on =
                    ui.time == 0.0 || ((ui.time * 2.0 * std::f64::consts::PI).sin() > 0.0);
                if blink_on {
                    let cursor_x = text_pos.x() + text_size.x();
                    ui.draw_line(
                        Vec2::new(cursor_x, text_pos.y()),
                        Vec2::new(cursor_x, text_pos.y() + text_size.y()),
                        ui.style().input_cursor,
                        ui.style().text_input_cursor_width,
                    );
                }
            }

            if text.is_empty() && !is_focused {
                ui.draw_text(placeholder, text_pos, ui.style().text_hint, font_size);
            } else {
                ui.draw_text(&text, text_pos, ui.style().input_text, font_size);
            }

            ui.pop_clip();
        }

        ViewDescriptor::Progress {
            value,
            range,
            fill_color,
        } => {
            let t = (value - range.start()) / (range.end() - range.start());
            let track_color = ui.style().slider_track;
            let bar_color = fill_color.unwrap_or(ui.style().slider_grab);

            ui.draw_rounded_rect(bounds, track_color, bounds.height() * 0.5);
            let fill_width = t * bounds.width();
            if fill_width > 0.0 {
                let fill_bounds =
                    Rect2D::from_origin_size(bounds.min, Vec2::new(fill_width, bounds.height()));
                ui.draw_rounded_rect(
                    fill_bounds,
                    anim_state.apply_to_color(bar_color),
                    bounds.height() * 0.5,
                );
            }
        }

        ViewDescriptor::ColorPicker { label, value_id } => {
            let color: Color = state_arena.get(*value_id);

            // Draw color swatch
            let swatch_size = bounds.height() - 4.0;
            let swatch_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x() + 2.0, bounds.min.y() + 2.0),
                Vec2::new(swatch_size, swatch_size),
            );
            ui.draw_rounded_rect(swatch_bounds, color, 2.0);

            // Label
            if !label.is_empty() {
                let font_size = ui.style().font_size;
                let text_pos = Vec2::new(
                    swatch_bounds.max.x() + ui.style().item_inner_spacing,
                    bounds.center().y() - font_size * 0.5,
                );
                ui.draw_text(
                    label,
                    text_pos,
                    anim_state.apply_to_color(ui.style().text_color),
                    font_size,
                );
            }
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
    }
}
