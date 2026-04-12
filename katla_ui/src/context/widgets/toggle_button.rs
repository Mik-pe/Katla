//! Internal toggle button widget implementation.
//!
//! This module contains rendering and interaction logic for toggle buttons.
//! These are private implementation details called by the public builder widgets
//! in `crate::widgets`.

use katla_math::Vec2;

use crate::Response;
use crate::icons::ForkAwesome;
use crate::widgets::ToggleButtonParams;

use super::super::UiContext;

impl UiContext {
    /// Draw a toggle button with an optional check icon when enabled.
    ///
    /// Returns a Response. Check `response.clicked` for click.
    /// Colors are passed as parameters to allow theme customization.
    pub(crate) fn toggle_button(&mut self, params: &ToggleButtonParams) -> Response {
        let widget_id = self.generate_id(params.id);
        let hovered = self.update_hover(widget_id, params.bounds);
        let click_result = self.click_interaction(
            widget_id,
            hovered,
            params.bounds,
            super::super::interaction::ClickConfig::POPUP_AWARE,
        );
        let clicked = click_result.is_clicked();
        let active = click_result.is_active();

        // Draw background
        let bg_color = if params.checked {
            params.checked_color
        } else {
            params.unchecked_color
        };
        self.draw_rect(params.bounds, bg_color);

        // Draw check icon and label
        let font_size = self.style.font_size;
        let icon_size = font_size;
        let padding = font_size;
        let text_x = params.bounds.min.x() + padding;
        let text_y = params.bounds.min.y() + 6.0;

        if params.checked {
            let check_icon = ForkAwesome::CHECK;
            self.draw_icon(
                check_icon,
                Vec2::new(text_x, text_y),
                icon_size,
                params.text_color,
            );
            self.draw_text(
                params.label,
                Vec2::new(text_x + icon_size + 4.0, text_y),
                params.text_color,
                font_size,
            );
        } else {
            // Reserve space for alignment
            self.draw_text(
                params.label,
                Vec2::new(text_x + icon_size + 4.0, text_y),
                params.text_color,
                font_size,
            );
        }

        // Use Response builder for consistent construction

        if hovered {
            self.input.set_cursor(crate::input::MouseCursor::Hand);
        }

        Response::interactive(
            clicked,
            hovered,
            active,
            params.bounds,
            &self.input,
            Some(widget_id),
        )
    }
}
