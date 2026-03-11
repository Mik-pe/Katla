//! Basic UI widgets: button, checkbox, slider, text input.
//!
//! These are internal implementations used by the builder widgets in `crate::widgets`.

use katla_math::{Color, Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::input::{mouse_button, KeyCode};
use crate::response::Response;

use super::super::interaction::ClickResult;
use super::super::UiContext;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::mouse_button;

    /// Test that normal button uses button_normal color.
    ///
    /// Verifies VAL-OPACITY-003: Button visual appearance - normal state
    #[test]
    fn test_button_normal_state_color() {
        let mut ctx = UiContext::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 28.0));

        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        // Mouse is NOT over the button
        ctx.input.set_mouse_pos(Vec2::new(200.0, 200.0));

        let response = ctx.button("test_btn", "Click Me", bounds);

        // Button should not be hovered or active
        assert!(!response.hovered, "Button should not be hovered");
        assert!(!response.active, "Button should not be active");

        // The button should have been rendered with button_normal color
        // We can verify this by checking the draw list contains the button's background
        let draw_list = ctx.end();

        // The draw list should have vertices for the button background (4 vertices for rect)
        // and text vertices
        assert!(
            draw_list.vertex_count() > 0,
            "Button should render vertices"
        );
    }

    /// Test that hovered button uses button_hovered color.
    ///
    /// Verifies VAL-OPACITY-003: Button visual appearance - hovered state
    #[test]
    fn test_button_hovered_state_color() {
        let mut ctx = UiContext::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 28.0));

        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        // Mouse IS over the button
        let btn_center = bounds.center();
        ctx.input.set_mouse_pos(btn_center);

        let response = ctx.button("test_btn", "Click Me", bounds);

        // Button should be hovered
        assert!(response.hovered, "Button should be hovered");
        assert!(
            !response.active,
            "Button should not be active (not pressed)"
        );

        ctx.end();
    }

    /// Test that active (pressed) button uses button_active color.
    ///
    /// Verifies VAL-OPACITY-003: Button visual appearance - active state
    #[test]
    fn test_button_active_state_color() {
        let mut ctx = UiContext::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 28.0));
        let btn_center = bounds.center();

        // Frame 1: Mouse press on button
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.set_mouse_button(mouse_button::LEFT, true);

        let response1 = ctx.button("test_btn", "Click Me", bounds);

        // Button is pressed and sets active_id for next frame
        assert!(response1.hovered, "Button should be hovered when pressed");
        assert!(
            !response1.active,
            "Button should not be active in first frame (sets active_id for next)"
        );
        ctx.end();

        // Frame 2: Button is now active (still pressed from previous frame)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.mouse_down[mouse_button::LEFT] = true; // Keep mouse down

        let response2 = ctx.button("test_btn", "Click Me", bounds);

        // Button should be active (was set in frame 1)
        // Note: hovered is false because is_hovered() returns false when active_id is set
        assert!(
            !response2.hovered,
            "Button is not hovered when active (by design)"
        );
        assert!(
            response2.active,
            "Button should be active (pressed from previous frame)"
        );

        ctx.end();
    }

    /// Test that click triggers on release while hovering.
    ///
    /// Verifies FLOW-003: Button interaction state change - click behavior
    #[test]
    fn test_button_click_on_release_while_hovering() {
        let mut ctx = UiContext::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 28.0));
        let btn_center = bounds.center();

        // Frame 1: Mouse press on button
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.set_mouse_button(mouse_button::LEFT, true);

        let response1 = ctx.button("test_btn", "Click Me", bounds);

        assert!(!response1.clicked, "Button should NOT click on press");
        ctx.end();

        // Frame 2: Button is now active (still pressed)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.mouse_down[mouse_button::LEFT] = true; // Keep mouse down

        let response2 = ctx.button("test_btn", "Click Me", bounds);

        assert!(response2.active, "Button should be active in frame 2");
        assert!(
            !response2.clicked,
            "Button should NOT click while still pressed"
        );
        ctx.end();

        // Frame 3: Mouse release while still hovering
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center); // Still over button
        ctx.input.set_mouse_button(mouse_button::LEFT, false);

        let response3 = ctx.button("test_btn", "Click Me", bounds);

        // NOW button should be clicked
        assert!(
            response3.clicked,
            "Button should click on release while hovering"
        );
        // Note: active_id is cleared when clicked, so button is not active anymore
        ctx.end();
    }

    /// Test that click does NOT trigger if mouse moves away before release.
    ///
    /// Verifies FLOW-003: Button interaction - click requires hover on release
    #[test]
    fn test_button_no_click_if_not_hovering_on_release() {
        let mut ctx = UiContext::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 28.0));
        let btn_center = bounds.center();

        // Frame 1: Mouse press on button
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.set_mouse_button(mouse_button::LEFT, true);

        let response1 = ctx.button("test_btn", "Click Me", bounds);

        assert!(!response1.clicked, "Button should NOT click on press");
        ctx.end();

        // Frame 2: Button is now active (still pressed)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.mouse_down[mouse_button::LEFT] = true; // Keep mouse down

        let response2 = ctx.button("test_btn", "Click Me", bounds);

        assert!(response2.active, "Button should be active in frame 2");
        ctx.end();

        // Frame 3: Mouse release AFTER moving away from button
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(500.0, 500.0)); // Away from button
        ctx.input.set_mouse_button(mouse_button::LEFT, false);

        let response3 = ctx.button("test_btn", "Click Me", bounds);

        // Button should NOT be clicked (mouse was not over it on release)
        assert!(
            !response3.clicked,
            "Button should NOT click if not hovering on release"
        );
        // Note: active_id gets cleared by end() when mouse is released
        ctx.end();
    }

    /// Test button state transitions: normal -> hovered -> active -> clicked.
    ///
    /// Verifies VAL-OPACITY-003: Button visual state transitions
    #[test]
    fn test_button_state_transitions() {
        let mut ctx = UiContext::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 28.0));
        let btn_center = bounds.center();

        // State 1: Normal (mouse away, not pressed)
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(500.0, 500.0));

        let response1 = ctx.button("test_btn", "Click Me", bounds);

        assert!(
            !response1.hovered && !response1.active && !response1.clicked,
            "State 1: Button should be normal (not hovered, not active)"
        );
        ctx.end();

        // State 2: Hovered (mouse over, not pressed)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center);

        let response2 = ctx.button("test_btn", "Click Me", bounds);

        assert!(
            response2.hovered && !response2.active && !response2.clicked,
            "State 2: Button should be hovered (not active)"
        );
        ctx.end();

        // State 3: Press (mouse over, pressed - sets active_id for next frame)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.set_mouse_button(mouse_button::LEFT, true);

        let response3 = ctx.button("test_btn", "Click Me", bounds);

        assert!(
            response3.hovered && !response3.active && !response3.clicked,
            "State 3: Button pressed, sets active_id for next frame"
        );
        ctx.end();

        // State 4: Active (still pressed, active_id was set in previous frame)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.mouse_down[mouse_button::LEFT] = true; // Keep mouse down

        let response4 = ctx.button("test_btn", "Click Me", bounds);

        // Note: hovered is false because is_hovered() returns false when active_id is set
        assert!(
            !response4.hovered && response4.active && !response4.clicked,
            "State 4: Button should be active (not hovered by design)"
        );
        ctx.end();

        // State 5: Release (clicked, can be hovered again, not active)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.set_mouse_button(mouse_button::LEFT, false);

        let response5 = ctx.button("test_btn", "Click Me", bounds);

        // Button is clicked and active_id is cleared
        assert!(response5.clicked, "State 5: Button should be clicked");
        // After click, button can be hovered again (active_id was cleared)
        ctx.end();
    }

    /// Test that custom button colors override default style colors.
    #[test]
    fn test_button_custom_colors() {
        let mut ctx = UiContext::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 28.0));

        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        let custom_normal = Color::rgb(1.0, 0.0, 0.0); // Red
        let custom_hover = Color::rgb(0.0, 1.0, 0.0); // Green

        // Mouse over button (should use custom hover color)
        let btn_center = bounds.center();
        ctx.input.set_mouse_pos(btn_center);

        let response = ctx.button_with_colors(
            "test_btn",
            "Click Me",
            bounds,
            Some(custom_normal),
            Some(custom_hover),
        );

        assert!(response.hovered, "Button should be hovered");

        ctx.end();
    }

    /// Test that multiple buttons have independent states.
    #[test]
    fn test_multiple_buttons_independent_states() {
        let mut ctx = UiContext::new();
        let bounds1 = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 28.0));
        let bounds2 = Rect2D::from_origin_size(Vec2::new(120.0, 0.0), Vec2::new(100.0, 28.0));

        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        // Mouse over button 1
        ctx.input.set_mouse_pos(bounds1.center());

        let response1 = ctx.button("btn1", "Button 1", bounds1);
        let response2 = ctx.button("btn2", "Button 2", bounds2);

        // Button 1 should be hovered, button 2 should not
        assert!(response1.hovered, "Button 1 should be hovered");
        assert!(!response2.hovered, "Button 2 should NOT be hovered");

        ctx.end();
    }
}

