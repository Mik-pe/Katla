//! UI widgets module.
//!
//! Contains all widget implementations organized by category:
//! - `basic` - label, button, checkbox, slider, text input
//! - `selectable` - selectable items, toggle buttons
//! - `container` - windows, headers, child regions
//! - `utility` - progress bar, tooltip, image
//! - `graph` - real-time data visualization
//! - `scroll_area` - scrollable container

mod basic;
mod container;
mod graph;
mod scroll_area;
mod selectable;
mod utility;

pub use scroll_area::{ScrollArea, ScrollAreaState};

use katla_math::Rect2D;

use crate::input::mouse_button;

use super::UiContext;

impl UiContext {
    // -------------------------------------------------------------------------
    // Widget Behavior Helpers
    // -------------------------------------------------------------------------

    /// Check if a widget is being hovered.
    ///
    /// This uses the imgui/egui approach: widgets at lower Z levels can only
    /// be hovered if the cursor is NOT inside a higher-level popup's bounds.
    /// This allows clicking outside popups to work correctly while still
    /// blocking hover for widgets covered by the popup.
    pub fn is_hovered(&self, bounds: Rect2D) -> bool {
        // Block hover if a popup consumed the click this frame (prevents click-through)
        if self.popup_consume_click {
            return false;
        }
        // If a popup is open and cursor is inside popup bounds,
        // block hover for widgets at lower Z levels
        if let Some(popup_bounds) = self.popup_bounds {
            if popup_bounds.contains(self.input.mouse_pos) && self.z_index < super::z_index::POPUP {
                return false;
            }
        }
        self.input.is_hovered(bounds) && self.active_id.is_none()
    }

    /// Update hover state for a widget.
    pub fn update_hover(&mut self, id: super::WidgetId, bounds: Rect2D) -> bool {
        let hovered = self.is_hovered(bounds);
        if hovered {
            self.hovered_id = Some(id);
            self.input.want_capture_mouse = true;
        }
        hovered
    }

    /// Handle button behavior (returns true if clicked).
    pub fn button_behavior(&mut self, id: super::WidgetId, bounds: Rect2D) -> bool {
        let hovered = self.update_hover(id, bounds);

        if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.active_id = Some(id);
        }

        let clicked = self.active_id == Some(id) && self.input.mouse_released[mouse_button::LEFT];

        // Only clear active_id if we're the active widget
        if clicked {
            self.active_id = None;
        }

        clicked
    }

    // -------------------------------------------------------------------------
    // Convenience Widget Methods (Auto-Layout)
    // -------------------------------------------------------------------------

    /// Add a button at the current cursor position.
    ///
    /// This is a convenience method that creates a button with automatic
    /// positioning. After adding, the cursor advances vertically.
    ///
    /// # Example
    /// ```ignore
    /// if ui.button_auto("Save Changes").clicked {
    ///     save_changes();
    /// }
    /// ```
    pub fn button_auto(&mut self, text: &str) -> crate::Response {
        use crate::widgets::Button;
        let bounds = Rect2D::from_origin_size(
            self.cursor(),
            katla_math::Vec2::new(100.0, self.style.button_height_medium),
        );
        let response = self.add(Button::new(text).bounds(bounds));
        // Advance cursor
        self.cursor = katla_math::Vec2::new(
            self.cursor.x(),
            self.cursor.y() + self.style.button_height_medium + self.style.item_spacing,
        );
        response
    }

    /// Add a button with custom width at the current cursor position.
    ///
    /// # Example
    /// ```ignore
    /// ui.button_auto_wide("Cancel", 120.0);
    /// ```
    pub fn button_auto_wide(&mut self, text: &str, width: f32) -> crate::Response {
        use crate::widgets::Button;
        let bounds = Rect2D::from_origin_size(
            self.cursor(),
            katla_math::Vec2::new(width, self.style.button_height_medium),
        );
        let response = self.add(Button::new(text).bounds(bounds));
        // Advance cursor
        self.cursor = katla_math::Vec2::new(
            self.cursor.x(),
            self.cursor.y() + self.style.button_height_medium + self.style.item_spacing,
        );
        response
    }

    /// Add a label at the current cursor position.
    ///
    /// The label is automatically sized to fit its text.
    ///
    /// # Example
    /// ```ignore
    /// ui.label_auto("Hello, World!");
    /// ```
    pub fn label_auto(&mut self, text: &str) -> crate::Response {
        let text_size = self.measure_text(text, self.style.font_size);
        let bounds = Rect2D::from_origin_size(self.cursor(), text_size);
        self.draw_text(text, self.cursor(), self.style.text_color, self.style.font_size);
        let response = crate::Response::new(bounds);
        // Advance cursor
        self.cursor = katla_math::Vec2::new(
            self.cursor.x(),
            self.cursor.y() + text_size.y() + self.style.item_spacing,
        );
        response
    }

    /// Add a label with custom color at the current cursor position.
    ///
    /// # Example
    /// ```ignore
    /// ui.label_auto_colored("Error:", katla_math::Color::RED);
    /// ```
    pub fn label_auto_colored(&mut self, text: &str, color: katla_math::Color) -> crate::Response {
        let text_size = self.measure_text(text, self.style.font_size);
        let bounds = Rect2D::from_origin_size(self.cursor(), text_size);
        self.draw_text(text, self.cursor(), color, self.style.font_size);
        let response = crate::Response::new(bounds);
        // Advance cursor
        self.cursor = katla_math::Vec2::new(
            self.cursor.x(),
            self.cursor.y() + text_size.y() + self.style.item_spacing,
        );
        response
    }
}
