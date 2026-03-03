//! Fixed render pipeline builder and executor.
//!
//! This module provides a builder-pattern API for creating fixed render pipelines
//! that combine multiple render passes into a cohesive rendering system.

mod builder;
mod fixed;

pub use builder::FixedPipelineBuilder;
pub use fixed::{AttachmentSize, FixedPipeline};
