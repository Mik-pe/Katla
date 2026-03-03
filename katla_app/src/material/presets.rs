//! Material presets for common PBR material configurations.
//!
//! These functions provide convenient ways to create common material types
//! with sensible default values.

use katla_gfx::handle::MaterialHandle;
use katla_gfx::MaterialInstance;

use super::builder::PbrMaterialBuilder;

/// Create a metallic material preset (chrome, gold, etc.).
///
/// Configures: metallic=1.0, roughness=0.2
pub fn create_metallic_preset(template: MaterialHandle) -> MaterialInstance {
    PbrMaterialBuilder::metal(template).build()
}

/// Create an emissive material preset.
///
/// Configures: metallic=0.0, roughness=0.8
/// Note: Set the emissive texture separately using the builder.
pub fn create_emissive_preset(template: MaterialHandle) -> MaterialInstance {
    PbrMaterialBuilder::new(template)
        .with_metallic(0.0)
        .with_roughness(0.8)
        .build()
}

/// Create a standard PBR material preset.
///
/// Configures: metallic=0.0, roughness=0.5
pub fn create_pbr_preset(template: MaterialHandle) -> MaterialInstance {
    PbrMaterialBuilder::plastic(template).build()
}

/// Create a rough/diffuse material preset.
///
/// Configures: metallic=0.0, roughness=0.9
pub fn create_rough_preset(template: MaterialHandle) -> MaterialInstance {
    PbrMaterialBuilder::new(template)
        .with_metallic(0.0)
        .with_roughness(0.9)
        .build()
}

/// Create a smooth/glossy material preset.
///
/// Configures: metallic=0.0, roughness=0.1
pub fn create_smooth_preset(template: MaterialHandle) -> MaterialInstance {
    PbrMaterialBuilder::new(template)
        .with_metallic(0.0)
        .with_roughness(0.1)
        .build()
}
