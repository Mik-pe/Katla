//! Unified material system.
//!
//! Provides material types and pipeline caching. Materials are defined using
//! MaterialDefinition for flexible, data-driven pipeline creation.

mod cache;
mod definition;
mod template;
mod types;

// Internal types (not public API)
pub(crate) use cache::MaterialPipelineCache;
pub(crate) use template::DescriptorSetLayout;

// Public configuration types
pub use template::MaterialDefinition;

// Public API
pub use definition::MaterialDomain;

// Internal API (pipeline caching)
pub(crate) use definition::MaterialKey;

// Public material type
pub use types::Material;

// Re-export from vulkan for public API
pub use crate::vulkan::material::descriptor::{RenderState, ShaderSource};
