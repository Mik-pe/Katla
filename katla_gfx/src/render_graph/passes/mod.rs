//! Pass templates for render graph construction.
//!
//! This module provides user-friendly pass templates that implement the
//! [`PassBuilder`][crate::render_graph::PassBuilder] trait:
//!
//! - [`GeometryPass`] - Renders 3D geometry with color and depth outputs
//! - [`FullscreenPass`] - Post-processing and compute-like fullscreen effects
//! - [`ShadowPass`] - Shadow mapping for directional, point, and spot lights
//! - [`UIPass`] - 2D UI rendering with alpha blending
//! - [`CompositePass`] - Multi-viewport compositing with positioning
//! - [`LightType`] - Light type enumeration for shadow passes
//!
//! # Example
//!
//! ```ignore
//! use katla_gfx::render_graph::{FrameGraph, GeometryPass, UIPass};
//!
//! let graph = FrameGraph::builder()
//!     .add_pass(GeometryPass::new("geometry")
//!         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
//!         .write_depth("depth", ImageFormat::D32Sfloat))
//!     .add_pass(UIPass::new("ui")
//!         .write("color"))  // Composited on top
//!     .build(&renderer)?;
//!
//! graph.execute(&renderer, |ctx| {
//!     ctx.pass("ui").draw_ui(&ui_draw_list);
//! })?;
//! ```
//!
//! Each pass template uses string-based resource names for convenience.
//! Names are resolved to handles at graph build time with zero runtime overhead.

mod composite;
mod compute;
mod fullscreen;
pub(crate) mod geometry;
mod shadow;
mod ui;
mod viewport;

pub use composite::{CompositePass, CompositePassData, ViewportRect};
pub use compute::{ComputePass, ComputePassData};
pub use fullscreen::{FullscreenPass, TonemapOperator, TonemapParams};
pub use geometry::GeometryPass;
pub use shadow::{LightType, ShadowPass};
pub use ui::UIPass;
pub use viewport::ViewportPass;
