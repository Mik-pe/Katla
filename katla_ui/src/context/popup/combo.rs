//! Combo box widget.

use katla_math::{Color, Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::text::FontId;

use super::super::state::WidgetState;
use super::super::{z_index, UiContext};

impl UiContext {
    /// Begin a combo box (dropdown with selection).
    ///
    /// Returns true if the combo is open and should have items drawn.
    /// Call `end_combo()` after adding selectable items.
    /// The `preview` text is shown in the closed combo box.
    pub fn begin_combo(&mut self, id: &str, preview: &str, bounds: Rect2D) -> bool {
        let combo_id = self.generate_id(id);

        // Get or initialize open state
        let is_open = self
            .storage
            .get(&combo_id)
            .map(|s| matches!(s, WidgetState::DropdownOpen(true)))
            .unwrap_or(false);

        // Draw combo box
        let hovered = self.update_hover(combo_id, bounds);

        // Toggle on click
        if self.button_behavior(combo_id, bounds) {
            let new_open = !is_open;
            self.storage
                .insert(combo_id, WidgetState::DropdownOpen(new_open));
            if new_open {
                self.popup_id = Some(combo_id);
                self.popup_opened_this_frame = true;
            } else {
                self.popup_id = None;
                self.popup_bounds = None;
            }
        }

        // Determine combo colors
        let bg_color = if is_open {
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

        // If open, prepare popup area
        if is_open {
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
            self.push_clip_absolute(popup_bounds); // Absolute clip - render outside parent
            self.push_id(id);

            return true;
        }

        false
    }

    /// End a combo box.
    pub fn end_combo(&mut self) {
        self.pop_clip();
        self.pop_id();
        self.pop_z_index();
    }
}
