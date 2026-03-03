//! GLTF material bridge for converting GLTF materials to MaterialInstance.
//!
//! This module provides utilities to bridge GLTF material data with Katla's
//! material system.

use katla_gfx::handle::{MaterialHandle, TextureHandle};
use katla_gfx::MaterialInstance;

use crate::util::gltf_material::GltfMaterialInfo;

use super::builder::PbrMaterialBuilder;

/// Create a MaterialInstance from GLTF material info with a texture array.
///
/// This function bridges the parsed GLTF material data with Katla's material
/// system, creating a properly configured MaterialInstance.
///
/// # Arguments
/// * `template` - The material template handle to use
/// * `gltf_info` - The parsed GLTF material information
/// * `textures` - Slice of texture handles indexed by GLTF texture index
///
/// # Returns
/// A configured MaterialInstance ready for registration with the renderer.
pub fn create_material_from_gltf(
    template: MaterialHandle,
    gltf_info: &GltfMaterialInfo,
    textures: &[TextureHandle],
) -> MaterialInstance {
    let mut builder = PbrMaterialBuilder::new(template)
        .with_metallic(gltf_info.metallic_factor)
        .with_roughness(gltf_info.roughness_factor);

    // Resolve textures from GLTF indices to handles
    if let Some(tex_idx) = gltf_info.base_color_texture {
        if let Some(&handle) = textures.get(tex_idx) {
            builder = builder.with_albedo(handle);
        }
    }

    if let Some(tex_idx) = gltf_info.normal_texture {
        if let Some(&handle) = textures.get(tex_idx) {
            builder = builder.with_normal_map(handle);
        }
    }

    if let Some(tex_idx) = gltf_info.metallic_roughness_texture {
        if let Some(&handle) = textures.get(tex_idx) {
            builder = builder.with_metallic_roughness(handle);
        }
    }

    if let Some(tex_idx) = gltf_info.occlusion_texture {
        if let Some(&handle) = textures.get(tex_idx) {
            builder = builder.with_occlusion(handle);
        }
    }

    if let Some(tex_idx) = gltf_info.emission_texture {
        if let Some(&handle) = textures.get(tex_idx) {
            builder = builder.with_emissive(handle);
        }
    }

    builder.build()
}

/// Material params extracted from GLTF for use with DrawCall.
#[derive(Clone, Copy, Debug)]
pub struct GltfMaterialParams {
    /// Metallic factor (0.0 = dielectric, 1.0 = metal).
    pub metallic: f32,
    /// Roughness factor (0.0 = smooth, 1.0 = rough).
    pub roughness: f32,
    /// Ambient occlusion factor (usually 1.0 if no AO texture).
    pub ao: f32,
}

impl GltfMaterialParams {
    /// Extract material params from GLTF material info.
    pub fn from_gltf_info(info: &GltfMaterialInfo) -> Self {
        Self {
            metallic: info.metallic_factor,
            roughness: info.roughness_factor,
            ao: 1.0, // GLTF doesn't have a scalar AO factor
        }
    }

    /// Convert to array format for DrawCall::with_material_params.
    pub fn to_array(self) -> [f32; 4] {
        [self.metallic, self.roughness, self.ao, 0.0]
    }
}

impl From<&GltfMaterialInfo> for GltfMaterialParams {
    fn from(info: &GltfMaterialInfo) -> Self {
        Self::from_gltf_info(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_material_handle(index: u32) -> MaterialHandle {
        unsafe { std::mem::transmute(index) }
    }

    fn create_test_texture_handle(index: u32) -> TextureHandle {
        unsafe { std::mem::transmute(index) }
    }

    #[test]
    fn test_gltf_material_params() {
        let info = GltfMaterialInfo {
            metallic_factor: 0.8,
            roughness_factor: 0.3,
            ..Default::default()
        };

        let params = GltfMaterialParams::from_gltf_info(&info);
        assert!((params.metallic - 0.8).abs() < f32::EPSILON);
        assert!((params.roughness - 0.3).abs() < f32::EPSILON);
        assert!((params.ao - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gltf_material_params_to_array() {
        let params = GltfMaterialParams {
            metallic: 0.5,
            roughness: 0.7,
            ao: 1.0,
        };
        let arr = params.to_array();
        assert_eq!(arr, [0.5, 0.7, 1.0, 0.0]);
    }

    #[test]
    fn test_create_material_from_gltf_no_textures() {
        let template = create_test_material_handle(1);
        let info = GltfMaterialInfo {
            metallic_factor: 0.5,
            roughness_factor: 0.3,
            ..Default::default()
        };

        let textures: &[TextureHandle] = &[];
        let material = create_material_from_gltf(template, &info, textures);
        assert_eq!(material.template(), template);
    }

    #[test]
    fn test_create_material_from_gltf_with_textures() {
        let template = create_test_material_handle(1);
        let albedo = create_test_texture_handle(10);
        let normal = create_test_texture_handle(11);

        let textures = [albedo, normal];

        let info = GltfMaterialInfo {
            base_color_texture: Some(0),
            normal_texture: Some(1),
            metallic_factor: 0.5,
            roughness_factor: 0.3,
            ..Default::default()
        };

        let material = create_material_from_gltf(template, &info, &textures);
        assert_eq!(material.template(), template);
    }
}
