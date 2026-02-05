//! High-level rendering types for deferred draw call submission.
//!
//! This module provides a clean abstraction over Vulkan that avoids exposing
//! ash::vk types to the application layer. Assets are registered with the
//! renderer and referenced via opaque handles.

pub mod registry;
pub mod types;

// Re-export commonly used types for convenience
pub use types::{
    DrawCall, DrawList, MaterialHandle, MaterialParams, MeshHandle,
};
