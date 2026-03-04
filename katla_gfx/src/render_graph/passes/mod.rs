//! Pass templates for render graph construction.
//!
//! This module provides user-friendly pass templates that implement the
//! [`PassBuilder`][crate::render_graph::PassBuilder] trait:
//!
//! - [`GeometryPass`] - Renders 3D geometry with color and depth outputs
//! - [`FullscreenPass`] - Post-processing and compute-like fullscreen effects
//! - [`ShadowPass`] - Shadow mapping for directional, point, and spot lights
//! - [`LightType`] - Light type enumeration for shadow passes
//!
//! # Example
//!
//! ```ignore
//! use katla_gfx::render_graph::{FrameGraph, GeometryPass, FullscreenPass};
//!
//! let graph = FrameGraph::builder()
//!     .add_pass(GeometryPass::new("geometry")
//!         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
//!         .write_depth("depth", ImageFormat::D32Sfloat))
//!     .add_pass(FullscreenPass::new("tonemap")
//!         .read("color")
//!         .write("output", ImageFormat::R8G8B8A8Srgb))
//!     .build(&renderer)?;
//! ```
//!
//! Each pass template uses string-based resource names for convenience.
//! Names are resolved to handles at graph build time with zero runtime overhead.

mod fullscreen;
mod geometry;
mod shadow;

pub use fullscreen::FullscreenPass;
pub use geometry::GeometryPass;
pub use shadow::{LightType, ShadowPass};

pub(crate) use fullscreen::FullscreenPassData;
pub(crate) use geometry::GeometryPassData;
pub(crate) use shadow::ShadowPassData;