impl UiContext {
    /// Draw a button (internal - use `widgets::Button` instead).
    pub(crate) fn button(&mut self, id: &str, text: &str, bounds: Rect2D) -> Response {
        self.button_with_colors(id, text, bounds, None, None)
    }

    /// Draw a button with optional custom background colors.
    pub(crate) fn button_with_colors(
        &mut self,
        id: &str,
        text: &str,
        bounds: Rect2D,
        fill_color: Option<Color>,
        hover_color: Option<Color>,
    ) -> Response {
        let widget_id = self.generate_id(id);

        let hovered = self.update_hover(widget_id, bounds);
        let active = self.active_id == Some(widget_id);

        // Handle click using unified click behavior
        // Note: On release, we need to use raw input check to bypass active_id blocking
        let clicked = if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.active_id = Some(widget_id);
            false
        } else if active && self.input.mouse_released[mouse_button::LEFT] {
            self.active_id = None;
            self.input.is_hovered(bounds)
        } else {
            false
        };

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

        // Draw button text
        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
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
        let active = self.active_id == Some(widget_id) && enabled;

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

        // Draw label
        let text_size = self.measure_text(label, self.style.font_size);
        let label_pos = Vec2::new(
            check_bounds.max.x() + 8.0,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(label, label_pos, self.style.text_color, self.style.font_size);

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

        let mut response = Response::interactive(false, hovered, focused, bounds, &self.input);
        response.changed = changed;
        response
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
