//! GLTF material parsing for PBR textures.
//!
//! Extracts material information from GLTF files including:
//! - Base color (albedo) texture
//! - Normal map
//! - Metallic/Roughness texture
//! - Occlusion texture
//! - Material factors (metallic, roughness, base color)

use gltf::Material;

/// Parsed material info from a GLTF material.
///
/// Contains texture indices (pointing to the GLTF images array)
/// and material factors for PBR rendering.
#[derive(Debug, Clone)]
pub struct GltfMaterialInfo {
    /// Base color factor (RGBA multiplier).
    pub base_color_factor: [f32; 4],

    /// Metallic factor (0.0 = dielectric, 1.0 = metal).
    pub metallic_factor: f32,

    /// Roughness factor (0.0 = smooth, 1.0 = rough).
    pub roughness_factor: f32,

    /// Emission factor (RGB multiplier for emission texture).
    pub emission_factor: [f32; 3],
    /// Tangent-space normal X/Y multiplier.
    pub normal_scale: f32,
    /// Occlusion texture influence on ambient lighting.
    pub occlusion_strength: f32,
    /// Coverage policy, mask threshold and two-sided rasterization.
    pub alpha_mode: crate::rendering::AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,

    /// Base color (albedo) texture index in GLTF images array.
    pub base_color_texture: Option<usize>,

    /// Normal map texture index in GLTF images array.
    pub normal_texture: Option<usize>,

    /// Metallic/Roughness texture index in GLTF images array.
    /// In GLTF, G channel = roughness, B channel = metallic.
    pub metallic_roughness_texture: Option<usize>,

    /// Occlusion texture index in GLTF images array.
    pub occlusion_texture: Option<usize>,

    /// Emission texture index in GLTF images array.
    pub emission_texture: Option<usize>,
}

impl Default for GltfMaterialInfo {
    fn default() -> Self {
        Self {
            base_color_factor: [1.0; 4],
            metallic_factor: 1.0,
            roughness_factor: 1.0,
            emission_factor: [0.0; 3],
            normal_scale: 1.0,
            occlusion_strength: 1.0,
            alpha_mode: crate::rendering::AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            base_color_texture: None,
            normal_texture: None,
            metallic_roughness_texture: None,
            occlusion_texture: None,
            emission_texture: None,
        }
    }
}

