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
pub mod widgets;

// Re-exports for convenience
pub use context::{GraphOptions, UiContext};
pub use draw_list::{DrawCommand, DrawList, TextureId, UiVertex};
pub use icons::ForkAwesome;
pub use input::UiInputState;
pub use renderer::{UiRenderError, UiRenderer};
pub use style::{UiStyle, UiTheme};
pub use text::{CachedGlyph, FontError, FontId, FontSystem};
