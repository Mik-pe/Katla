//! Render pass architecture for the Katla graphics engine.
//!
//! This module provides a flexible, trait-based render pass system for defining
//! custom rendering pipelines. It integrates with Vulkan's dynamic rendering
//! (Vulkan 1.3+) for efficient rendering without explicit render pass objects.
//!
//! # Overview
//!
//! - [`RenderPass`] - Core trait for defining render passes
//! - [`RenderPassContext`] - Context provided during pass execution
//! - [`AttachmentInfo`] - Describes render pass attachments
//! - [`AttachmentResources`] - Manages GPU resources for attachments
//! - [`PassExecutor`] - Executes render passes with dynamic rendering
//! - [`LoadOp`] / [`StoreOp`] - Attachment load/store operations
//! - [`ClearValue`] - Clear values for attachments
//! - [`BarrierKind`] - Memory barrier types for synchronization
//!
//! # Built-in Pass Templates
//!
//! - [`GeometryPass`](passes::GeometryPass) - Renders 3D geometry
//! - [`FullscreenPass`](passes::FullscreenPass) - Post-processing effects
//!
//! # Architecture
//!
//! The render pass system is designed around:
//! 1. **Trait-based design** - Each pass implements `RenderPass`
//! 2. **Vulkan 1.3 dynamic rendering** - No explicit render pass objects
//! 3. **Automatic synchronization** - Passes declare their barrier requirements
//!
//! # Example
//!
//! ```ignore
//! use katla_gfx::render_pass::{RenderPass, RenderPassContext, AttachmentInfo, BarrierKind, PassExecutor};
//! use katla_gfx::render_pass::passes::{GeometryPass, FullscreenPass};
//! use katla_gfx::texture::ImageFormat;
//!
//! // Create a geometry pass
//! let geometry = GeometryPass::new()
//!     .output_color("color", ImageFormat::R16G16B16A16Sfloat)
//!     .output_depth("depth", ImageFormat::D32Sfloat);
//!
//! // Create a post-processing pass
//! let tone_map = FullscreenPass::new("ToneMapPass")
//!     .input("color")
//!     .output("output", ImageFormat::R8G8B8A8Srgb);
//! ```

//!
//! # Dynamic Rendering (Vulkan 1.3)
//!
//! This system uses Vulkan 1.3's dynamic rendering feature, which eliminates
//! the need for explicit render pass objects. Instead, attachments are bound
//! directly at draw time using `vkCmdBeginRendering` and `vkCmdEndRendering`.
//!
//! Benefits:
//! - Reduced pipeline creation overhead
//! - More flexible attachment management
//! - Better compatibility with modern rendering techniques

mod attachment;
mod types;

pub use attachment::AttachmentResources;
pub use types::{AttachmentInfo, BarrierKind, ClearValue, LoadOp, StoreOp};
