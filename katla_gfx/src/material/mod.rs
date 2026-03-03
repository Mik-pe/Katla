//! Unified material system.
//!
//! Provides material types and pipeline caching. Materials are defined using
//! MaterialDefinition for flexible, data-driven pipeline creation.

mod definition;
mod template;
mod types;

// Public configuration types
pub use template::MaterialDefinition;

// Public API
pub use definition::MaterialDomain;

// Public material type
pub use types::Material;

// Re-export from vulkan for public API
pub use crate::vulkan::material::descriptor::{RenderState, ShaderSource};
