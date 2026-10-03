//! Complete portable surfaces with independent editable live instances.

use crate::{
    material_images::{TextureAssignments, TextureSource},
    rendering::MaterialSampling,
};
use katla_agent::material::{MaterialPreset, MaterialValues};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Current portable material definition format.
pub const MATERIAL_VERSION: u32 = 1;

/// A complete PBR surface. Applying it creates an independent editable copy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialAsset {
    /// Must match `MATERIAL_VERSION`.
    pub version: u32,
    /// Human-readable surface label.
    pub name: String,
    /// sRGB base RGB, linear alpha/emission and PBR factors.
    pub values: MaterialValues,
    /// Independent coordinates and samplers for all image roles.
    #[serde(default)]
    pub sampling: MaterialSampling,
    /// Complete image choices; omission means neutral images in every role.
    #[serde(default = "neutral_textures")]
    pub textures: TextureAssignments,
}
fn neutral_textures() -> TextureAssignments {
    TextureAssignments {
        albedo: TextureSource::Neutral,
        normal: TextureSource::Neutral,
        metallic_roughness: TextureSource::Neutral,
        occlusion: TextureSource::Neutral,
        emission: TextureSource::Neutral,
    }
}
impl Default for MaterialAsset {
    fn default() -> Self {
        Self {
            version: MATERIAL_VERSION,
            name: "Plaster".into(),
            values: MaterialPreset::Plaster.values(),
            sampling: MaterialSampling::default(),
            textures: neutral_textures(),
        }
    }
}
impl MaterialAsset {
    /// Validate the format and all authored factors, roots and sampling policies.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != MATERIAL_VERSION {
            return Err(format!(
                "Unsupported material version {}; expected {MATERIAL_VERSION}",
                self.version
            ));
        }
        if self.name.trim().is_empty() || self.name.len() > 256 || self.name.contains('\0') {
            return Err("Material name must contain 1..256 bytes without NUL".into());
        }
        self.values.validate()?;
        self.sampling.validate().map_err(str::to_owned)?;
        self.textures.validate()?;
        if self
            .textures
            .roles()
            .iter()
            .any(|source| matches!(source, TextureSource::Inherit))
        {
            return Err("Reusable materials require an explicit image or neutral for all five roles; capture resolves inherited bindings".into());
        }
        Ok(())
    }
    /// Parse bounded strict RON without allocating GPU resources.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > crate::util::asset_io::MAX_ASSET_BYTES {
            return Err("Material file exceeds 64 MiB".into());
        }
        let asset: Self = ron::from_str(text).map_err(|error| error.to_string())?;
        asset.validate()?;
        Ok(asset)
    }
    /// Read and validate a material definition without changing live objects.
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::parse(&crate::util::asset_io::read_text(path)?)
    }
    /// Atomically publish validated RON; image existence is checked by authoring control.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let text = ron::ser::to_string_pretty(self, crate::scene::ron_pretty_config())
            .map_err(|error| error.to_string())?;
        crate::util::asset_io::write_text(path, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_material_round_trip_defaults_and_strict_validation() {
        let mut asset = MaterialAsset::default();
        asset.textures.normal = TextureSource::GltfImage {
            asset: crate::scene::AssetRef::Resource("models/example.glb".into()),
            image_index: 2,
        };
        let text = ron::ser::to_string_pretty(&asset, crate::scene::ron_pretty_config()).unwrap();
        assert_eq!(MaterialAsset::parse(&text).unwrap(), asset);
        let mut json = serde_json::to_value(&asset).unwrap();
        json.as_object_mut().unwrap().remove("textures");
        assert_eq!(
            serde_json::from_value::<MaterialAsset>(json.clone())
                .unwrap()
                .textures,
            neutral_textures()
        );
        json["shader"] = serde_json::json!("pbr");
        assert!(serde_json::from_value::<MaterialAsset>(json).is_err());
        asset.values.emissive_factor = [4., 1., 0.];
        assert!(asset.validate().is_ok());
        asset.sampling.normal.uv.rotation = f32::NAN;
        assert!(asset.validate().is_err());
    }
}
