//! Re-export of the unified Material type from katla_vulkan.
//!
//! This module provides a direct re-export of katla_vulkan::Material
//! without any additional compatibility layers.

// Re-export the unified Material type and related types
pub use katla_vulkan::{
    material::PbrTextureSet,
    MaterialHandle,
    MaterialTemplate,
    Texture,
    VertexBinding,
    vulkan::material::Material,
};
