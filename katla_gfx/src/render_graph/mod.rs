//! Render graph API for frame rendering.
//!
//! This module provides a frame graph implementation for managing render passes,
//! resources, and dependencies with automatic barrier generation.
//!
//! The render graph has three layers:
//!
//! - **Layer 1 (Graph Structure)**: `FrameGraphBuilder`, `PassBuilder`, `PassDesc`,
//!   `GraphCompiler`, `ExecutionPlan` — pure data and dependency analysis, no GPU types.
//! - **Layer 2 (Backend Interface)**: `RenderGraphBackend` trait — defines how the
//!   render graph interacts with a specific GPU backend.
//! - **Layer 3 (Backend Implementation)**: `VulkanRenderGraph` / `MetalRenderGraph` —
//!   concrete implementations for each backend.
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
//!     frame.submit(geometry_pass_id, &draw_list);
//! });
//! ```

// Layer 1: Backend-agnostic graph structure (no GPU types)
mod builder;
mod compiler;
#[cfg(feature = "vulkan")]
pub mod descriptor_sets;
mod error;
mod frame_graph;
mod handles;
mod pass;
mod passes;
mod resource;

// Layer 2: Backend interface trait
mod backend;

// Layer 3: Backend-specific execution
pub mod any_frame;
pub mod any_frame_graph;
mod frame;
#[cfg(all(target_os = "macos", feature = "metal"))]
mod metal_backend;
#[cfg(feature = "vulkan")]
mod transient_texture;
#[cfg(feature = "vulkan")]
mod vulkan_backend;

// Public API
pub use backend::RenderGraphBackend;
pub use builder::SimplePass;
pub use error::RenderGraphError;
pub use frame::Frame;
pub(crate) use frame::PassExecutionData;
pub use frame_graph::{FrameGraph, FrameGraphBuilder};
pub use handles::{PassId, ResourceId};
pub use pass::{PassDesc, PassKind, PassType};
pub use passes::{
    CompositePass, DepthPrepass, FullscreenPass, GeometryPass, OutlinePass, OverlayParams,
    OverlayPass, ParticlePass, ShadowPass, StencilIndicatorPass, TonemapOperator, TonemapParams,
    UIPass, ViewportPass, ViewportRect,
};
pub use resource::{
    GraphResourceDesc, GraphResourceHandle, GraphResourceType, ResourceState, TransientTextureOps,
};
#[cfg(feature = "vulkan")]
pub use transient_texture::TransientTexture;

/// Special resource name for the swapchain backbuffer.
pub const BACKBUFFER_NAME: &str = "backbuffer";
