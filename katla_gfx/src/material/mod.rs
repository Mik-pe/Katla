//! Unified material system.
//!
//! Provides a trait-based abstraction for materials, reusing existing types
//! from the vulkan module. This trait describes a material's pipeline requirements
//! without dictating implementation details.

mod cache;
mod config;
mod definition;

// Re-export public API
pub use cache::{MaterialCacheError, MaterialCacheStats, MaterialPipelineCache};
pub use config::{
    BindlessPbrMaterialConfig, BindlessSkinnedPbrMaterialConfig, DynamicMaterialConfig,
    FullPbrMaterialConfig, PbrMaterialConfig, SkinnedPbrMaterialConfig,
};
pub use definition::{MaterialDefinition, MaterialDomain, MaterialKey};

// Re-export from vulkan for public API
pub use crate::vulkan::material::descriptor::{RenderState, ShaderSource};
