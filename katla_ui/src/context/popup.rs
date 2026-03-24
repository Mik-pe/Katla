//! Popup and menu widgets.
//!
//! Context menus, dropdowns, modal dialogs, and popup containers.
//!
//! All popups use a closure-based API with external state management:
//! ```ignore
//! let mut dialog_open = false;
//!
//! // Open from anywhere
//! if button.clicked { dialog_open = true; }
//!
//! // Render popup - takes &mut bool for open state
//! ui.modal("dialog", &mut dialog_open, 300.0, 150.0, |ui| {
//!     ui.label("Are you sure?");
//!     if ui.button("Close").clicked {
//!         dialog_open = false;
//!     }
//! });
//! ```

mod api;
mod internal;
mod menu;
mod types;

use katla_math::Rect2D;

use super::UiContext;

// Re-export public types
pub use types::{CloseBehavior, Popup, PopupPosition, PopupStyle};

impl UiContext {
    // -------------------------------------------------------------------------
    // Popup State Queries
    // -------------------------------------------------------------------------

    /// Check if any popup is currently open.
    #[inline]
    pub fn has_open_popup(&self) -> bool {
        self.popup_id.is_some()
    }

    /// Get the bounds of the current popup.
    pub fn get_popup_bounds(&self) -> Rect2D {
        self.popup_bounds
            .unwrap_or_else(|| Rect2D::from_size(katla_math::Vec2::new(0.0, 0.0)))
    }

    /// Close the current popup/dropdown/context menu.
    pub(crate) fn close_current_popup(&mut self) {
        self.popup_id = None;
        self.popup_position = None;
        self.popup_bounds = None;
        self.popup_opened_this_frame = false;
    }

    /// Track popup item bounds for auto-sizing.
    pub(crate) fn track_popup_item(&mut self, item_bounds: Rect2D) {
        use katla_math::Vec2;

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
