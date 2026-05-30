//! Widget behavior helpers and convenience methods.
//!
//! This module provides:
//! - **Interaction helpers**: `click_interaction()`, `is_hovered()`, `update_hover()`
//! - **Internal implementations**: Organized into submodules by widget category
//!
//! # Architecture
//!
//! ## Two-Layer Widget System
//!
//! 1. **Public Builder Widgets** (`crate::widgets`)
//!    - User-facing builder pattern: `ImageButton::new(icon).bounds(my_bounds)`
//!    - Ergonomic, composable, discoverable API
//!
//! 2. **Internal Implementations** (submodules here)
//!    - Actual rendering logic: `UiContext::button_with_colors()`, `UiContext::slider()`
//!    - Private implementation details
//!    - Called by builder widgets via the `Widget` trait
//!
//! # Example Flow
//!
//! ```ignore
//! // User code (layer 1)
//! ui.add(ImageButton::new('X').bounds(my_bounds))
//!
//! // ToggleButton::ui() calls (layer 2)
//! ui.toggle_button(&ToggleButtonParams { ... })
//! ```

mod basic;
mod scroll_area;
mod utility;

pub use scroll_area::{ScrollArea, ScrollAreaState};

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
    pub(crate) fn register_focusable(&mut self, id: super::WidgetId, bounds: Rect2D) {
        self.focusable_widgets.push((id, bounds));
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
    pub fn is_hovered(&self, bounds: Rect2D) -> bool {
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
    pub fn register_hover_layer(&mut self, z: u32, bounds: Rect2D) {
        if z > self.hover_z_index && bounds.contains(self.input.mouse_pos) {
            self.hover_z_index = z;
        }
    }

    /// Create a hit-test response for a given bounds.
    ///
    /// Returns a [`crate::Response`] populated with `hovered`, `right_clicked`,
    /// `middle_clicked`, and other interaction state for the given rectangle.
    /// Useful for custom list/grid items rendered inside callbacks where no
    /// widget produces a Response.
    ///
    /// This does **not** register the area as a focusable widget or set an
    /// active/hovered widget ID — it only reads current input state.
    pub fn sense(&self, bounds: Rect2D) -> crate::Response {
        let hovered = self.is_hovered(bounds);
        crate::Response::interactive(false, hovered, false, bounds, &self.input, None)
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

    // -------------------------------------------------------------------------
    // Convenience Widget Methods (Auto-Layout)
    // -------------------------------------------------------------------------

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
        self.advance_cursor(text_size);
        response
    }
}
