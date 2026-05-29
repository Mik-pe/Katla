//! Rendering utilities for the application.
//!
//! This module provides rendering-related types and utilities.

pub mod frame_context;

#[cfg(feature = "editor")]
pub mod physics_debug;
#[cfg(feature = "editor")]
pub(crate) mod reverb_debug;

pub use frame_context::FrameContext;

#[cfg(feature = "editor")]
pub use crate::billboard_icons::rasterize_icon as rasterize_billboard_icon;
