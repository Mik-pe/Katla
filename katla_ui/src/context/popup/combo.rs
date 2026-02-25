//! Combo box widget.

use katla_math::{Color, Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::text::FontId;

use super::super::{z_index, UiContext};

impl UiContext {
    /// Combo box with selection preview.
    ///
    /// The `preview` text is shown in the closed combo box.
    /// When open, the closure is called to render the selectable items.
    ///
    /// # Example
    /// ```ignore
    /// let mut combo_open = false;
    /// let mut selected = 0;
    /// let options = ["Option A", "Option B", "Option C"];
    ///
    /// ui.combo("my_combo", options[selected], bounds, &mut combo_open, |ui, open| {
    ///     for (i, opt) in options.iter().enumerate() {
    ///         if ui.menu_item_clicked(opt) {
    ///             selected = i;
    ///             *open = false;
    ///         }
    ///     }
    /// });
    /// ```
    pub fn combo<F>(&mut self, id: &str, preview: &str, bounds: Rect2D, open: &mut bool, content: F)
    where
        F: FnOnce(&mut Self, &mut bool),
    {
        // Use stable ID for combo - consistent across frames
        let combo_id = self.make_stable_id(id);

        // Draw combo box trigger
        let hovered = self.update_hover(combo_id, bounds);

        // Toggle on click
        // Note: On release, we use self.input.is_hovered() directly instead of self.is_hovered()
        // because self.is_hovered() returns false when active_id is set (which it is during press).
        let clicked = if hovered && self.input.mouse_pressed[crate::input::mouse_button::LEFT] {
            self.active_id = Some(combo_id);
            false
        } else if self.active_id == Some(combo_id)
            && self.input.mouse_released[crate::input::mouse_button::LEFT]
        {
            self.active_id = None;
            // Check if mouse is still over combo using raw input check (bypasses active_id block)
            self.input.is_hovered(bounds)
        } else {
            false
        };

        if clicked {
            *open = !*open;
        }

        // Determine combo colors
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
        let icon = ForkAwesome::CARET_DOWN;
        let icon_size = self.style.font_size;
        let icon_pos = Vec2::new(
            bounds.max.x() - icon_size - self.style.menu_padding,
            bounds.center().y() - icon_size * 0.5,
        );
        self.draw_icon_aligned(
            icon,
            icon_pos,
            icon_size,
            self.style.combo_text,
            FontId::DEFAULT,
        );

        // If open, render popup content
        if *open {
            // Mark popup as open for input blocking
            let was_just_opened = self.popup_id != Some(combo_id);
            self.popup_id = Some(combo_id);
            if was_just_opened {
                self.popup_opened_this_frame = true;
            }

            // Switch to popup Z-index
            self.push_z_index(z_index::POPUP);

            let popup_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.max.y()),
                Vec2::new(
                    bounds.width().max(self.style.menu_min_width),
                    400.0, // Generous height for menu items
                ),
            );

            // Draw popup background with shadow
            let shadow_offset = Vec2::new(4.0, 4.0);
            let shadow_bounds = Rect2D::new(
                popup_bounds.min + shadow_offset,
                popup_bounds.max + shadow_offset,
            );
            self.draw_rect(shadow_bounds, self.style.popup_shadow);
            self.draw_rect(popup_bounds, self.style.popup_bg);
            self.draw_rect_border(
                popup_bounds,
                Color::TRANSPARENT,
                self.style.popup_border,
                1.0,
            );

            self.popup_bounds = Some(popup_bounds);
            self.push_clip_absolute(popup_bounds);
            self.push_id(id);

            // Run content closure
            content(self, open);

            // If closure closed the popup, clear state
            if !*open {
                self.popup_id = None;
                self.popup_bounds = None;
            }

            // Clean up
            self.pop_clip();
            self.pop_id();
            self.pop_z_index();
        }
    }
}
