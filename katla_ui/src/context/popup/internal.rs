//! Internal popup helpers: position calculation, background drawing, close handling.

use katla_math::{Color, Rect2D, Vec2};

use super::super::UiContext;
use super::{Popup, PopupPosition, PopupStyle};

impl UiContext {
    /// Calculate popup position based on config.
    ///
    /// For `AtCursor`, uses the captured position from when the popup was first opened.
    pub(super) fn calculate_popup_position<'a>(&self, config: &Popup<'a>) -> Vec2 {
        match config.position {
            PopupPosition::AtCursor => {
                // Use captured position if available, otherwise current mouse pos
                self.popup_position.unwrap_or(self.input.mouse_pos)
            }
            PopupPosition::AtPosition(pos) => pos,
            PopupPosition::BelowButton(trigger) => Vec2::new(trigger.min.x(), trigger.max.y()),
            PopupPosition::Fixed(bounds) => bounds.min,
            PopupPosition::Centered { width, height } => Vec2::new(
                (self.screen_size.x() - width) * 0.5,
                (self.screen_size.y() - height) * 0.5,
            ),
        }
    }

    /// Calculate final popup bounds from tracked content.
    pub(super) fn calculate_final_popup_bounds<'a>(
        &self,
        config: &Popup<'a>,
        position: Vec2,
    ) -> Rect2D {
        match config.position {
            PopupPosition::Fixed(bounds) => bounds,
            PopupPosition::Centered { width, height } => {
                Rect2D::from_origin_size(position, Vec2::new(width, height))
            }
            _ => {
                // Auto-size from tracked content
                let content_bounds = self.popup_content_bounds.unwrap_or_else(|| {
                    Rect2D::from_origin_size(
                        position,
                        Vec2::new(self.style.menu_min_width, self.style.menu_item_height),
                    )
                });

                let min_width = self.style.menu_min_width;
                let min_height = self.style.menu_item_height;
                let final_width = content_bounds.width().max(min_width);
                let final_height = content_bounds.height().max(min_height);

                Rect2D::from_origin_size(content_bounds.min, Vec2::new(final_width, final_height))
            }
        }
    }

    /// Draw popup background (shadow + bg + border).
    pub(super) fn draw_popup_background(&mut self, bounds: Rect2D) {
        // Shadow
        let shadow_offset = Vec2::new(4.0, 4.0);
        let shadow_bounds = Rect2D::new(bounds.min + shadow_offset, bounds.max + shadow_offset);
        self.draw_rect(shadow_bounds, self.style.popup_shadow);

        // Background
        self.draw_rounded_rect(bounds, self.style.popup_bg, self.style.popup_rounding);

        // Border
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.popup_border, 1.0);
    }

    /// Handle popup close behavior.
    ///
    /// Returns `true` if the popup should be closed.
    pub(super) fn handle_popup_close<'a>(&mut self, config: &Popup<'a>, bounds: Rect2D) -> bool {
        // Capture mouse when over popup
        if bounds.contains(self.input.mouse_pos) {
            self.input.want_capture_mouse = true;
        }

        // Handle click-outside-to-close
        if config.close_behavior == super::CloseBehavior::ClickOutside
            && self.input.mouse_clicked(crate::input::mouse_button::LEFT)
            && !bounds.contains(self.input.mouse_pos)
        {
            // For dropdowns, don't close if clicking on the trigger button
            // This allows toggling the dropdown by clicking the same button
            if let PopupPosition::BelowButton(trigger) = config.position
                && trigger.contains(self.input.mouse_pos)
            {
                return false;
            }
            return true;
        }

        // Handle Escape-to-close
        if self.input.key_pressed(crate::input::KeyCode::Escape) {
            return true;
        }

        // Capture keyboard for modals
        if config.style == PopupStyle::Modal {
            self.input.want_capture_keyboard = true;
        }

        false
    }
}
