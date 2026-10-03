//! Rendering utilities for the application.
//!
//! This module provides rendering-related types and utilities.

pub mod frame_context;
mod frame_uniforms;
pub use frame_uniforms::FrameUniforms;
pub(crate) mod material;
pub(crate) use material::SurfaceParameters;
pub use material::{AlphaMode, MaterialSurface};
mod particle_stats;
pub use particle_stats::ParticleStats;

#[cfg(feature = "editor")]
pub mod grid;
#[cfg(feature = "editor")]
pub mod physics_debug;
#[cfg(feature = "editor")]
pub(crate) mod reverb_debug;

pub use frame_context::{FrameContext, FrameSubmission};

#[cfg(feature = "editor")]
pub use crate::billboard_icons::rasterize_icon as rasterize_billboard_icon;
