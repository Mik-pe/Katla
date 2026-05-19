//! Render pass types for the Katla graphics engine.
//!
//! Provides types for configuring render pass attachments and their load/store
//! operations when used with Vulkan's dynamic rendering (Vulkan 1.3+).
//!
//! # Overview
//!
//! - [`AttachmentInfo`] - Describes render pass attachments
//! - [`LoadOp`] / [`StoreOp`] - Attachment load/store operations
//! - [`ClearValue`] - Clear values for attachments
//! - [`BarrierKind`] - Memory barrier types for synchronization

mod types;

pub use types::{AttachmentInfo, BarrierKind, ClearValue, LoadOp, ResourceState, StoreOp};
