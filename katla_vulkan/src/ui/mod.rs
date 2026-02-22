//! UI Rendering utilities.
//!
//! This module provides types for rendering immediate mode UI overlays.
//! The key insight is that UI is just 2D geometry with textures - no special
//! "UI" concept needed in the core renderer.
//!
//! The application layer owns and manages these resources, not VulkanRenderer.

mod renderer;

pub use renderer::{UiDrawCommand, UiDrawData, UIRenderer};
