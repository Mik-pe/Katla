//! Combo box widget.

use katla_math::{Color, Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::text::FontId;

use super::super::UiContext;
use super::Popup;

impl UiContext {
    pub(crate) fn combo<F>(
        &mut self,
        id: &str,
        preview: &str,
        bounds: Rect2D,
        open: &mut bool,
        content: F,
    ) where
        F: FnOnce(&mut Self, &mut bool),
    {
        let combo_id = self.make_stable_id(id);

        // Draw combo box trigger
        let hovered = self.update_hover(combo_id, bounds);
        let clicked = self.click_behavior(combo_id, hovered).as_clicked_bool();

        if clicked {
            *open = !*open;
        }

        let bg_color = if *open {
            self.style.combo_bg
        } else if self.active_id == Some(combo_id) || hovered {
            self.style.combo_hovered
        } else {
            self.style.combo_bg
        };

        self.draw_rect(bounds, bg_color);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.combo_border, 1.0);

        // Draw preview text (top-left positioning, centered vertically)
        let text_size = self.measure_text(preview, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + self.style.menu_padding,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(
            preview,
            text_pos,
            self.style.combo_text,
            self.style.font_size,
        );

        // Draw dropdown icon
        let icon_size = self.style.font_size;
        let icon_pos = Vec2::new(
            bounds.max.x() - icon_size - self.style.menu_padding,
            bounds.center().y() - icon_size * 0.5,
        );
        self.draw_icon_aligned(
            ForkAwesome::CARET_DOWN,
            icon_pos,
            icon_size,
            self.style.combo_text,
            FontId::DEFAULT,
        );

        // If open, render popup content using the popup API
        if *open {
            self.popup(
                Popup::new(id).below_button(bounds).menu(),
                open,
                content,
            );
        }
    }
}
