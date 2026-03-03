//! Built-in render pass templates.
//!
//! This module provides ready-to-use render pass implementations for common
//! rendering scenarios. These can be used directly or as templates for
//! custom passes.
//!
//! # Available Passes
//!
//! - [`GeometryPass`] - Renders 3D geometry with color and depth outputs
//! - [`FullscreenPass`] - Fullscreen quad pass for post-processing effects
//! - [`TonemapPass`] - Converts HDR to LDR with configurable tone mapping
//! - [`UIPass`] - Renders immediate mode UI overlays

mod fullscreen;
mod geometry;
mod tonemap;
mod ui;

pub use fullscreen::FullscreenPass;
pub use geometry::GeometryPass;
pub use tonemap::{TonemapMethod, TonemapPass};
pub use ui::UIPass;
