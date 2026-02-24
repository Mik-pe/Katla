//! Popup and menu widgets.
//!
//! Context menus, dropdowns, modal dialogs, and popup containers.
//!
//! Module structure:
//! - `types` - PopupPosition, PopupStyle, CloseBehavior, Popup builder
//! - `api` - popup(), context_menu(), dropdown(), modal(), menu_bar_dropdown()
//! - `menu` - menu_item_clicked*, toggle_menu_item_clicked, menu_separator
//! - `combo` - begin_combo, end_combo
//! - `internal` - position calculation, background drawing, close handling

mod api;
mod combo;
mod internal;
mod menu;
mod types;

use katla_math::{Rect2D, Vec2};

use crate::input::mouse_button;

use super::state::WidgetState;
use super::UiContext;

// Re-export public types
pub use types::{CloseBehavior, Popup, PopupPosition, PopupStyle};

impl UiContext {
    // -------------------------------------------------------------------------
    // Popup State Management
    // -------------------------------------------------------------------------

    /// Check if a popup is currently open (built-in or custom).
    #[inline]
    pub fn is_popup_open(&self) -> bool {
        self.popup_id.is_some() || self.popup_bounds.is_some()
    }

    /// Register custom popup bounds for input blocking.
    pub fn set_custom_popup_bounds(&mut self, bounds: Rect2D) {
        self.popup_bounds = Some(bounds);
    }

    /// Clear custom popup bounds.
    pub fn clear_custom_popup_bounds(&mut self) {
        // Only clear if there's no popup_id (i.e., it was a custom popup)
        if self.popup_id.is_none() {
            self.popup_bounds = None;
        }
    }

    /// Block input for the current popup.
    pub fn block_input_for_popup(&mut self, popup_bounds: Rect2D) {
        // Register the bounds for click-outside detection
        self.set_custom_popup_bounds(popup_bounds);

        // Capture mouse when hovering over popup
        if popup_bounds.contains(self.input.mouse_pos) {
            self.input.want_capture_mouse = true;
        }
    }

    /// Check if mouse is over any registered popup bounds.
    pub fn is_mouse_over_popup(&self) -> bool {
        self.popup_bounds
            .map(|bounds| bounds.contains(self.input.mouse_pos))
            .unwrap_or(false)
    }

    /// Check if a popup is currently open.
    pub fn has_open_popup(&self) -> bool {
        self.popup_id.is_some()
    }

    /// Pre-register popup bounds BEFORE rendering regular widgets.
    pub fn preregister_popup(&mut self, bounds: Rect2D) {
        self.popup_bounds = Some(bounds);
    }

    /// Open a popup programmatically by ID.
    pub fn open_popup(&mut self, id: &str) {
        let popup_id = self.generate_id(id);
        self.popup_id = Some(popup_id);
        self.popup_opened_this_frame = true;
        self.active_id = None;
        self.input.focused_id = None;
    }

    /// Open a popup with known bounds (preregisters for same-frame blocking).
    pub fn open_popup_with_bounds(&mut self, id: &str, bounds: Rect2D) {
        self.open_popup(id);
        self.popup_bounds = Some(bounds);
    }

    /// Check if a specific popup is currently open.
    pub fn is_popup_open_with_id(&self, id: &str) -> bool {
        let popup_id = self.generate_id(id);
        self.popup_id == Some(popup_id)
    }

    /// Get the bounds of the current popup.
    pub fn get_popup_bounds(&self) -> Rect2D {
        self.popup_bounds
            .unwrap_or_else(|| Rect2D::from_size(Vec2::new(0.0, 0.0)))
    }

    /// Open a context menu at the current mouse position.
    ///
    /// Call this when detecting a right-click on an area.
    /// Returns true if the menu was just opened.
    pub fn open_context_menu(&mut self, id: &str) -> bool {
        let context_id = self.generate_id(id);

        if self.input.mouse_pressed[mouse_button::RIGHT] {
            self.storage.insert(
                context_id,
                WidgetState::ContextMenuPos(self.input.mouse_pos),
            );
            self.popup_id = Some(context_id);
            self.popup_opened_this_frame = true;
            return true;
        }

        false
    }

    /// Open a context menu at a specific position without checking for input.
    ///
    /// Returns true always (menu was opened).
    pub fn open_context_menu_at(&mut self, id: &str, pos: Vec2) -> bool {
        let context_id = self.generate_id(id);

        self.storage
            .insert(context_id, WidgetState::ContextMenuPos(pos));
        self.popup_id = Some(context_id);
        self.popup_opened_this_frame = true;
        true
    }

    /// Check if a context menu is currently open.
    pub fn is_context_menu_open(&mut self, id: &str) -> bool {
        let context_id = self.generate_id(id);
        self.popup_id == Some(context_id)
    }

    /// Close the current popup/dropdown/context menu.
    pub fn close_current_popup(&mut self) {
        if let Some(popup_id) = self.popup_id {
            self.storage
                .insert(popup_id, WidgetState::DropdownOpen(false));
        }
        self.popup_id = None;
        self.popup_bounds = None;
    }

    /// Track popup item bounds for auto-sizing.
    pub fn track_popup_item(&mut self, item_bounds: Rect2D) {
        self.popup_content_bounds = Some(match self.popup_content_bounds {
            None => item_bounds,
            Some(existing) => Rect2D::new(
                Vec2::new(
                    existing.min.x().min(item_bounds.min.x()),
                    existing.min.y().min(item_bounds.min.y()),
                ),
                Vec2::new(
                    existing.max.x().max(item_bounds.max.x()),
                    existing.max.y().max(item_bounds.max.y()),
                ),
            ),
        });
    }
}
