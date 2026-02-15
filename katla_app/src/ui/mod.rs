//! UI integration module.
//!
//! This module provides the bridge between katla_ui and the application layer.

mod debug_overlay;
mod editor_ui;

pub use debug_overlay::DebugOverlay;
pub use editor_ui::{EditorUI, EditorAction, EntityInfo, SpawnableModel};
