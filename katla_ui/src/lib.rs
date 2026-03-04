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
//! 2. Call widget functions or use builder widgets to build the UI
//! 3. Call `context.end()` to finalize and get the draw list
//! 4. Render the draw list
//!
//! # Example
//!
//! ```ignore
//! use katla_ui::{UiContext, UiInputState, widgets::Button};
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
//! // Using builder widgets
//! if ui.add(Button::new("Click Me!").bounds(my_bounds)).clicked {
//!     println!("Button clicked!");
//! }
//!
//! let draw_list = ui.end();
//!
//! // Render the draw list with your renderer
//! my_renderer.render(draw_list);
//! ```

mod context;
mod draw_list;
mod icons;
pub mod input;
pub mod response;
mod sense;
mod style;
mod text;
mod widget;
pub mod widgets;

pub use context::{
    z_index, CloseBehavior, GraphOptions, LayoutDirection, LayoutState, Popup, PopupPosition,
    PopupStyle, ScrollArea, ScrollAreaState, UiContext, WindowState, ZGuard,
};
pub use draw_list::{DrawCommand, DrawList, TextureId};
pub use icons::ForkAwesome;
pub use input::{mouse_button, KeyCode, MouseCursor, UiInputState};
pub use katla_gfx::VertexUI;
pub use response::Response;
pub use sense::Sense;
pub use style::{FontSize, UiStyle, UiTheme};
pub use text::{CachedGlyph, FontError, FontId, FontSystem};
pub use widget::Widget;
