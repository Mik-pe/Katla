//! Selectable widgets: selectable items and toggle buttons.

use katla_math::{Color, Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::input::mouse_button;
use crate::Response;

use super::super::UiContext;

impl UiContext {
    /// Draw a selectable item with selection state.
    ///
    /// Returns a Response. Check `response.clicked` for click.
    /// The `selected` parameter controls whether the item is highlighted as selected.
    pub fn selectable(&mut self, id: &str, label: &str, selected: bool, bounds: Rect2D) -> Response {
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

        // Determine colors based on state
        let bg_color = if selected {
            self.style.selectable_selected
        } else if active {
            self.style.menu_active
        } else if hovered {
            self.style.selectable_hovered
        } else {
            Color::TRANSPARENT
        };

        // Draw background
        if bg_color != Color::TRANSPARENT {
            self.draw_rect(bounds, bg_color);
        }

        // Draw label (top-left positioning, centered vertically)
        let text_size = self.measure_text(label, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + self.style.menu_padding,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(label, text_pos, self.style.text_color, self.style.font_size);

        Response {
            clicked,
            hovered,
            active,
            changed: clicked,
            bounds,
        }
    }

    /// Draw a toggle button with an optional check icon when enabled.
    ///
    /// Returns a Response. Check `response.clicked` for click.
    /// Colors are passed as parameters to allow theme customization.
    #[allow(clippy::too_many_arguments)]
    pub fn toggle_button(
        &mut self,
        id: &str,
        label: &str,
        checked: bool,
        bounds: Rect2D,
        checked_color: Color,
        unchecked_color: Color,
        text_color: Color,
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

        // Draw background
        let bg_color = if checked { checked_color } else { unchecked_color };
        self.draw_rect(bounds, bg_color);

        // Draw check icon and label
        let font_size = self.style.font_size;
        let icon_size = font_size;
        let padding = font_size;
        let text_x = bounds.min.x() + padding;
        let text_y = bounds.min.y() + 6.0;

        if checked {
            let check_icon = ForkAwesome::CHECK;
            self.draw_icon(check_icon, Vec2::new(text_x, text_y), icon_size, text_color);
            self.draw_text(label, Vec2::new(text_x + icon_size + 4.0, text_y), text_color, font_size);
        } else {
            // Reserve space for alignment
            self.draw_text(label, Vec2::new(text_x + icon_size + 4.0, text_y), text_color, font_size);
        }

        Response {
            clicked,
            hovered,
            active,
            changed: clicked,
            bounds,
        }
    }
}
