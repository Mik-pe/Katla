//! Application-layer input module.
//!
//! This module is intentionally `katla_app`-specific (winit lives here).
//! The ECS (`katla_ecs`) should only know about high-level `Action`s / input state.

pub mod map;

pub use map::{InputBinding, InputMapper, KeyCombo, MouseCombo};
