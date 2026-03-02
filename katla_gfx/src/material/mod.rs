//! Unified material system.
//!
//! Provides a trait-based abstraction for materials, reusing existing types
//! from the vulkan module. This trait describes a material's pipeline requirements
//! without dictating implementation details.

mod cache;
mod config;
mod definition;

// Internal types (not public API)
pub(crate) use cache::MaterialPipelineCache;
pub(crate) use config::DynamicMaterialConfig;

// Public API
pub use config::{PbrMaterialConfig, PbrMaterialFlags};
pub use definition::{MaterialDefinition, MaterialDomain, MaterialKey};

// Re-export from vulkan for public API
pub use crate::vulkan::material::descriptor::{RenderState, ShaderSource};
