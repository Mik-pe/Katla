//! Descriptor sets for render graph passes.
//!
//! This module provides specialized descriptor sets for different pass types:
//!
//! - [`CompositingDescriptorSet`] - Multi-viewport compositing with texture arrays
//!
//! # Example
//!
//! ```ignore
//! use katla_gfx::render_graph::descriptor_sets::CompositingDescriptorSet;
//!
//! // Create descriptor set with viewport textures
//! let textures = vec![viewport0_view, viewport1_view];
//! let desc_set = CompositingDescriptorSet::new(&context, &textures)?;
//! ```

mod compositing;

pub use compositing::CompositingDescriptorSet;
