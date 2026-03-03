//! Material builder utilities for the application layer.

mod builder;
mod gltf_bridge;
mod presets;

pub use builder::{texture_slots, PbrMaterialBuilder, PbrParams};
pub use gltf_bridge::{create_material_from_gltf, GltfMaterialParams};
pub use presets::{
    create_emissive_preset, create_metallic_preset, create_pbr_preset, create_rough_preset,
    create_smooth_preset,
};
