//! Render graph API for frame rendering.
//!
//! This module provides a frame graph implementation for managing
//! render passes, resources, and dependencies.
//!
//! # Overview
//!
//! - [`FrameGraph`] - Executable render graph
//! - [`ExecutionContext`] - Context for pass execution
//! - [`PassBuilder`] - Trait for building render passes
//! - [`GeometryPass`] - Geometry render pass template
//! - [`FullscreenPass`] - Fullscreen/compute pass template
//! - [`ShadowPass`] - Shadow mapping pass template
//! - [`UIPass`] - UI render pass template
//! - [`LightType`] - Light type enumeration for shadow passes

mod builder;
mod compiler;
mod error;
mod graph;
mod pass;
mod passes;
mod resource;

pub use builder::PassBuilder;
pub use error::RenderGraphError;
pub use graph::{ExecutionContext, FrameGraph, FrameGraphBuilder, PassHandle};
pub use passes::{FullscreenPass, GeometryPass, LightType, ShadowPass};

pub(crate) use builder::InternalPassBuilder;
pub(crate) use compiler::{ExecutionPlan, GraphCompiler, PassInfo, ResourceBarrier};
pub(crate) use pass::{PassContext, PassDesc, PassExecFn, PassType};
pub(crate) use resource::{GraphResourceHandle, ResourceState};
