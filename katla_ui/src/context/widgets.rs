//! UI widgets module.
//!
//! Contains all widget implementations organized by category:
//! - `basic` - label, button, checkbox, slider, text input
//! - `selectable` - selectable items, toggle buttons
//! - `container` - windows, headers, child regions
//! - `utility` - progress bar, tooltip, image
//! - `graph` - real-time data visualization

mod basic;
mod container;
mod graph;
mod selectable;
mod utility;

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

        let clicked = self.active_id == Some(id)
            && self.input.mouse_released[mouse_button::LEFT];

        // Only clear active_id if we're the active widget
        if clicked {
            self.active_id = None;
        }

        clicked
    }
}
