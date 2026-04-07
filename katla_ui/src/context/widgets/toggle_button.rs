//! Internal toggle button widget implementation.
//!
//! This module contains rendering and interaction logic for toggle buttons.
//! These are private implementation details called by the public builder widgets
//! in `crate::widgets`.

use katla_math::{Color, Rect2D, Vec2};

use crate::Response;
use crate::icons::ForkAwesome;

use super::super::UiContext;

impl UiContext {
    /// Draw a toggle button with an optional check icon when enabled.
    ///
    /// Returns a Response. Check `response.clicked` for click.
    /// Colors are passed as parameters to allow theme customization.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn toggle_button(
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

        // Handle click using consolidated helper
        let clicked = self.click_behavior(widget_id, hovered).is_clicked();

        // Draw background
        let bg_color = if checked {
            checked_color
        } else {
            unchecked_color
        };
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
            self.draw_text(
                label,
                Vec2::new(text_x + icon_size + 4.0, text_y),
                text_color,
                font_size,
            );
        } else {
            // Reserve space for alignment
            self.draw_text(
                label,
                Vec2::new(text_x + icon_size + 4.0, text_y),
                text_color,
                font_size,
            );
        }

        // Use Response builder for consistent construction
        Response::interactive(clicked, hovered, active, bounds, &self.input)
    }
}
