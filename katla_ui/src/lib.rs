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
//! if ui.button("click_me", "Click Me!", Rect2D::from_origin_size(Vec2::new(10.0, 10.0), Vec2::new(100.0, 30.0))) {
//!     println!("Button clicked!");
//! }
//!
//! ui.label("Hello, World!", Rect2D::from_origin_size(Vec2::new(10.0, 50.0), Vec2::new(200.0, 20.0)));
//!
//! let draw_list = ui.end();
//!
//! // Render with UiRenderer
//! ui_renderer.render(command_buffer, draw_list, screen_size);
//! ```
//!
//! # Icon Fonts
//!
//! Icon fonts like ForkAwesome can be loaded and used for rendering icons.
//! Load the icon font with `FontId::ICON` and use `draw_icon()`:
//!
//! ```ignore
//! use katla_ui::{FontId, icons::ForkAwesome};
//!
//! // In setup:
//! let icon_bytes = include_bytes!("path/to/forkawesome.ttf");
//! ui.fonts.add_font_with_id(icon_bytes, FontId::ICON)?;
//! ui.fonts.precache_icons(FontId::ICON, 16.0, scale_factor, ForkAwesome::common_icons());
//!
//! // In render loop:
//! ui.draw_icon(ForkAwesome::CUBE, pos, 16.0, [1.0, 1.0, 1.0, 1.0]);
//! ```
//!
//! # Dependency Restrictions
//!
//! This crate follows Katla's architecture rules:
//! - CAN depend on: `katla_math`, `katla_vulkan`
//! - MUST NOT depend on: `katla_ecs`, `katla_app`

pub mod context;
pub mod draw_list;
pub mod icons;
pub mod input;
pub mod primitives;
pub mod renderer;
pub mod style;
pub mod text;

// Keep existing module paths for compatibility
pub mod layout {
    //! Layout utilities (placeholder - implemented in context module)
    pub use crate::context::{LayoutDirection, LayoutState};
}

pub mod widgets {
    //! Built-in UI widgets (placeholder - implemented in context module)
}

// Re-exports for convenience
pub use context::{
    z_index, GraphOptions, LayoutDirection, LayoutState, UiContext, WindowState, ZGuard,
};
pub use draw_list::{DrawCommand, DrawList, TextureId, UiVertex};
pub use icons::ForkAwesome;
pub use input::UiInputState;
pub use renderer::{UiRenderError, UiRenderer};
pub use style::{FontSize, UiStyle, UiTheme};
pub use text::{CachedGlyph, FontError, FontId, FontSystem};

use katla_math::Rect2D;

/// Response from a widget interaction.
///
/// Provides detailed information about widget state and interaction.
/// Use this for more expressive widget handling.
///
/// # Example
/// ```ignore
/// let resp = ui.button_response("id", "Click", bounds);
/// if resp.clicked {
///     // Handle click
/// }
/// if resp.hovered {
///     ui.tooltip("Click this button");
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Response {
    /// Widget was clicked this frame.
    pub clicked: bool,
    /// Widget is being hovered.
    pub hovered: bool,
    /// Widget is active (pressed/held).
    pub active: bool,
    /// Widget value changed (for sliders, text inputs, etc.).
    pub changed: bool,
    /// Widget bounds.
    pub bounds: Rect2D,
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
        }
    }

    /// Check if any interaction occurred.
    pub fn any(&self) -> bool {
        self.clicked || self.hovered || self.active || self.changed
    }
}

impl Default for Response {
    fn default() -> Self {
        Self::new(Rect2D::from_size(katla_math::Vec2::new(0.0, 0.0)))
    }
}

// Add Response-based widget methods to UiContext
impl UiContext {
    /// Draw a button and return a detailed response.
    ///
    /// This is an alternative API that provides more information than the
    /// simple boolean return of `button()`.
    pub fn button_response(&mut self, id: &str, text: &str, bounds: Rect2D) -> Response {
        let widget_id = self.generate_id(id);
        let clicked = self.button_behavior(widget_id, bounds);
        let hovered = self.hovered_id == Some(widget_id) || self.is_hovered(bounds);
        let active = self.active_id == Some(widget_id);

        // Determine colors based on state
        let (bg_color, text_color) = if active {
            (self.style.button_active, self.style.button_text)
        } else if hovered {
            (self.style.button_hovered, self.style.button_text)
        } else {
            (self.style.button_normal, self.style.button_text)
        };

        // Draw button background
        self.draw_rect(bounds, bg_color);

        // Draw button text (centered)
        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = katla_math::Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(text, text_pos, text_color, self.style.font_size);

        Response {
            clicked,
            hovered,
            active,
            changed: clicked,
            bounds,
        }
    }

    /// Draw a checkbox and return a detailed response.
    pub fn checkbox_response(&mut self, id: &str, label: &str, checked: &mut bool, bounds: Rect2D) -> Response {
        let changed = self.checkbox(id, label, checked, bounds);
        let widget_id = self.generate_id(id);
        let hovered = self.hovered_id == Some(widget_id);
        let active = self.active_id == Some(widget_id);

        Response {
            clicked: changed,
            hovered,
            active,
            changed,
            bounds,
        }
    }

    /// Draw a slider and return a detailed response.
    pub fn slider_response(
        &mut self,
        id: &str,
        value: &mut f32,
        min: f32,
        max: f32,
        bounds: Rect2D,
    ) -> Response {
        let widget_id = self.generate_id(id);
        let changed = self.slider(id, value, min, max, bounds);
        let hovered = self.hovered_id == Some(widget_id);
        let active = self.active_id == Some(widget_id);

        Response {
            clicked: false,
            hovered,
            active,
            changed,
            bounds,
        }
    }
}
