//! Basic UI widgets: button, checkbox, slider, text input.
//!
//! These are internal implementations used by the builder widgets in `crate::widgets`.

use katla_math::{Color, Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::input::{mouse_button, KeyCode};
use crate::Response;

use super::super::UiContext;

impl UiContext {
    /// Draw a button (internal - use `widgets::Button` instead).
    pub(crate) fn button(&mut self, id: &str, text: &str, bounds: Rect2D) -> Response {
        let widget_id = self.generate_id(id);

        let hovered = self.update_hover(widget_id, bounds);
        let active = self.active_id == Some(widget_id);

        // Handle click
        let clicked = if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.active_id = Some(widget_id);
            false
        } else if active && self.input.mouse_released[mouse_button::LEFT] {
            self.active_id = None;
            hovered
        } else {
            false
        };

        // Determine colors based on state
        let bg_color = if active {
            self.style.button_active
        } else if hovered {
            self.style.button_hovered
        } else {
            self.style.button_normal
        };

        // Draw button background
        self.draw_rect(bounds, bg_color);

        // Draw button text (centered, top-left positioning)
        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(text, text_pos, self.style.button_text, self.style.font_size);

        // Check for double-click (on click release)
        let double_clicked = clicked && self.input.mouse_double_clicked(mouse_button::LEFT);

        // Track drag delta when active
        let drag_delta = if active {
            self.input.mouse_delta
        } else {
            Vec2::new(0.0, 0.0)
        };

        Response {
            clicked,
            hovered,
            active,
            changed: clicked,
            bounds,
            drag_delta,
            double_clicked,
        }
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
        let active = self.active_id == Some(widget_id);

        let clicked = if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.active_id = Some(widget_id);
            false
        } else if active && self.input.mouse_released[mouse_button::LEFT] {
            self.active_id = None;
            hovered
        } else {
            false
        };

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

        // Draw label (top-left positioning, vertically centered)
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

        // Check for double-click
        let double_clicked = clicked && self.input.mouse_double_clicked(mouse_button::LEFT);

        // Track drag delta when active
        let drag_delta = if active {
            self.input.mouse_delta
        } else {
            Vec2::new(0.0, 0.0)
        };

        Response {
            clicked,
            hovered,
            active,
            changed: clicked,
            bounds,
            drag_delta,
            double_clicked,
        }
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

        // Track drag delta when active
        let drag_delta = if active {
            self.input.mouse_delta
        } else {
            Vec2::new(0.0, 0.0)
        };

        Response {
            clicked: false,
            hovered,
            active,
            changed,
            bounds,
            drag_delta,
            double_clicked: false,
        }
    }

    /// Draw a separator line.
    pub fn separator(&mut self, bounds: Rect2D) {
        let y = bounds.center().y();
        self.draw_line(
            Vec2::new(bounds.min.x(), y),
            Vec2::new(bounds.max.x(), y),
            self.style.separator,
            1.0,
        );
    }

    /// Draw a text input field (internal - use `widgets::TextInput` instead).
    pub(crate) fn text_input(&mut self, id: &str, text: &mut String, bounds: Rect2D) -> Response {
        let widget_id = self.generate_id(id);
        let hovered = self.update_hover(widget_id, bounds);

        // Focus on click
        if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.input.focused_id = Some(widget_id);
        }

        let focused = self.input.focused_id == Some(widget_id);
        let mut changed = false;

        // Handle keyboard input when focused
        if focused {
            self.input.want_capture_keyboard = true;

            // Process character input
            for &c in &self.input.characters {
                if c == '\x08' {
                    // Backspace
                    if !text.is_empty() {
                        text.pop();
                        changed = true;
                    }
                } else if c >= ' ' && text.len() < self.style.text_input_max_length {
                    // Printable character
                    text.push(c);
                    changed = true;
                }
            }

            // Handle special keys
            if self.input.key_pressed(KeyCode::Enter) {
                // Could trigger a callback here
            }
            if self.input.key_pressed(KeyCode::Escape) {
                self.input.focused_id = None;
            }
        }

        // Draw background
        self.draw_rect(bounds, self.style.input_bg);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.input_border, 1.0);

        // Draw text with clipping (top-left positioning, centered vertically)
        let padding = 4.0;
        let text_bounds = bounds.contract(padding);
        self.push_clip(text_bounds);

        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + padding,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(text, text_pos, self.style.input_text, self.style.font_size);

        // Draw cursor when focused
        if focused {
            let cursor_x = text_pos.x() + self.measure_text(text, self.style.font_size).x();
            self.draw_line(
                Vec2::new(cursor_x, text_pos.y()),
                Vec2::new(cursor_x, text_pos.y() + self.style.font_size),
                self.style.input_cursor,
                1.0,
            );
        }

        self.pop_clip();

        Response {
            clicked: false,
            hovered,
            active: focused,
            changed,
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }

    /// Draw a multiline text area (internal - use `widgets::TextArea` instead).
    pub(crate) fn text_area(&mut self, id: &str, text: &mut String, bounds: Rect2D) -> Response {
        let widget_id = self.generate_id(id);
        let hovered = self.update_hover(widget_id, bounds);

        if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.input.focused_id = Some(widget_id);
        }

        let focused = self.input.focused_id == Some(widget_id);
        let mut changed = false;

        if focused {
            self.input.want_capture_keyboard = true;

            for &c in &self.input.characters {
                if c == '\x08' {
                    if !text.is_empty() {
                        text.pop();
                        changed = true;
                    }
                } else if (c >= ' ' || c == '\n') && text.len() < self.style.text_area_max_length {
                    text.push(c);
                    changed = true;
                }
            }
        }

        // Draw background
        self.draw_rect(bounds, self.style.input_bg);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.input_border, 1.0);

        // Draw text with clipping
        let padding = 4.0;
        self.push_clip(bounds.contract(padding));

        let mut y = bounds.min.y() + padding;
        for line in text.split('\n') {
            if y + self.style.font_size > bounds.min.y() + padding && y < bounds.max.y() - padding {
                self.draw_text(
                    line,
                    Vec2::new(bounds.min.x() + padding, y),
                    self.style.input_text,
                    self.style.font_size,
                );
            }
            y += self.style.font_size + 2.0;
        }

        self.pop_clip();

        Response {
            clicked: false,
            hovered,
            active: focused,
            changed,
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }
}
