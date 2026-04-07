//! Icon font support for UI rendering.
//!
//! This module provides icon constants for use with icon fonts like ForkAwesome.
//! Icons are represented as Unicode characters in the Private Use Area (PUA).
//!
//! # Usage
//!
//! ```ignore
//! use katla_ui::{FontId, icons::ForkAwesome};
//!
//! // Draw an icon using the icon font
//! ui.set_font(FontId::ICON);
//! ui.draw_text(&ForkAwesome::CUBE.to_string(), pos, color, 16.0);
//! ui.set_font(FontId::DEFAULT); // Switch back to regular font
//!
//! // Or use the convenience method:
//! ui.draw_icon(ForkAwesome::CUBE, pos, 16.0, color);
//! ```

pub use katla_icons::ForkAwesome;
