//! Katla UI - Immediate mode UI system for the Katla engine.
//!
//! This crate provides a simple, immediate mode UI system suitable for:
//! - Debug overlays and development tools
//! - In-game HUDs
//! - Settings panels
//! - Editor interfaces
//!
//! # Architecture
//!
//! The UI system follows an immediate mode pattern:
//! 1. Call `context.begin()` at the start of the frame
//! 2. Call widget functions (button, label, etc.) to build the UI
//! 3. Call `context.end()` to finalize and get the draw list
//! 4. Render the draw list using `UiRenderer`
//!
//! # Example
//!
//! ```ignore
//! use katla_ui::{UiContext, UiInputState};
//! use katla_math::{Vec2, Rect2D, Color};
//!
//! // Initialize context
//! let mut ui = UiContext::new();
//!
//! // Per-frame input update
//! ui.input.mouse_pos = mouse_position;
//! ui.input.mouse_down[0] = left_mouse_pressed;
//!
//! // Build UI
//! ui.begin(screen_size);
//!
//! if ui.button("click_me", "Click Me!", Rect2D::from_origin_size(Vec2::new(10.0, 10.0), Vec2::new(100.0, 30.0))).clicked {
//!     println!("Button clicked!");
//! }
//!
//! ui.label("Hello, World!", Rect2D::from_origin_size(Vec2::new(10.0, 50.0), Vec2::new(200.0, 20.0)));
//!
//! let draw_list = ui.end();
//!
//! // Render the draw list with your renderer
//! ui_renderer.render(draw_list);
//! ```

mod context;
mod draw_list;
mod icons;
pub mod input;
mod renderer;
mod style;
mod text;

use katla_math::{Rect2D, Vec2};

pub use context::{
    CloseBehavior, Popup, PopupPosition, PopupStyle, z_index, GraphOptions, LayoutDirection,
    LayoutState, ScrollArea, ScrollAreaState, UiContext, WindowState, ZGuard,
};
pub use draw_list::{DrawCommand, DrawList, TextureId, UiVertex};
pub use icons::ForkAwesome;
pub use input::{mouse_button, KeyCode, MouseCursor, UiInputState};
pub use renderer::{UiRenderError, UiRenderer};
pub use style::{FontSize, UiStyle, UiTheme};
pub use text::{CachedGlyph, FontError, FontId, FontSystem};

/// Response from a widget interaction.
///
/// Provides detailed information about widget state and interaction.
/// All interactive widgets return this type.
///
/// # Example
/// ```ignore
/// let resp = ui.button("id", "Click", bounds);
/// if resp.clicked {
///     // Handle click
/// }
/// if resp.hovered {
///     ui.tooltip("Click this button");
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Response {
    /// Widget was clicked this frame (button released while hovered).
    pub clicked: bool,
    /// Widget is being hovered (mouse over, not blocked by popup).
    pub hovered: bool,
    /// Widget is active (mouse pressed on it).
    pub active: bool,
    /// Widget value changed (for sliders, text inputs, checkboxes).
    pub changed: bool,
    /// Widget bounds.
    pub bounds: Rect2D,
    /// Mouse delta while dragging (only valid when active).
    pub drag_delta: Vec2,
    /// Widget was double-clicked this frame.
    pub double_clicked: bool,
}

impl Response {
    /// Create a new response with default values.
    pub fn new(bounds: Rect2D) -> Self {
        Self {
            clicked: false,
            hovered: false,
            active: false,
            changed: false,
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }

    /// Create a response for a clicked widget.
    pub fn clicked(bounds: Rect2D) -> Self {
        Self {
            clicked: true,
            hovered: true,
            active: false,
            changed: true,
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }

    /// Create a response for a hovered widget.
    pub fn hovered(bounds: Rect2D) -> Self {
        Self {
            clicked: false,
            hovered: true,
            active: false,
            changed: false,
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }

    /// Create a response for an active (pressed) widget.
    pub fn active(bounds: Rect2D) -> Self {
        Self {
            clicked: false,
            hovered: true,
            active: true,
            changed: false,
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }

    /// Check if any interaction occurred.
    pub fn any(&self) -> bool {
        self.clicked || self.hovered || self.active || self.changed
    }

    /// Show tooltip on hover (chainable).
    pub fn on_hover_text(self, ui: &mut UiContext, text: &str) -> Self {
        if self.hovered && !self.active {
            ui.tooltip(text);
        }
        self
    }

    /// Combine two responses (union of interactions).
    pub fn union(self, other: Self) -> Self {
        Response {
            clicked: self.clicked || other.clicked,
            hovered: self.hovered || other.hovered,
            active: self.active || other.active,
            changed: self.changed || other.changed,
            bounds: self.bounds.union(&other.bounds),
        }
    }
}

impl Default for Response {
    fn default() -> Self {
        Self::new(Rect2D::from_size(Vec2::new(0.0, 0.0)))
    }
}

impl std::ops::BitOr for Response {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        self.union(other)
    }
}

impl std::ops::BitOrAssign for Response {
    fn bitor_assign(&mut self, other: Self) {
        *self = self.union(other);
    }
}
