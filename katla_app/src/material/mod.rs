//! Material builder utilities for the application layer.

mod builder;
mod gltf_bridge;

pub use builder::{texture_slots, PbrMaterialBuilder, PbrParams};
pub use gltf_bridge::{create_material_from_gltf, GltfMaterialParams};
