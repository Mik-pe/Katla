//! UI integration module.
//!
//! This module provides the bridge between katla_ui and the application layer.

mod asset_browser;
mod debug_overlay;
mod editor_ui;
pub mod theme;

pub use asset_browser::{AssetAction, AssetBrowserState, AssetEntry, AssetType};
pub use debug_overlay::DebugOverlay;
pub use editor_ui::{EditorAction, EditorUI, EntityInfo, FocusedPanel, SpawnableModel};
pub use theme::Theme;
