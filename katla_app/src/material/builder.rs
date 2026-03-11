//! PBR Material builder for convenient material creation.

use bytemuck::{Pod, Zeroable};
use katla_gfx::handle::{MaterialHandle, TextureHandle};
use katla_gfx::Material;

/// Texture slot indices for PBR materials.
pub mod texture_slots {
    /// Albedo/base color texture slot.
    pub const ALBEDO: u32 = 0;
    /// Normal map texture slot.
    pub const NORMAL_MAP: u32 = 1;
    /// Metallic/roughness texture slot (R=metallic, G=roughness).
    pub const METALLIC_ROUGHNESS: u32 = 2;
    /// Emissive texture slot.
    pub const EMISSIVE: u32 = 3;
    /// Ambient occlusion texture slot.
    pub const OCCLUSION: u32 = 4;
}

/// Push constant data for PBR scalar parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PbrParams {
    /// Metallic value (0.0 = dielectric, 1.0 = metal).
    pub metallic: f32,
    /// Roughness value (0.0 = smooth, 1.0 = rough).
    pub roughness: f32,
}

/// Builder for creating PBR (Physically Based Rendering) materials.
///
/// This builder provides a convenient API for configuring PBR materials
/// with textures and scalar parameters. It builds a `Material`
/// with all configured properties.
pub struct PbrMaterialBuilder {
    /// Reference to the material template (pipeline + descriptor layouts)
    pub(crate) template: MaterialHandle,
    /// Albedo/base color texture (use TextureHandle::NONE for no texture)
    pub(crate) albedo: TextureHandle,
    /// Normal map texture (use TextureHandle::NONE for no texture)
    pub(crate) normal_map: TextureHandle,
    /// Metallic/roughness texture (use TextureHandle::NONE for no texture)
    pub(crate) metallic_roughness: TextureHandle,
    /// Emission/glow texture (use TextureHandle::NONE for no texture)
    pub(crate) emissive: TextureHandle,
    /// Ambient occlusion texture (use TextureHandle::NONE for no texture)
    pub(crate) occlusion: TextureHandle,
    /// Scalar metallic value (0.0-1.0)
    pub(crate) metallic: f32,
    /// Scalar roughness value (0.0-1.0)
    pub(crate) roughness: f32,
}

impl PbrMaterialBuilder {
    /// Create a new PBR material builder with the given template.
    pub fn new(template: MaterialHandle) -> Self {
        Self {
            template,
            albedo: TextureHandle::NONE,
            normal_map: TextureHandle::NONE,
            metallic_roughness: TextureHandle::NONE,
            emissive: TextureHandle::NONE,
            occlusion: TextureHandle::NONE,
            metallic: 0.0,
            roughness: 0.5,
        }
    }

    /// Create a builder preset for metallic materials (chrome, gold, etc.).
    ///
    /// Configures: metallic=1.0, roughness=0.2
    pub fn metal(template: MaterialHandle) -> Self {
        Self {
            template,
            albedo: TextureHandle::NONE,
            normal_map: TextureHandle::NONE,
            metallic_roughness: TextureHandle::NONE,
            emissive: TextureHandle::NONE,
            occlusion: TextureHandle::NONE,
            metallic: 1.0,
            roughness: 0.2,
        }
    }

    /// Create a builder preset for plastic/dielectric materials.
    ///
    /// Configures: metallic=0.0, roughness=0.5
    pub fn plastic(template: MaterialHandle) -> Self {
        Self {
            template,
            albedo: TextureHandle::NONE,
            normal_map: TextureHandle::NONE,
            metallic_roughness: TextureHandle::NONE,
            emissive: TextureHandle::NONE,
            occlusion: TextureHandle::NONE,
            metallic: 0.0,
            roughness: 0.5,
        }
    }

    /// Set the albedo texture.
    pub fn with_albedo(mut self, texture: TextureHandle) -> Self {
        self.albedo = texture;
        self
    }

    /// Set the normal map texture.
    pub fn with_normal_map(mut self, texture: TextureHandle) -> Self {
        self.normal_map = texture;
        self
    }

    /// Set the metallic/roughness texture.
    pub fn with_metallic_roughness(mut self, texture: TextureHandle) -> Self {
        self.metallic_roughness = texture;
        self
    }

    /// Set the emissive texture.
    pub fn with_emissive(mut self, texture: TextureHandle) -> Self {
        self.emissive = texture;
        self
    }

    /// Set the ambient occlusion texture.
    pub fn with_occlusion(mut self, texture: TextureHandle) -> Self {
        self.occlusion = texture;
        self
    }

