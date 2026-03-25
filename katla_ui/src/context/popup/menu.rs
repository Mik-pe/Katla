//! Menu item widgets for use inside popups.

use katla_math::{Rect2D, Vec2};

use crate::FontSize;
use crate::icons::ForkAwesome;
use crate::input::mouse_button;
use crate::text::FontId;

use super::super::UiContext;

impl UiContext {
    /// Menu item with automatic positioning inside a popup.
    ///
    /// Returns true if clicked. Use inside `context_menu()`, `dropdown()`, or `modal()`.
    /// Items are positioned automatically - no manual bounds needed.
    pub fn menu_item_clicked(&mut self, label: &str) -> bool {
        self.menu_item_clicked_ex(label, None, true, "")
    }

    /// Menu item with icon.
    pub fn menu_item_clicked_with_icon(&mut self, label: &str, icon: char) -> bool {
        self.menu_item_clicked_ex(label, Some(icon), true, "")
    }

    /// Menu item with icon, shortcut hint, and enabled state.
    pub fn menu_item_clicked_with_icon_and_shortcut(
        &mut self,
        label: &str,
        icon: char,
        enabled: bool,
        shortcut: &str,
    ) -> bool {
        self.menu_item_clicked_ex(label, Some(icon), enabled, shortcut)
    }

    /// Menu item with all options.
    fn menu_item_clicked_ex(
        &mut self,
        label: &str,
        icon: Option<char>,
        enabled: bool,
        shortcut: &str,
    ) -> bool {
        let item_height = self.style.menu_item_height;

        let item_bounds =
            Rect2D::from_origin_size(self.popup_cursor, Vec2::new(self.popup_width, item_height));

        // Track for auto-sizing
        self.track_popup_item(item_bounds);

        // Use provided icon or default based on label
        let icon_char = icon.unwrap_or(ForkAwesome::ANGLE_RIGHT);

        // Draw and check click
        let clicked =
            self.draw_popup_item_contents(label, icon_char, enabled, item_bounds, shortcut);

        // Advance cursor
        self.popup_cursor = Vec2::new(self.popup_cursor.x(), self.popup_cursor.y() + item_height);

        clicked
    }

    /// Toggle menu item with automatic positioning.
    ///
    /// Shows checkmark when `checked` is true.
    pub fn toggle_menu_item_clicked(&mut self, label: &str, checked: bool) -> bool {
        let item_height = self.style.menu_item_height;

        let item_bounds =
            Rect2D::from_origin_size(self.popup_cursor, Vec2::new(self.popup_width, item_height));

        // Track for auto-sizing
        self.track_popup_item(item_bounds);

        let hovered = self.is_hovered(item_bounds);

        // Hover background
        if hovered {
            self.draw_rect(item_bounds, self.style.menu_hovered);
        }

        // Checkmark or space
        let icon = if checked { ForkAwesome::CHECK } else { ' ' };
        let text_size = self.scaled_font_size(FontSize::Small);
        let text_y = item_bounds.min.y() + 6.0;

        self.draw_icon_aligned(
            icon,
            Vec2::new(item_bounds.min.x() + 8.0, text_y),
            12.0,
            self.style.text_color,
            FontId::DEFAULT,
        );

        // Label
        self.draw_text(
            label,
            Vec2::new(item_bounds.min.x() + 28.0, text_y),
            self.style.text_color,
            text_size,
        );

        // Click detection
        let clicked = hovered && self.input.mouse_clicked(mouse_button::LEFT);

        // Advance cursor
        self.popup_cursor = Vec2::new(self.popup_cursor.x(), self.popup_cursor.y() + item_height);

        clicked
    }

    /// Menu separator with automatic positioning.
    pub fn menu_separator(&mut self) {
        let separator_height = 8.0;

        let sep_bounds = Rect2D::from_origin_size(
            self.popup_cursor,
            Vec2::new(self.popup_width, separator_height),
        );

        // Track for auto-sizing
        self.track_popup_item(sep_bounds);

        // Draw line
        self.draw_line(
            Vec2::new(sep_bounds.min.x() + 8.0, sep_bounds.center().y()),
            Vec2::new(sep_bounds.max.x() - 8.0, sep_bounds.center().y()),
            self.style.separator,
            1.0,
        );

        // Advance cursor
        self.popup_cursor = Vec2::new(
            self.popup_cursor.x(),
            self.popup_cursor.y() + separator_height,
        );
    }

    /// Internal: draw popup item contents (hover, icon, label, shortcut).
    pub(super) fn draw_popup_item_contents(
        &mut self,
        label: &str,
        icon: char,
        enabled: bool,
        bounds: Rect2D,
        shortcut: &str,
    ) -> bool {
        let hovered = self.is_hovered(bounds);

        // Hover background
        if enabled && hovered {
            self.draw_rect(bounds, self.style.menu_hovered);
        }

        let text_size = self.scaled_font_size(FontSize::Small);
        let text_y = bounds.min.y() + 6.0;

        // Colors
        let icon_color = if enabled {
            self.style.text_color
        } else {
            self.style.text_disabled
        };
        let label_color = if enabled {
            self.style.text_color
        } else {
            self.style.text_disabled
        };

        // Icon
        self.draw_icon_aligned(
            icon,
            Vec2::new(bounds.min.x() + 8.0, text_y),
            12.0,
            icon_color,
            FontId::DEFAULT,
        );

        // Label
        self.draw_text(
            label,
            Vec2::new(bounds.min.x() + 28.0, text_y),
            label_color,
            text_size,
        );

        // Shortcut (right-aligned)
        if !shortcut.is_empty() {
            let shortcut_size = self.measure_text(shortcut, text_size);
            self.draw_text(
                shortcut,
                Vec2::new(bounds.max.x() - shortcut_size.x() - 8.0, text_y),
                self.style.text_disabled,
                text_size,
            );
        }

        // Click detection
        enabled && hovered && self.input.mouse_clicked(mouse_button::LEFT)
    }
}
