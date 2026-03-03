//! Unified material system.
//!
//! Provides material types and pipeline caching. Materials are defined using
//! MaterialTemplateConfig for flexible, data-driven pipeline creation.

mod cache;
mod definition;
mod template;
mod types;

// Internal types (not public API)
pub(crate) use cache::MaterialPipelineCache;
pub(crate) use template::DescriptorSetLayout;

// Public configuration types
pub use template::MaterialTemplateConfig;

// Public API
pub use definition::{MaterialDomain, MaterialKey};

/// Type alias for the simple handle-based Material type.
/// Use this for creating materials with the PbrMaterialBuilder.
pub type MaterialInstance = types::Material;

// Re-export from vulkan for public API
pub use crate::vulkan::material::descriptor::{RenderState, ShaderSource};
