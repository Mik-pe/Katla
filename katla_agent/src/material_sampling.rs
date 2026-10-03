//! GPU-independent material sampling requests and authoring units.

use serde::{Deserialize, Serialize};

/// Material image roles, independent of native handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TextureRole {
    Albedo,
    Normal,
    MetallicRoughness,
    Occlusion,
    Emission,
}

impl TextureRole {
    /// Stable role order used in inspection and material sampling rows.
    pub const ALL: [Self; 5] = [
        Self::Albedo,
        Self::Normal,
        Self::MetallicRoughness,
        Self::Occlusion,
        Self::Emission,
    ];
    /// Index into the ordered material role list.
    pub const fn index(self) -> usize {
        match self {
            Self::Albedo => 0,
            Self::Normal => 1,
            Self::MetallicRoughness => 2,
            Self::Occlusion => 3,
            Self::Emission => 4,
        }
    }
    /// JSON role name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Albedo => "albedo",
            Self::Normal => "normal",
            Self::MetallicRoughness => "metallic_roughness",
            Self::Occlusion => "occlusion",
            Self::Emission => "emission",
        }
    }
}

/// Spatial and mip filtering combined into one authoring choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Minification {
    Nearest,
    Linear,
    NearestMipmapNearest,
    LinearMipmapNearest,
    NearestMipmapLinear,
    LinearMipmapLinear,
}

/// Filtering within the magnified texture level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Magnification {
    Nearest,
    Linear,
}

/// Behavior outside the normalized UV interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TextureWrap {
    Repeat,
    ClampToEdge,
    MirroredRepeat,
}

/// Patch one role while preserving all omitted properties and image bindings.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SamplingPatch {
    /// Select UV0 or UV1 from the target mesh.
    pub tex_coord: Option<u32>,
    /// Translation after scale and rotation, in UV units.
    pub offset: Option<[f32; 2]>,
    /// Rotation in radians.
    pub rotation: Option<f32>,
    /// Independent axis scales; negative and zero values are legal.
    pub scale: Option<[f32; 2]>,
    /// Spatial and mip filtering for minification.
    pub minification: Option<Minification>,
    /// Spatial filtering for magnification.
    pub magnification: Option<Magnification>,
    /// Addressing on the U axis.
    pub wrap_u: Option<TextureWrap>,
    /// Addressing on the V axis.
    pub wrap_v: Option<TextureWrap>,
    /// Requested anisotropy in 1..16, clamped to the device maximum.
    pub anisotropy: Option<u8>,
}

impl SamplingPatch {
    /// True when the request contains no editable property.
    pub fn is_empty(self) -> bool {
        self.tex_coord.is_none()
            && self.offset.is_none()
            && self.rotation.is_none()
            && self.scale.is_none()
            && self.minification.is_none()
            && self.magnification.is_none()
            && self.wrap_u.is_none()
            && self.wrap_v.is_none()
            && self.anisotropy.is_none()
    }

    pub(crate) fn tool_schema() -> serde_json::Value {
        use serde_json::json;
        let properties = json!({
            "tex_coord":{"type":"integer","minimum":0,"maximum":1,"description":"Use an available UV set: 0 or 1; inspect reports mesh availability"},
            "offset":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2,"description":"UV translation after scale and rotation"},
            "rotation":{"type":"number","description":"UV rotation in radians"},
            "scale":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2,"description":"Independent UV axis scales; negative mirrors and zero collapses an axis"},
            "minification":{"type":"string","enum":["nearest","linear","nearest_mipmap_nearest","linear_mipmap_nearest","nearest_mipmap_linear","linear_mipmap_linear"],"description":"Nearest/linear without a mip suffix restrict sampling to level zero"},
            "magnification":{"type":"string","enum":["nearest","linear"]},
            "wrap_u":{"type":"string","enum":["repeat","clamp_to_edge","mirrored_repeat"]},
            "wrap_v":{"type":"string","enum":["repeat","clamp_to_edge","mirrored_repeat"]},
            "anisotropy":{"type":"integer","minimum":1,"maximum":16,"description":"1 disables anisotropy; higher values require linear minification and magnification"}
        });
        let required_any: Vec<_> = properties
            .as_object()
            .into_iter()
            .flat_map(|fields| fields.keys())
            .map(|field| json!({"required":[field]}))
            .collect();
        json!({"type":"object","properties":properties,"additionalProperties":false,"anyOf":required_any})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_sampling_schema_rejects_unknown_fields_and_matches_discovery_actions() {
        let op: crate::material::MaterialOp = serde_json::from_value(serde_json::json!({"action":"set_sampling","entity_ids":["4294967297"],"role":"albedo","patch":{"rotation":1.5,"minification":"linear_mipmap_linear"}})).unwrap();
        assert!(matches!(
            op,
            crate::material::MaterialOp::SetSampling {
                role: TextureRole::Albedo,
                ..
            }
        ));
        assert!(
            serde_json::from_value::<SamplingPatch>(serde_json::json!({"rotatoin":1})).is_err()
        );
        assert!(serde_json::from_value::<crate::material::MaterialOp>(serde_json::json!({"action":"set_sampling","entity_ids":["1"],"role":"color","patch":{"rotation":1}})).is_err());
        let schema = crate::material::MaterialOp::tool_schema();
        let branches = schema["oneOf"].as_array().unwrap();
        let sampling = branches
            .iter()
            .find(|branch| branch["properties"]["action"]["const"] == "set_sampling")
            .unwrap();
        assert_eq!(
            sampling["required"],
            serde_json::json!(["action", "entity_ids", "role", "patch"])
        );
        assert_eq!(
            sampling["properties"]["patch"]["additionalProperties"],
            false
        );
        assert_eq!(
            sampling["properties"]["patch"]["anyOf"]
                .as_array()
                .unwrap()
                .len(),
            9
        );
        assert!(SamplingPatch::default().is_empty());
    }
}
