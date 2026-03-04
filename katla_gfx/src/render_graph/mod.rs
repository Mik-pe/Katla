//! Render graph API for frame rendering.
//!
//! This module provides a frame graph implementation for managing render passes,
//! resources, and dependencies with automatic barrier generation.
//!
//! # Overview
//!
//! - [`FrameGraph`] - Executable render graph (build once, execute every frame)
//! - [`Frame`] - Context for submitting work during frame execution
//! - [`GeometryPass`] - Geometry render pass template
//! - [`FullscreenPass`] - Fullscreen/compute pass template
//! - [`ShadowPass`] - Shadow mapping pass template
//!
//! # Example
//!
//! ```ignore
//! // Build once at startup
//! let frame_graph = renderer.create_frame_graph()
//!     .add_pass(GeometryPass::new("geometry")
//!         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
//!         .write_depth("depth", ImageFormat::D32Sfloat))
//!     .add_pass(FullscreenPass::new("tonemap")
//!         .read("color")
//!         .write_backbuffer())
//!     .build()?;
//!
//! // Execute every frame
//! renderer.render(&frame_graph, |frame| {
//!     frame.submit("geometry", &draw_list);
//! });
//! ```

mod builder;
mod compiler;
mod error;
mod graph;
mod pass;
mod passes;
mod resource;

// Public API - minimal surface
pub use error::RenderGraphError;
pub use graph::{Frame, FrameGraph, FrameGraphBuilder};
pub use passes::{FullscreenPass, GeometryPass, LightType, ShadowPass};

// Internal - for pass template implementation
pub(crate) use builder::{InternalPassBuilder, PassBuilder};
pub(crate) use pass::{PassContext, PassDesc, PassType};
pub(crate) use resource::{GraphResourceHandle, ResourceState};