impl GltfMaterialInfo {
    /// Parse material info from a GLTF material.
    ///
    /// Extracts all PBR-relevant information from the GLTF material,
    /// including texture indices and material factors.
    pub fn from_gltf(material: &Material) -> Self {
        let pbr = material.pbr_metallic_roughness();

        // Get material factors
        let base_color_factor = pbr.base_color_factor();
        let metallic_factor = pbr.metallic_factor();
        let roughness_factor = pbr.roughness_factor();
        let emission_factor = material.emissive_factor();

        // Get texture indices
        // Note: gltf crate returns texture index, we need to convert to image index
        let base_color_texture = pbr
            .base_color_texture()
            .map(|info| info.texture().source().index());

        let normal_texture = material
            .normal_texture()
            .map(|info| info.texture().source().index());

        let metallic_roughness_texture = pbr
            .metallic_roughness_texture()
            .map(|info| info.texture().source().index());

        let occlusion_texture = material
            .occlusion_texture()
            .map(|info| info.texture().source().index());

        let emission_texture = material
            .emissive_texture()
            .map(|info| info.texture().source().index());

        Self {
            base_color_factor,
            metallic_factor,
            roughness_factor,
            emission_factor,
            normal_scale: material.normal_texture().map_or(1.0, |info| info.scale()),
            occlusion_strength: material
                .occlusion_texture()
                .map_or(1.0, |info| info.strength()),
            alpha_mode: match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => crate::rendering::AlphaMode::Opaque,
                gltf::material::AlphaMode::Mask => crate::rendering::AlphaMode::Mask,
                gltf::material::AlphaMode::Blend => crate::rendering::AlphaMode::Blend,
            },
            alpha_cutoff: material.alpha_cutoff().unwrap_or(0.5),
            double_sided: material.double_sided(),
            base_color_texture,
            normal_texture,
            metallic_roughness_texture,
            occlusion_texture,
            emission_texture,
        }
    }

    /// Check if this material has any PBR textures.
    pub fn has_textures(&self) -> bool {
        self.base_color_texture.is_some()
            || self.normal_texture.is_some()
            || self.metallic_roughness_texture.is_some()
            || self.occlusion_texture.is_some()
            || self.emission_texture.is_some()
    }

    /// Check if this material has "enhanced" PBR textures beyond just albedo.
    /// Returns true if it has normal, MR, or AO textures.
    pub fn has_enhanced_pbr(&self) -> bool {
        self.normal_texture.is_some()
            || self.metallic_roughness_texture.is_some()
            || self.occlusion_texture.is_some()
            || self.emission_texture.is_some()
    }

    /// Get a summary of the material for logging.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if let Some(idx) = self.base_color_texture {
            parts.push(format!("albedo[{}]", idx));
        }
        if let Some(idx) = self.normal_texture {
            parts.push(format!("normal[{}]", idx));
        }
        if let Some(idx) = self.metallic_roughness_texture {
            parts.push(format!("MR[{}]", idx));
        }
        if let Some(idx) = self.occlusion_texture {
            parts.push(format!("AO[{}]", idx));
        }
        if let Some(idx) = self.emission_texture {
            parts.push(format!("emiss[{}]", idx));
        }

        if parts.is_empty() {
            format!(
                "no textures (M={:.2}, R={:.2})",
                self.metallic_factor, self.roughness_factor
            )
        } else {
            format!(
                "{} (M={:.2}, R={:.2})",
                parts.join(", "),
                self.metallic_factor,
                self.roughness_factor
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_matches_gltf_implicit_material() {
        let document = gltf::Gltf::from_slice(br#"{"asset":{"version":"2.0"},"materials":[{}]}"#)
            .unwrap()
            .document;
        let material = GltfMaterialInfo::from_gltf(&document.materials().next().unwrap());
        let default = GltfMaterialInfo::default();
        assert_eq!(default.base_color_factor, material.base_color_factor);
        assert_eq!(default.metallic_factor, material.metallic_factor);
        assert_eq!(default.roughness_factor, material.roughness_factor);
        assert_eq!(default.emission_factor, material.emission_factor);
    }

    #[test]
    fn test_gltf_coverage_preserves_modes_cutoff_and_double_sided() {
        let document = gltf::Gltf::from_slice(br#"{"asset":{"version":"2.0"},"materials":[{}, {"alphaMode":"MASK", "alphaCutoff":0.25, "doubleSided":true}, {"alphaMode":"BLEND"}]}"#).unwrap().document;
        let materials: Vec<_> = document
            .materials()
            .map(|material| GltfMaterialInfo::from_gltf(&material))
            .collect();
        assert_eq!(materials[0].alpha_mode, crate::rendering::AlphaMode::Opaque);
        assert_eq!(materials[0].alpha_cutoff, 0.5);
        assert!(!materials[0].double_sided);
        assert_eq!(materials[1].alpha_mode, crate::rendering::AlphaMode::Mask);
        assert_eq!(materials[1].alpha_cutoff, 0.25);
        assert!(materials[1].double_sided);
        assert_eq!(materials[2].alpha_mode, crate::rendering::AlphaMode::Blend);
    }

    #[test]
    fn test_emission_only_material_has_textures() {
        let material = GltfMaterialInfo {
            emission_texture: Some(0),
            ..Default::default()
        };
        assert!(material.has_textures());
        assert!(material.has_enhanced_pbr());
    }

    #[test]
    fn test_summary_no_textures() {
        let info = GltfMaterialInfo {
            base_color_factor: [1.0, 1.0, 1.0, 1.0],
            metallic_factor: 0.5,
            roughness_factor: 0.3,
            ..Default::default()
        };
        let summary = info.summary();
        assert!(summary.contains("no textures"));
        assert!(summary.contains("M=0.50"));
        assert!(summary.contains("R=0.30"));
    }

    #[test]
    fn test_summary_with_textures() {
        let info = GltfMaterialInfo {
            base_color_texture: Some(0),
            normal_texture: Some(1),
            metallic_roughness_texture: Some(2),
            occlusion_texture: Some(3),
            metallic_factor: 1.0,
            roughness_factor: 0.5,
            ..Default::default()
        };
        let summary = info.summary();
        assert!(summary.contains("albedo[0]"));
        assert!(summary.contains("normal[1]"));
        assert!(summary.contains("MR[2]"));
        assert!(summary.contains("AO[3]"));
        assert!(info.has_textures());
    }
}
