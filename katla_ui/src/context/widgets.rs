//! Widget behavior helpers and convenience methods.
//!
//! This module provides:
//! - **Interaction helpers**: `click_behavior()`, `is_hovered()`, `update_hover()`
//! - **Internal implementations**: Organized into submodules by widget category
//!
//! # Architecture
//!
//! ## Two-Layer Widget System
//!
//! 1. **Public Builder Widgets** (`crate::widgets`)
//!    - User-facing builder pattern: `Button::new("Click").bounds(my_bounds)`
//!    - Ergonomic, composable, discoverable API
//!
//! 2. **Internal Implementations** (submodules here)
//!    - Actual rendering logic: `UiContext::button_with_colors()`, `UiContext::checkbox()`
//!    - Private implementation details
//!    - Called by builder widgets via the `Widget` trait
//!
//! # Example Flow
//!
//! ```ignore
//! // User code (layer 1)
//! ui.add(Button::new("Click").bounds(my_bounds))
//!
//! // Button::ui() calls (layer 2)
//! ui.button_with_colors("Click", my_bounds, None, None)
//! ```

mod basic;
mod container;
mod graph;
mod scroll_area;
mod toggle_button;
mod utility;

pub use scroll_area::{ScrollArea, ScrollAreaState};

use crate::context::interaction::ClickResult;
use katla_math::Rect2D;

use super::UiContext;

impl UiContext {
    // -------------------------------------------------------------------------
    // Widget Behavior Helpers
    // -------------------------------------------------------------------------

    /// Check if a widget is being hovered.
    ///
    /// Widgets can only be hovered if no higher-z-index content covers the
    /// mouse position. This is tracked automatically by `draw_rect` when
    /// drawing at a z-index above DEFAULT.
    pub fn is_hovered(&self, bounds: Rect2D) -> bool {
        if self.popup_consume_click {
            return false;
        }
        if self.z_index < self.hover_z_index {
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

    pub(crate) fn update_hover(&mut self, id: super::WidgetId, bounds: Rect2D) -> bool {
        let hovered = self.is_hovered(bounds);
        if hovered {
            self.hovered_id = Some(id);
            self.input.want_capture_mouse = true;
        }
        hovered
    }

    /// Handle standard click behavior for interactive widgets.
    ///
    /// Returns a `ClickResult` indicating the interaction state.
    pub(crate) fn click_behavior(&mut self, id: super::WidgetId, hovered: bool) -> ClickResult {
        let active = self.active_id == Some(id);

        if hovered && self.input.mouse_pressed[crate::input::mouse_button::LEFT] {
            self.active_id = Some(id);
            ClickResult::Pressed
        } else if active && self.input.mouse_released[crate::input::mouse_button::LEFT] {
            self.active_id = None;
            if hovered {
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
        self.add(Button::new(text).bounds(bounds))
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
        self.advance_cursor(text_size);
        response
    }
}
