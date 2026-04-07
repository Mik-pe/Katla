//! Internal widget implementations.
//!
//! This module contains the actual rendering and interaction logic for basic widgets.
//! These are private implementation details called by the public builder widgets
//! in `crate::widgets`.

use katla_math::{Color, Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::input::{KeyCode, mouse_button};
use crate::response::Response;

use super::super::UiContext;
use super::super::drawing::center_in_bounds;

impl UiContext {
    /// Draw a button with optional custom background colors and border.
    pub(crate) fn button_with_colors(
        &mut self,
        id: &str,
        text: &str,
        bounds: Rect2D,
        fill_color: Option<Color>,
        hover_color: Option<Color>,
        border_color: Option<Color>,
    ) -> Response {
        let widget_id = self.generate_id(id);

        let hovered = self.update_hover(widget_id, bounds);
        let click_result = self.click_interaction(
            widget_id,
            hovered,
            bounds,
            super::super::interaction::ClickConfig::POPUP_BYPASS,
        );
        let clicked = click_result.is_clicked();
        let active = click_result.is_active();

        // Determine background color based on state
        let bg_color = if active {
            hover_color.unwrap_or(self.style.button_active)
        } else if hovered {
            hover_color.unwrap_or(self.style.button_hovered)
        } else {
            fill_color.unwrap_or(self.style.button_normal)
        };

        // Draw button background
        self.draw_rect(bounds, bg_color);

        // Draw border if specified
        if let Some(border_color) = border_color {
            self.draw_selection_border(bounds, border_color, 1.0);
        }

        // Draw button text
        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = center_in_bounds(bounds, text_size);
        self.draw_text(text, text_pos, self.style.text_color, self.style.font_size);

        Response::interactive(clicked, hovered, active, bounds, &self.input)
    }
    pub(crate) fn image_button(
        &mut self,
        id: &str,
        icon: char,
        bounds: Rect2D,
        enabled: bool,
    ) -> Response {
        let widget_id = self.generate_id(id);

        let hovered = self.update_hover(widget_id, bounds) && enabled;
        let click_result = self.click_interaction(
            widget_id,
            hovered,
            bounds,
            super::super::interaction::ClickConfig::POPUP_AWARE,
        );
        let clicked = click_result.is_clicked() && enabled;
        let active = click_result.is_active() && enabled;

        // Determine colors based on state
        let bg_color = if !enabled {
            self.style.button_normal * 0.5
        } else if active {
            self.style.button_active
        } else if hovered {
            self.style.button_hovered
        } else {
            Color::TRANSPARENT
        };

        // Draw button background
        self.draw_rect(bounds, bg_color);

        // Determine icon color based on state
        let icon_color = if !enabled {
            self.style.button_text * 0.5
        } else if hovered {
            self.style.button_text
        } else {
            self.style.button_text * 0.8
        };

        // Draw icon centered
        let icon_size = bounds.height().min(bounds.width()) * 0.6;
        self.draw_icon_centered(icon, bounds, icon_size, icon_color);

        let mut response = Response::interactive(clicked, hovered, active, bounds, &self.input);
        if !enabled {
            response.clicked = false;
            response.changed = false;
            response.double_clicked = false;
        }
        response
    }

    /// Draw a checkbox (internal - use `widgets::Checkbox` instead).
    pub(crate) fn checkbox(
        &mut self,
        id: &str,
        label: &str,
        checked: &mut bool,
        bounds: Rect2D,
    ) -> Response {
        let widget_id = self.generate_id(id);

        let hovered = self.update_hover(widget_id, bounds);
        let click_result = self.click_interaction(
            widget_id,
            hovered,
            bounds,
            super::super::interaction::ClickConfig::POPUP_AWARE,
        );
        let clicked = click_result.is_clicked();
        let active = click_result.is_active();

        if clicked {
            *checked = !*checked;
        }

        // Draw checkbox background
        let check_size = bounds.height().min(20.0);
        let check_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.min.x(), bounds.center().y() - check_size * 0.5),
            Vec2::new(check_size, check_size),
        );

        let bg_color = if *checked {
            self.style.checkbox_check
        } else {
            self.style.checkbox_bg
        };
        self.draw_rect_border(check_bounds, bg_color, self.style.checkbox_border, 1.0);

        // Draw check icon if checked
        if *checked {
            let icon_size = check_size * 0.7;
            self.draw_icon_centered(ForkAwesome::CHECK, check_bounds, icon_size, Color::WHITE);
        }

        // Draw label
        let text_size = self.measure_text(label, self.style.font_size);
        let label_pos = Vec2::new(
            check_bounds.max.x() + 8.0,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(
            label,
            label_pos,
            self.style.text_color,
            self.style.font_size,
        );

        Response::interactive(clicked, hovered, active, bounds, &self.input)
    }

    /// Draw a slider (internal - use `widgets::Slider` instead).
    pub(crate) fn slider(
        &mut self,
        id: &str,
        value: &mut f32,
        min: f32,
        max: f32,
        bounds: Rect2D,
    ) -> Response {
        let widget_id = self.generate_id(id);

        let hovered = self.update_hover(widget_id, bounds);
        let active = self.active_id == Some(widget_id);

        // Handle dragging
        let mut changed = false;

        if active {
            if self.input.mouse_down[mouse_button::LEFT] {
                let t =
                    ((self.input.mouse_pos.x() - bounds.min.x()) / bounds.width()).clamp(0.0, 1.0);
                let new_value = min + t * (max - min);
                if (new_value - *value).abs() > 0.0001 {
                    *value = new_value;
                    changed = true;
                }
            } else {
                self.active_id = None;
            }
        } else if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.active_id = Some(widget_id);
        }

        // Draw track
        let track_height = 4.0;
        let track_bounds = Rect2D::from_center_size(
            Vec2::new(bounds.center().x(), bounds.center().y()),
            Vec2::new(bounds.width(), track_height),
        );
        self.draw_rect(track_bounds, self.style.slider_track);

        // Draw grab
        let t = (*value - min) / (max - min);
        let grab_size = 12.0;
        let grab_pos = bounds.min.x() + t * (bounds.width() - grab_size);
        let grab_bounds = Rect2D::from_origin_size(
            Vec2::new(grab_pos, bounds.center().y() - grab_size * 0.5),
            Vec2::new(grab_size, grab_size),
        );
        let grab_color = if active {
            self.style.slider_grab_active
        } else if hovered {
            self.style.slider_grab_hovered
        } else {
            self.style.slider_grab
        };
        self.draw_rect(grab_bounds, grab_color);

        let mut response = Response::interactive(false, hovered, active, bounds, &self.input);
        response.changed = changed;
        response
    }

    /// Draw a text input field (internal - use `widgets::TextInput` instead).
    pub(crate) fn text_input(
        &mut self,
        id: &str,
        text: &mut String,
        bounds: Rect2D,
        placeholder: Option<&str>,
        show_clear: bool,
    ) -> Response {
        let widget_id = self.generate_id(id);
        let hovered = self.update_hover(widget_id, bounds);

        // Clear button bounds (right side)
        let clear_size = bounds.height();
        let clear_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.max.x() - clear_size, bounds.min.y()),
            Vec2::new(clear_size, clear_size),
        );
        let clear_hovered = show_clear && !text.is_empty() && self.input.is_hovered(clear_bounds);
        let clear_clicked = clear_hovered && self.input.mouse_pressed[mouse_button::LEFT];

        // Focus on click (but not on clear button)
        if hovered && !clear_hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.focused_id = Some(widget_id);
        }

        let focused = self.focused_id == Some(widget_id);
        let mut changed = false;

        // Handle clear button
        if clear_clicked {
            text.clear();
            changed = true;
            self.focused_id = Some(widget_id);
        }

        // Handle keyboard input when focused
        if focused {
            self.input.want_capture_keyboard = true;

            // Handle backspace
            if self.input.key_pressed(KeyCode::Backspace) && !text.is_empty() {
                text.pop();
                changed = true;
            }

            // Handle character input
            for &c in &self.input.characters {
                if c >= ' ' && text.len() < self.style.text_input_max_length {
                    text.push(c);
                    changed = true;
                }
                if changed {
                    self.last_input_time = self.time;
                }
            }

            if self.input.key_pressed(KeyCode::Escape) {
                self.focused_id = None;
            }
        }

        // Draw background
        self.draw_rect(bounds, self.style.input_bg);

        // Focused highlight border
        let border_color = if focused {
            self.style.input_border_focused
        } else {
            self.style.input_border
        };
        self.draw_rect_border(bounds, Color::TRANSPARENT, border_color, 1.0);

        // Text area (shrink right if clear button is shown and text is non-empty)
        let padding = 4.0;
        let text_area_width = if show_clear && !text.is_empty() {
            bounds.width() - clear_size - padding
        } else {
            bounds.width() - padding
        };
        let text_bounds =
            Rect2D::from_origin_size(bounds.min, Vec2::new(text_area_width, bounds.height()))
                .contract(padding);
        self.push_clip(text_bounds);

        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + padding,
            bounds.center().y() - text_size.y() * 0.5,
        );

        // Draw placeholder or text
        if text.is_empty() && !focused {
            if let Some(placeholder_text) = placeholder {
                self.draw_text(
                    placeholder_text,
                    text_pos,
                    self.style.text_hint,
                    self.style.font_size,
                );
            }
        } else {
            self.draw_text(text, text_pos, self.style.input_text, self.style.font_size);
        }

        self.pop_clip();

        // Draw cursor when focused (with blink and grace period after typing)
        if focused {
            let grace_period = 0.8;
            let time_since_input = self.time - self.last_input_time;
            let blink_on = self.time == 0.0
                || time_since_input < grace_period
                || ((self.time * 2.0 * std::f64::consts::PI).sin() > 0.0);
            if blink_on {
                let cursor_x = text_pos.x() + self.measure_text(text, self.style.font_size).x();
                self.draw_line(
                    Vec2::new(cursor_x, text_pos.y()),
                    Vec2::new(cursor_x, text_pos.y() + text_size.y()),
                    self.style.input_cursor,
                    self.style.text_input_cursor_width,
                );
            }
        }

        // Draw clear button
        if show_clear && !text.is_empty() {
            let icon_color = if clear_hovered {
                self.style.text_color
            } else {
                self.style.text_disabled
            };
            self.draw_icon_centered(
                crate::icons::ForkAwesome::TIMES,
                clear_bounds,
                clear_size * 0.6,
                icon_color,
            );
        }

        // Set text cursor when hovered
        if hovered {
            self.input.set_cursor(crate::input::MouseCursor::Text);
        }

        let mut response = Response::interactive(false, hovered, focused, bounds, &self.input);
        response.changed = changed;
        response
    }

    /// Draw a radio button (internal - use `widgets::RadioButton` instead).
    pub(crate) fn radio_button(
        &mut self,
        id: &str,
        value: &mut usize,
        index: usize,
        label: &str,
        bounds: Rect2D,
    ) -> Response {
        let is_selected = *value == index;
        let widget_id = self.generate_id(id);
        let hovered = self.update_hover(widget_id, bounds);
        let active = self.active_id == Some(widget_id);

        // Draw radio circle as a rectangle border + fill (simplified)
        let center_x = bounds.min.x() + 10.0;
        let center_y = bounds.center().y();
        let radius = 8.0;

        // Outer circle (border rect)
        let outer_bounds = Rect2D::from_origin_size(
            Vec2::new(center_x - radius, center_y - radius),
            Vec2::new(radius * 2.0, radius * 2.0),
        );
        self.draw_rect_border(
            outer_bounds,
            Color::TRANSPARENT,
            if is_selected {
                self.style.checkbox_check
            } else if hovered {
                self.style.text_color
            } else {
                self.style.checkbox_border
            },
            1.0,
        );

        // Inner circle (filled when selected)
        if is_selected {
            let inner_radius = radius * 0.5;
            let inner_bounds = Rect2D::from_origin_size(
                Vec2::new(center_x - inner_radius, center_y - inner_radius),
                Vec2::new(inner_radius * 2.0, inner_radius * 2.0),
            );
            self.draw_rect(inner_bounds, self.style.checkbox_check);
        }

        // Label
        let label_pos = Vec2::new(center_x + radius + 8.0, bounds.min.y());
        self.draw_text(
            label,
            label_pos,
            self.style.text_color,
            self.style.font_size,
        );

        let clicked = self
            .click_interaction(
                widget_id,
                hovered,
                bounds,
                super::super::interaction::ClickConfig::POPUP_AWARE,
            )
            .is_clicked();

        let mut response = Response::interactive(clicked, hovered, active, bounds, &self.input);
        response.changed = clicked && !is_selected;

        if response.changed {
            *value = index;
        }

        response
    }
}
