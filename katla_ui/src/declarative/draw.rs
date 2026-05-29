use std::collections::HashSet;

use katla_math::{Color, Rect2D, Vec2};

use crate::context::UiContext;

use super::animation::AnimationState;
use super::descriptor::ViewDescriptor;
use super::state::{StateArena, StateId};
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
            label,
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

            if let Some(label_text) = label {
                let font_size = ui.style().font_size;
                let text_size = ui.measure_text(label_text, font_size);
                let text_pos = Vec2::new(
                    bounds.center().x() - text_size.x() * 0.5,
                    bounds.center().y() - text_size.y() * 0.5,
                );
                ui.draw_text(label_text, text_pos, ui.style().button_text, font_size);
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

        ViewDescriptor::Separator { direction, color } => {
            let line_color = color.unwrap_or(ui.style().separator);
            match direction {
                super::descriptor::SeparatorDirection::Horizontal => {
                    let y = bounds.center().y();
                    ui.draw_line(
                        Vec2::new(bounds.min.x(), y),
                        Vec2::new(bounds.max.x(), y),
                        anim_state.apply_to_color(line_color),
                        1.0,
                    );
                }
                super::descriptor::SeparatorDirection::Vertical => {
                    let x = bounds.center().x();
                    ui.draw_line(
                        Vec2::new(x, bounds.min.y()),
                        Vec2::new(x, bounds.max.y()),
                        anim_state.apply_to_color(line_color),
                        1.0,
                    );
                }
            }
        }

        ViewDescriptor::Icon { icon, size, color } => {
            let font_size = size
                .map(|fs| ui.scaled_font_size(fs))
                .unwrap_or(ui.style().font_size);
            let icon_color = color.unwrap_or(ui.style().text_color);
            let text_size = ui.measure_text(&icon.to_string(), font_size);
            let text_pos = Vec2::new(
                bounds.center().x() - text_size.x() * 0.5,
                bounds.center().y() - text_size.y() * 0.5,
            );
            ui.draw_text(
                &icon.to_string(),
                text_pos,
                anim_state.apply_to_color(icon_color),
                font_size,
            );
        }

        ViewDescriptor::Selectable {
            on_click: _,
            selected,
            child: _,
        } => {
            if *selected {
                ui.draw_rect(bounds, ui.style().selectable_selected);
            } else if is_hovered {
                ui.draw_rect(bounds, ui.style().selectable_hovered);
            }
        }

        ViewDescriptor::Section {
            title,
            expanded_id,
            on_remove,
            ..
        } => {
            let expanded: bool = state_arena.get(*expanded_id);
            let font_size = ui.style().font_size;

            // Header background
            let header_height = font_size + 8.0;
            let header_bounds =
                Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), header_height));
            ui.draw_rect(header_bounds, ui.style().window_title_bg);

            // Chevron
            let chevron = if expanded {
                katla_icons::ForkAwesome::CHEVRON_DOWN.to_string()
            } else {
                katla_icons::ForkAwesome::CHEVRON_RIGHT.to_string()
            };
            let chevron_y = header_bounds.center().y() - font_size * 0.5;
            ui.draw_text(
                &chevron,
                Vec2::new(bounds.min.x() + 4.0, chevron_y),
                ui.style().text_color,
                font_size,
            );

            // Title
            let title_x = bounds.min.x() + font_size + 8.0;
            ui.draw_text(
                title,
                Vec2::new(title_x, chevron_y),
                anim_state.apply_to_color(ui.style().text_color),
                font_size,
            );

            // Remove button (×)
            if on_remove.is_some() {
                let close_x = bounds.max.x() - font_size - 4.0;
                ui.draw_text(
                    "\u{00d7}",
                    Vec2::new(close_x, chevron_y),
                    ui.style().text_disabled,
                    font_size,
                );
            }
        }

        ViewDescriptor::TabBar(desc) => {
            let font_size = ui.style().font_size;
            let selected: usize = state_arena.get(desc.selected_id);
            let tab_count = desc.tabs.len().max(1);
            let tab_width = bounds.width() / tab_count as f32;

            for (i, tab) in desc.tabs.iter().enumerate() {
                let tab_bounds = Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x() + i as f32 * tab_width, bounds.min.y()),
                    Vec2::new(tab_width, bounds.height()),
                );
                let is_selected = i == selected;
                let is_tab_hovered = tab_bounds.contains(ui.mouse_pos());

                let bg = if is_selected {
                    ui.style().selectable_selected
                } else if is_tab_hovered {
                    ui.style().button_hovered
                } else {
                    ui.style().button_normal
                };
                ui.draw_rect(tab_bounds, anim_state.apply_to_color(bg));

                if is_selected {
                    ui.draw_line(
                        Vec2::new(tab_bounds.min.x(), tab_bounds.max.y() - 2.0),
                        Vec2::new(tab_bounds.max.x(), tab_bounds.max.y() - 2.0),
                        anim_state.apply_to_color(ui.style().text_color),
                        2.0,
                    );
                }

                let label_size = ui.measure_text(&tab.label, font_size);
                let text_pos = Vec2::new(
                    tab_bounds.center().x() - label_size.x() * 0.5,
                    tab_bounds.center().y() - label_size.y() * 0.5,
                );
                ui.draw_text(
                    &tab.label,
                    text_pos,
                    anim_state.apply_to_color(ui.style().text_color),
                    font_size,
                );
            }
        }

        ViewDescriptor::Grid(_) => {}

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

        ViewDescriptor::StatusBar(_) => {
            ui.draw_line(
                Vec2::new(bounds.min.x(), bounds.min.y()),
                Vec2::new(bounds.max.x(), bounds.min.y()),
                ui.style().separator,
                1.0,
            );
            ui.draw_rect(bounds, ui.style().window_bg);
        }

        ViewDescriptor::DraggablePanel(desc) => {
            let title_bar_height = 25.0_f32;

            let shadow_offset = Vec2::new(6.0, 6.0);
            let shadow_bounds = Rect2D::new(bounds.min + shadow_offset, bounds.max + shadow_offset);
            ui.draw_rect(shadow_bounds, ui.style().popup_shadow);

            ui.draw_rect(bounds, ui.style().window_bg);
            ui.draw_rect_border(bounds, ui.style().window_bg, ui.style().window_border, 1.0);

            let title_bounds = Rect2D::new(
                bounds.min,
                Vec2::new(bounds.max.x(), bounds.min.y() + title_bar_height),
            );

            let can_drag = is_hovered && mouse_in_rect(title_bounds, ui);
            let title_color = if is_active || can_drag {
                ui.style().window_title_bg_active
            } else {
                ui.style().window_title_bg
            };
            ui.draw_rect(title_bounds, title_color);

            let handle_x = bounds.min.x() + desc.width * 0.5 - 20.0;
            let handle_y = bounds.min.y() + 6.0;
            for i in 0..3 {
                let line_y = handle_y + i as f32 * 3.0;
                ui.draw_line(
                    Vec2::new(handle_x, line_y),
                    Vec2::new(handle_x + 40.0, line_y),
                    ui.style().text_disabled,
                    1.0,
                );
            }

            let font_size = ui.style().font_size;
            let title_pos = Vec2::new(bounds.min.x() + font_size, bounds.min.y() + font_size);
            ui.draw_text(&desc.title, title_pos, ui.style().text_color, font_size);

            let close_size = 24.0;
            let close_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.max.x() - close_size - 6.0, bounds.min.y() + 4.0),
                Vec2::new(close_size, close_size),
            );
            let close_hovered = close_bounds.contains(ui.mouse_pos());
            let close_bg = if close_hovered {
                ui.style().button_hovered
            } else {
                title_color
            };
            ui.draw_rect(close_bounds, close_bg);
            ui.draw_text(
                "\u{00d7}",
                Vec2::new(close_bounds.min.x() + 6.0, close_bounds.min.y() + 2.0),
                ui.style().text_color,
                font_size,
            );
        }

        ViewDescriptor::MenuBar(desc) => {
            ui.draw_rect(bounds, ui.style().menu_bg);
            ui.draw_line(
                Vec2::new(bounds.min.x(), bounds.max.y()),
                Vec2::new(bounds.max.x(), bounds.max.y()),
                ui.style().separator,
                1.0,
            );

            let font_size = ui.style().font_size;
            let item_spacing = ui.style().window_padding;
            let mut x = bounds.min.x() + item_spacing;
            let y_center = bounds.min.y() + (desc.height - font_size) * 0.5;

            for group in &desc.groups {
                let label_size = ui.measure_text(&group.label, font_size);
                let group_bounds = Rect2D::from_origin_size(
                    Vec2::new(x, bounds.min.y()),
                    Vec2::new(label_size.x() + item_spacing * 2.0, desc.height),
                );
                let group_hovered = group_bounds.contains(ui.mouse_pos());
                if group_hovered {
                    ui.draw_rect(group_bounds, ui.style().button_hovered);
                }
                ui.draw_text(
                    &group.label,
                    Vec2::new(x + item_spacing, y_center),
                    ui.style().text_color,
                    font_size,
                );

                let is_open: bool = state_arena.get(group.open_id);
                if is_open {
                    let dropdown_y = group_bounds.max.y();
                    let dropdown_width = 180.0_f32;
                    let entry_height = 28.0_f32;
                    let dropdown_bounds = Rect2D::from_origin_size(
                        Vec2::new(group_bounds.min.x(), dropdown_y),
                        Vec2::new(dropdown_width, group.items.len() as f32 * entry_height),
                    );

                    ui.draw_rect(dropdown_bounds, ui.style().window_bg);
                    ui.draw_rect_border(
                        dropdown_bounds,
                        ui.style().window_bg,
                        ui.style().window_border,
                        1.0,
                    );

                    for (i, entry) in group.items.iter().enumerate() {
                        let entry_bounds = Rect2D::from_origin_size(
                            Vec2::new(
                                dropdown_bounds.min.x(),
                                dropdown_y + i as f32 * entry_height,
                            ),
                            Vec2::new(dropdown_width, entry_height),
                        );

                        let entry_hovered = entry_bounds.contains(ui.mouse_pos());
                        if entry_hovered && !entry.disabled {
                            ui.draw_rect(entry_bounds, ui.style().selectable_hovered);
                        }

                        let text_color = if entry.disabled {
                            ui.style().text_disabled
                        } else {
                            ui.style().text_color
                        };
                        let entry_y = entry_bounds.center().y() - font_size * 0.5;
                        ui.draw_text(
                            &entry.label,
                            Vec2::new(entry_bounds.min.x() + item_spacing, entry_y),
                            text_color,
                            font_size,
                        );
                    }
                }

                x += label_size.x() + item_spacing * 2.0;
            }
        }

        ViewDescriptor::TreeView(desc) => {
            let font_size = ui.style().font_size;
            let row_height = desc.row_height;
            let indent = desc.indent_per_level;
            let item_spacing = ui.style().item_inner_spacing;

            let visible_indices =
                compute_visible_tree_items(&desc.items, state_arena, desc.expanded_id);

            let scroll_offset: f32 = state_arena.get(desc.scroll_id);
            let selected_id: Option<u64> = state_arena.get(desc.selected_id);
            let expanded: HashSet<u64> = state_arena.get(desc.expanded_id);

            let visible_count = visible_indices.len();
            let first_row =
                ((scroll_offset.max(0.0) / row_height).floor() as usize).min(visible_count);
            let last_row = ((scroll_offset + bounds.height()) / row_height).ceil() as usize;
            let last_row = last_row.min(visible_count);

            for vis_idx in first_row..last_row {
                let data_idx = visible_indices[vis_idx];
                let item = &desc.items[data_idx];
                let item_y = bounds.min.y() + vis_idx as f32 * row_height - scroll_offset;
                let item_bounds = Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x(), item_y),
                    Vec2::new(bounds.width(), row_height),
                );

                let is_selected = selected_id == Some(item.id);
                let row_hovered =
                    bounds.contains(ui.mouse_pos()) && item_bounds.contains(ui.mouse_pos());

                if is_selected {
                    ui.draw_rect(item_bounds, ui.style().selectable_selected);
                } else if row_hovered {
                    ui.draw_rect(item_bounds, ui.style().selectable_hovered);
                }

                for depth_level in 0..item.depth {
                    let guide_x = bounds.min.x() + depth_level as f32 * indent + item_spacing;
                    ui.draw_line(
                        Vec2::new(guide_x, item_bounds.min.y()),
                        Vec2::new(guide_x, item_bounds.max.y()),
                        ui.style().border,
                        1.0,
                    );
                }

                let arrow_x = bounds.min.x() + item.depth as f32 * indent + item_spacing;
                let arrow_y = item_bounds.center().y() - font_size * 0.5;

                if item.has_children {
                    let arrow_char = if expanded.contains(&item.id) {
                        katla_icons::ForkAwesome::CHEVRON_DOWN.to_string()
                    } else {
                        katla_icons::ForkAwesome::CHEVRON_RIGHT.to_string()
                    };
                    ui.draw_text(
                        &arrow_char,
                        Vec2::new(arrow_x, arrow_y),
                        ui.style().text_disabled,
                        font_size,
                    );
                }

                let content_x = arrow_x + indent;
                let label_y = item_bounds.center().y() - font_size * 0.5;
                ui.draw_text(
                    &item.label,
                    Vec2::new(content_x, label_y),
                    ui.style().text_color,
                    font_size,
                );
            }
        }

        ViewDescriptor::Modal(desc) => {
            let is_open: bool = state_arena.get(desc.open_id);
            if !is_open {
                return;
            }

            let screen_size = ui.screen_size();
            let screen_bounds = Rect2D::new(Vec2::new(0.0, 0.0), screen_size);
            ui.draw_rect(screen_bounds, ui.style().popup_shadow);

            let cx = (screen_size.x() - desc.width) * 0.5;
            let cy = (screen_size.y() - desc.height) * 0.5;
            let modal_bounds =
                Rect2D::from_origin_size(Vec2::new(cx, cy), Vec2::new(desc.width, desc.height));

            ui.draw_rect_border(
                modal_bounds,
                ui.style().window_bg,
                ui.style().window_border,
                1.0,
            );
        }

        ViewDescriptor::ContextMenu(desc) => {
            let is_open: bool = state_arena.get(desc.open_id);
            if !is_open {
                return;
            }

            let font_size = ui.style().font_size;
            let item_height = 28.0_f32;
            let item_spacing = ui.style().item_inner_spacing;
            let max_label_width: f32 = desc
                .items
                .iter()
                .map(|item| ui.measure_text(&item.label, font_size).x())
                .fold(0.0_f32, f32::max);
            let menu_width = max_label_width + item_spacing * 4.0;
            let menu_height = desc.items.len() as f32 * item_height;

            let menu_bounds =
                Rect2D::from_origin_size(bounds.min, Vec2::new(menu_width, menu_height));

            ui.draw_rect(menu_bounds, ui.style().window_bg);
            ui.draw_rect_border(
                menu_bounds,
                ui.style().window_bg,
                ui.style().window_border,
                1.0,
            );

            for (i, entry) in desc.items.iter().enumerate() {
                let entry_bounds = Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x(), bounds.min.y() + i as f32 * item_height),
                    Vec2::new(menu_width, item_height),
                );

                let entry_hovered = entry_bounds.contains(ui.mouse_pos());
                if entry_hovered && !entry.disabled {
                    ui.draw_rect(entry_bounds, ui.style().selectable_hovered);
                }

                let text_color = if entry.disabled {
                    ui.style().text_disabled
                } else {
                    ui.style().text_color
                };
                let label_y = entry_bounds.center().y() - font_size * 0.5;
                ui.draw_text(
                    &entry.label,
                    Vec2::new(entry_bounds.min.x() + item_spacing * 2.0, label_y),
                    text_color,
                    font_size,
                );
            }
        }

        ViewDescriptor::Custom(draw_fn) => {
            draw_fn(ui, bounds);
        }
    }
}

fn mouse_in_rect(rect: Rect2D, ui: &UiContext) -> bool {
    rect.contains(ui.mouse_pos())
}

fn compute_visible_tree_items(
    items: &[super::descriptor::TreeItem],
    state_arena: &StateArena,
    expanded_id: StateId,
) -> Vec<usize> {
    let expanded: HashSet<u64> = state_arena.get(expanded_id);
    let mut visible = Vec::new();
    let mut parent_stack: Vec<u64> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        while parent_stack.len() > item.depth as usize {
            parent_stack.pop();
        }

        if item.depth == 0 {
            visible.push(i);
        } else if let Some(&parent_id) = parent_stack.last() {
            if expanded.contains(&parent_id) {
                visible.push(i);
            } else {
                continue;
            }
        }

        if item.has_children {
            parent_stack.push(item.id);
        }
    }

    visible
}
