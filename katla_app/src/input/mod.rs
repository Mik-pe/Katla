//! Application-layer input module.
//!
//! This module is intentionally `katla_app`-specific (winit lives here).
//! The ECS (`katla_ecs`) should only know about high-level `Action`s / input state.

pub mod actions;
pub mod map;
pub mod mouse;
pub mod state;
pub mod viewport_input;

pub use actions::Action;
pub use map::{InputBinding, InputMapper, KeyCombo, MouseCombo};
pub use mouse::MouseButton;
pub use state::{ButtonState, InputState, ModifierKey};
pub use viewport_input::{is_viewport_active, update_active_viewport};
