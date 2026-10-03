//! GLTF material parsing for PBR textures.
//!
//! Extracts material information from GLTF files including:
//! - Base color (albedo) texture
//! - Normal map
//! - Metallic/Roughness texture
//! - Occlusion texture
//! - Material factors (metallic, roughness, base color)

use crate::rendering::{MaterialSampling, TextureSampling, UvTransform};
use gltf::Material;

/// An image source together with the texture reference’s independent sampling policy.
#[derive(Debug, Clone, Copy)]
pub struct GltfTextureInfo {
    pub image_index: usize,
    pub sampling: TextureSampling,
}

/// Parsed material info from a GLTF material.
///
/// Contains image references, independent texture sampling settings and surface
/// factors for PBR rendering.
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

    /// Base color (albedo) image reference and sampling policy.
    pub base_color_texture: Option<GltfTextureInfo>,

    /// Normal map image reference and sampling policy.
    pub normal_texture: Option<GltfTextureInfo>,

    /// Metallic/Roughness image reference and sampling policy.
    /// In GLTF, G channel = roughness, B channel = metallic.
    pub metallic_roughness_texture: Option<GltfTextureInfo>,

    /// Occlusion image reference and sampling policy.
    pub occlusion_texture: Option<GltfTextureInfo>,

    /// Emission image reference and sampling policy.
    pub emission_texture: Option<GltfTextureInfo>,
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
    pub fn from_gltf(material: &Material) -> Result<Self, String> {
        let pbr = material.pbr_metallic_roughness();

        // Get material factors
        let base_color_factor = pbr.base_color_factor();
        let metallic_factor = pbr.metallic_factor();
        let roughness_factor = pbr.roughness_factor();
        let emission_factor = material.emissive_factor();

        let base_color_texture = pbr.base_color_texture().map(texture_info).transpose()?;
        let metallic_roughness_texture = pbr
            .metallic_roughness_texture()
            .map(texture_info)
            .transpose()?;
        let emission_texture = material.emissive_texture().map(texture_info).transpose()?;
        let normal_texture = material
            .normal_texture()
            .map(|info| {
                texture_reference(
                    info.texture(),
                    info.tex_coord(),
                    info.extension_value("KHR_texture_transform"),
                )
            })
            .transpose()?;
        let occlusion_texture = material
            .occlusion_texture()
            .map(|info| {
                texture_reference(
                    info.texture(),
                    info.tex_coord(),
                    info.extension_value("KHR_texture_transform"),
                )
            })
            .transpose()?;

        Ok(Self {
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
        })
    }

    /// Sampling policies retain texture references even when they share an image.
    pub fn sampling(&self) -> MaterialSampling {
        MaterialSampling {
            albedo: self
                .base_color_texture
                .map_or_else(Default::default, |info| info.sampling),
            normal: self
                .normal_texture
                .map_or_else(Default::default, |info| info.sampling),
            metallic_roughness: self
                .metallic_roughness_texture
                .map_or_else(Default::default, |info| info.sampling),
            occlusion: self
                .occlusion_texture
                .map_or_else(Default::default, |info| info.sampling),
            emission: self
                .emission_texture
                .map_or_else(Default::default, |info| info.sampling),
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
            parts.push(format!("albedo[{}]", idx.image_index));
        }
        if let Some(idx) = self.normal_texture {
            parts.push(format!("normal[{}]", idx.image_index));
        }
        if let Some(idx) = self.metallic_roughness_texture {
            parts.push(format!("MR[{}]", idx.image_index));
        }
        if let Some(idx) = self.occlusion_texture {
            parts.push(format!("AO[{}]", idx.image_index));
        }
        if let Some(idx) = self.emission_texture {
            parts.push(format!("emiss[{}]", idx.image_index));
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

fn texture_info(info: gltf::texture::Info<'_>) -> Result<GltfTextureInfo, String> {
    let uv = info.texture_transform().map_or(
        UvTransform {
            tex_coord: info.tex_coord(),
            ..Default::default()
        },
        |transform| UvTransform {
            tex_coord: transform.tex_coord().unwrap_or(info.tex_coord()),
            offset: transform.offset(),
            rotation: transform.rotation(),
            scale: transform.scale(),
        },
    );
    let mut result = texture_reference(info.texture(), uv.tex_coord, None)?;
    result.sampling.uv = uv;
    uv.validate().map_err(str::to_owned)?;
    Ok(result)
}

fn texture_reference(
    texture: gltf::Texture<'_>,
    tex_coord: u32,
    extension: Option<&serde_json::Value>,
) -> Result<GltfTextureInfo, String> {
    #[derive(serde::Deserialize)]
    #[serde(default, rename_all = "camelCase")]
    struct Transform {
        offset: [f32; 2],
        rotation: f32,
        scale: [f32; 2],
        tex_coord: Option<u32>,
    }
    impl Default for Transform {
        fn default() -> Self {
            Self {
                offset: [0.0; 2],
                rotation: 0.0,
                scale: [1.0; 2],
                tex_coord: None,
            }
        }
    }
    let mut uv = UvTransform {
        tex_coord,
        ..Default::default()
    };
    if let Some(extension) = extension {
        let transform: Transform = serde_json::from_value(extension.clone())
            .map_err(|error| format!("Invalid KHR_texture_transform: {error}"))?;
        uv = UvTransform {
            tex_coord: transform.tex_coord.unwrap_or(tex_coord),
            offset: transform.offset,
            rotation: transform.rotation,
            scale: transform.scale,
        };
    }
    uv.validate().map_err(str::to_owned)?;
    Ok(GltfTextureInfo {
        image_index: texture.source().index(),
        sampling: TextureSampling {
            uv,
            sampler: sampler(texture.sampler()),
        },
    })
}

fn sampler(sampler: gltf::texture::Sampler<'_>) -> katla_gfx::SamplerDescriptor {
    use gltf::texture::{MagFilter, MinFilter, WrappingMode};
    use katla_gfx::{AddressMode, FilterMode, MipFilter, SamplerDescriptor};
    let (min_filter, mip_filter) = match sampler.min_filter() {
        Some(MinFilter::Nearest) => (FilterMode::Nearest, MipFilter::None),
        Some(MinFilter::Linear) => (FilterMode::Linear, MipFilter::None),
        Some(MinFilter::NearestMipmapNearest) => (FilterMode::Nearest, MipFilter::Nearest),
        Some(MinFilter::LinearMipmapNearest) => (FilterMode::Linear, MipFilter::Nearest),
        Some(MinFilter::NearestMipmapLinear) => (FilterMode::Nearest, MipFilter::Linear),
        Some(MinFilter::LinearMipmapLinear) | None => (FilterMode::Linear, MipFilter::Linear),
    };
    let wrap = |mode| match mode {
        WrappingMode::ClampToEdge => AddressMode::ClampToEdge,
        WrappingMode::MirroredRepeat => AddressMode::MirroredRepeat,
        WrappingMode::Repeat => AddressMode::Repeat,
    };
    SamplerDescriptor {
        min_filter,
        mip_filter,
        mag_filter: match sampler.mag_filter() {
            Some(MagFilter::Nearest) => FilterMode::Nearest,
            _ => FilterMode::Linear,
        },
        address_u: wrap(sampler.wrap_s()),
        address_v: wrap(sampler.wrap_t()),
        ..SamplerDescriptor::linear_repeat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_independent_roles_preserve_uv_transforms_and_shared_image_sampler_policies() {
        let document = gltf::Gltf::from_slice(br#"{
            "asset":{"version":"2.0"},"extensionsUsed":["KHR_texture_transform"],
            "images":[{"uri":"test.png"}],
            "samplers":[{"minFilter":9986,"magFilter":9728,"wrapS":33648,"wrapT":33071},{}],
            "textures":[{"source":0,"sampler":0},{"source":0,"sampler":1}],
            "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0,"texCoord":2,"extensions":{"KHR_texture_transform":{"texCoord":1,"offset":[0.2,0.3],"rotation":0.5,"scale":[-2,3]}}},"metallicRoughnessTexture":{"index":1,"texCoord":1}},
                "normalTexture":{"index":0,"texCoord":0,"extensions":{"KHR_texture_transform":{"texCoord":1,"scale":[-1,2]}}},
                "occlusionTexture":{"index":1,"extensions":{"KHR_texture_transform":{"offset":[0.4,0.7],"rotation":1}}},
                "emissiveTexture":{"index":0,"texCoord":1}}]
        }"#).unwrap().document;
        let material = GltfMaterialInfo::from_gltf(&document.materials().next().unwrap()).unwrap();
        let sampling = material.sampling();
        assert_eq!(
            material.base_color_texture.unwrap().image_index,
            material.metallic_roughness_texture.unwrap().image_index
        );
        assert_eq!(
            sampling.albedo.uv,
            UvTransform {
                tex_coord: 1,
                offset: [0.2, 0.3],
                rotation: 0.5,
                scale: [-2., 3.]
            }
        );
        assert_eq!(sampling.normal.uv.tex_coord, 1);
        assert_eq!(sampling.normal.uv.scale, [-1., 2.]);
        assert_eq!(sampling.occlusion.uv.offset, [0.4, 0.7]);
        assert_eq!(sampling.occlusion.uv.rotation, 1.);
        assert_eq!(sampling.emission.uv.tex_coord, 1);
        assert_eq!(
            sampling.albedo.sampler.min_filter,
            katla_gfx::FilterMode::Nearest
        );
        assert_eq!(
            sampling.albedo.sampler.mag_filter,
            katla_gfx::FilterMode::Nearest
        );
        assert_eq!(
            sampling.albedo.sampler.mip_filter,
            katla_gfx::MipFilter::Linear
        );
        assert_eq!(
            sampling.albedo.sampler.address_u,
            katla_gfx::AddressMode::MirroredRepeat
        );
        assert_eq!(
            sampling.albedo.sampler.address_v,
            katla_gfx::AddressMode::ClampToEdge
        );
        assert_eq!(
            sampling.metallic_roughness.sampler,
            katla_gfx::SamplerDescriptor::linear_repeat()
        );
        sampling.validate().unwrap();
    }

    #[test]
    fn test_all_gltf_minification_filters_preserve_spatial_and_mip_choices() {
        use katla_gfx::{
            FilterMode::{Linear, Nearest},
            MipFilter,
        };
        for (min, spatial, mip) in [
            (9728, Nearest, MipFilter::None),
            (9729, Linear, MipFilter::None),
            (9984, Nearest, MipFilter::Nearest),
            (9985, Linear, MipFilter::Nearest),
            (9986, Nearest, MipFilter::Linear),
            (9987, Linear, MipFilter::Linear),
        ] {
            let document = gltf::Gltf::from_slice(
                &serde_json::to_vec(
                    &serde_json::json!({"asset":{"version":"2.0"},"samplers":[{"minFilter":min}]}),
                )
                .unwrap(),
            )
            .unwrap()
            .document;
            let policy = sampler(document.samplers().next().unwrap());
            assert_eq!((policy.min_filter, policy.mip_filter), (spatial, mip));
        }
    }

    #[test]
    fn test_default_matches_gltf_implicit_material() {
        let document = gltf::Gltf::from_slice(br#"{"asset":{"version":"2.0"},"materials":[{}]}"#)
            .unwrap()
            .document;
        let material = GltfMaterialInfo::from_gltf(&document.materials().next().unwrap()).unwrap();
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
            .map(|material| GltfMaterialInfo::from_gltf(&material).unwrap())
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
            emission_texture: Some(GltfTextureInfo {
                image_index: 0,
                sampling: Default::default(),
            }),
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
            base_color_texture: Some(GltfTextureInfo {
                image_index: 0,
                sampling: Default::default(),
            }),
            normal_texture: Some(GltfTextureInfo {
                image_index: 1,
                sampling: Default::default(),
            }),
            metallic_roughness_texture: Some(GltfTextureInfo {
                image_index: 2,
                sampling: Default::default(),
            }),
            occlusion_texture: Some(GltfTextureInfo {
                image_index: 3,
                sampling: Default::default(),
            }),
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