    /// Set the metallic scalar value (clamped to 0.0-1.0).
    pub fn with_metallic(mut self, metallic: f32) -> Self {
        self.metallic = metallic.clamp(0.0, 1.0);
        self
    }

    /// Set the roughness scalar value (clamped to 0.0-1.0).
    pub fn with_roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness.clamp(0.0, 1.0);
        self
    }

    /// Build the final Material.
    ///
    /// Creates a `Material` from the template and applies all configured
    /// textures and scalar parameters.
    pub fn build(self) -> Material {
        let mut material = Material::new(self.template);

        // Apply textures to their slots
        if !self.albedo.is_none() {
            material.set_texture(texture_slots::ALBEDO, self.albedo);
        }
        if !self.normal_map.is_none() {
            material.set_texture(texture_slots::NORMAL_MAP, self.normal_map);
        }
        if !self.metallic_roughness.is_none() {
            material.set_texture(texture_slots::METALLIC_ROUGHNESS, self.metallic_roughness);
        }
        if !self.emissive.is_none() {
            material.set_texture(texture_slots::EMISSIVE, self.emissive);
        }
        if !self.occlusion.is_none() {
            material.set_texture(texture_slots::OCCLUSION, self.occlusion);
        }

        // Encode scalar parameters as push constants
        let params = PbrParams {
            metallic: self.metallic,
            roughness: self.roughness,
        };
        let push_data = bytemuck::bytes_of(&params).to_vec();
        material.set_push_constants(push_data);
        material
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_gfx::handle::Handle;

    fn create_test_material_handle(index: u32) -> MaterialHandle {
        // Create a handle using the internal new method - for testing only
        // We use a transmute pattern since Handle::new is pub(crate)
        unsafe { std::mem::transmute(index) }
    }

    fn create_test_texture_handle(index: u32) -> TextureHandle {
        unsafe { std::mem::transmute(index) }
    }

    #[test]
    fn test_builder_new_defaults() {
        let template = create_test_material_handle(1);
        let builder = PbrMaterialBuilder::new(template);

        assert!(builder.albedo.is_none());
        assert!(builder.normal_map.is_none());
        assert!(builder.metallic_roughness.is_none());
        assert!(builder.emissive.is_none());
        assert!(builder.occlusion.is_none());
        assert!((builder.metallic - 0.0).abs() < f32::EPSILON);
        assert!((builder.roughness - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_builder_metal_preset() {
        let template = create_test_material_handle(1);
        let builder = PbrMaterialBuilder::metal(template);

        assert!((builder.metallic - 1.0).abs() < f32::EPSILON);
        assert!((builder.roughness - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn test_builder_plastic_preset() {
        let template = create_test_material_handle(1);
        let builder = PbrMaterialBuilder::plastic(template);

        assert!((builder.metallic - 0.0).abs() < f32::EPSILON);
        assert!((builder.roughness - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_builder_chaining() {
        let template = create_test_material_handle(1);
        let albedo = create_test_texture_handle(10);
        let normal = create_test_texture_handle(11);
        let mr = create_test_texture_handle(12);

        let builder = PbrMaterialBuilder::new(template)
            .with_albedo(albedo)
            .with_normal_map(normal)
            .with_metallic_roughness(mr)
            .with_metallic(0.8)
            .with_roughness(0.3);

        assert_eq!(builder.albedo, albedo);
        assert_eq!(builder.normal_map, normal);
        assert_eq!(builder.metallic_roughness, mr);
        assert!((builder.metallic - 0.8).abs() < f32::EPSILON);
        assert!((builder.roughness - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_metallic_clamping() {
        let template = create_test_material_handle(1);

        // Test upper bound clamping
        let builder = PbrMaterialBuilder::new(template).with_metallic(2.0);
        assert!((builder.metallic - 1.0).abs() < f32::EPSILON);

        // Test lower bound clamping
        let builder = PbrMaterialBuilder::new(template).with_metallic(-1.0);
        assert!((builder.metallic - 0.0).abs() < f32::EPSILON);

        // Test value within range
        let builder = PbrMaterialBuilder::new(template).with_metallic(0.5);
        assert!((builder.metallic - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_roughness_clamping() {
        let template = create_test_material_handle(1);

        // Test upper bound clamping
        let builder = PbrMaterialBuilder::new(template).with_roughness(1.5);
        assert!((builder.roughness - 1.0).abs() < f32::EPSILON);

        // Test lower bound clamping
        let builder = PbrMaterialBuilder::new(template).with_roughness(-0.5);
        assert!((builder.roughness - 0.0).abs() < f32::EPSILON);

        // Test value within range
        let builder = PbrMaterialBuilder::new(template).with_roughness(0.7);
        assert!((builder.roughness - 0.7).abs() < f32::EPSILON);
    }
}
