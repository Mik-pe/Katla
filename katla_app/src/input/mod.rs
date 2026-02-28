//! Application-layer input module.
//!
//! This module is intentionally `katla_app`-specific (winit lives here).
//! The ECS (`katla_ecs`) should only know about high-level `Action`s / input state.

pub mod map;
pub mod viewport_input;

pub use map::{InputBinding, InputMapper, KeyCombo, MouseCombo};
pub use viewport_input::{is_viewport_active, update_active_viewport};
