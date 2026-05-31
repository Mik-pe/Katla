//! Widget behavior helpers.
//!
//! This module provides:
//! - **Interaction helpers**: `click_interaction()`, `is_hovered()`, `update_hover()`
//! - **Internal implementations**: Organized into submodules by widget category

mod scroll_area;
mod utility;

pub use scroll_area::ScrollAreaState;

use crate::context::interaction::{ClickConfig, ClickResult};
use katla_math::Rect2D;

use super::UiContext;

impl UiContext {
    // -------------------------------------------------------------------------
    // Widget Behavior Helpers
    // -------------------------------------------------------------------------

    /// Register a widget as focusable for Tab navigation.
    ///
    /// Call this for any interactive widget that should participate in
    /// keyboard Tab/Shift+Tab focus cycling. Widgets are navigated in
    /// the order they are registered during the frame.
    pub(crate) fn register_focusable(&mut self, id: super::WidgetId, bounds: Rect2D, label: &str) {
        self.focusable_widgets.push((id, bounds));

        if let Some(ref pending) = self.pending_focus_label
            && pending == label
        {
            self.focused_id = Some(id);
            self.pending_focus_label = None;
        }
    }

    /// Check if a widget is being hovered.
    ///
    /// Widgets can only be hovered if no higher-z-index content covers the
    /// mouse position. This is tracked automatically by `draw_rect` when
    /// drawing at a z-index above DEFAULT.
    ///
    /// Uses the maximum of the current and previous frame's hover_z_index
    /// to handle cases where higher-z content hasn't re-rendered yet this
    /// frame (e.g., popups drawn after the widgets checking hover).
    pub(crate) fn is_hovered(&self, bounds: Rect2D) -> bool {
        let effective_z = self.hover_z_index.max(self.prev_hover_z_index);
        if self.z_index < effective_z {
            return false;
        }
        self.input.is_hovered(bounds) && self.active_id.is_none()
    }

    /// Register that the mouse is hovering over content at the given z-index.
    ///
    /// Called automatically by `draw_rect` when the current z-index is above
    /// DEFAULT. The highest z-index wins — if multiple regions overlap at
    /// the mouse position, only the highest z-index is remembered.
    pub(crate) fn register_hover_layer(&mut self, z: u32, bounds: Rect2D) {
        if z > self.hover_z_index && bounds.contains(self.input.mouse_pos) {
            self.hover_z_index = z;
        }
    }

    pub(crate) fn update_hover(&mut self, id: super::WidgetId, bounds: Rect2D) -> bool {
        let hovered = self.is_hovered(bounds);
        if hovered {
            self.hovered_id = Some(id);
            self.input.want_capture_mouse = true;
        }
        hovered
    }

    /// Unified click interaction handling.
    ///
    /// Detects press, release, and click events with configurable popup awareness.
    /// Returns a `ClickResult` indicating the interaction state.
    ///
    /// - `popup_aware: true` — uses pre-computed `hovered` on release (respects popup blocking).
    ///   Used by checkboxes, radio buttons, toggle buttons, image buttons, and sliders.
    /// - `popup_aware: false` — uses raw `input.is_hovered(bounds)` on release to bypass
    ///   popup blocking. Used by buttons and menu items that must respond even when
    ///   a popup is consuming clicks.
    pub(crate) fn click_interaction(
        &mut self,
        id: super::WidgetId,
        hovered: bool,
        bounds: Rect2D,
        config: ClickConfig,
    ) -> ClickResult {
        let active = self.active_id == Some(id);

        if hovered && self.input.mouse_pressed[crate::input::mouse_button::LEFT] {
            self.active_id = Some(id);
            ClickResult::Pressed
        } else if active && self.input.mouse_released[crate::input::mouse_button::LEFT] {
            self.active_id = None;
            let release_hovered = if config.popup_aware {
                hovered
            } else {
                self.input.is_hovered(bounds)
            };
            if release_hovered {
                ClickResult::Clicked
            } else {
                ClickResult::Released
            }
        } else {
            ClickResult::None
        }
    }
}
