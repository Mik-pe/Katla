//! Material authoring requests shared by the editor and external agents.

use serde::{Deserialize, Serialize};

/// Surface coverage and compositing policy.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum AlphaMode {
    /// Full coverage; texture and factor alpha do not reduce coverage.
    #[default]
    Opaque,
    /// Reject fragments whose sampled alpha times base alpha is below the cutoff.
    Mask,
    /// Composite straight alpha over opaque geometry without writing scene depth.
    Blend,
}

/// Per-object PBR factors. Base color is sRGB; GPU conversion belongs to the app.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MaterialValues {
    /// Base color multiplier [red, green, blue, alpha], each in 0..=1.
    pub base_color: [f32; 4],
    /// Zero is dielectric, one is metal.
    pub metallic: f32,
    /// Zero is polished, one is matte.
    pub roughness: f32,
    /// Ambient occlusion multiplier; one leaves the surface unoccluded.
    pub ao: f32,
    /// Linear RGB emission multiplier, including values above one for HDR emission.
    pub emissive_factor: [f32; 3],
    /// Tangent-space normal X/Y multiplier.
    pub normal_scale: f32,
    /// Occlusion texture influence on ambient light, in 0..1.
    pub occlusion_strength: f32,
    /// Opaque, cutoff mask, or sorted alpha compositing.
    pub alpha_mode: AlphaMode,
    /// Alpha threshold for masked surfaces; values above one reject all coverage.
    pub alpha_cutoff: f32,
    /// Render both sides and reverse back-face shading normals.
    pub double_sided: bool,
}

impl MaterialValues {
    /// Reject invalid factors before changing any object.
    pub fn validate(&self) -> Result<(), String> {
        for (index, value) in self.base_color.into_iter().enumerate() {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(format!(
                    "base_color[{index}] must be finite and within 0..=1; received {value}"
                ));
            }
        }
        for (field, value) in [
            ("metallic", self.metallic),
            ("roughness", self.roughness),
            ("ao", self.ao),
            ("occlusion_strength", self.occlusion_strength),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(format!(
                    "{field} must be finite and within 0..=1; received {value}"
                ));
            }
        }
        for (index, value) in self.emissive_factor.into_iter().enumerate() {
            if !value.is_finite() || value < 0.0 {
                return Err(format!(
                    "emissive_factor[{index}] must be finite nonnegative linear RGB; received {value}"
                ));
            }
        }
        if !self.normal_scale.is_finite() {
            return Err("normal_scale must be finite".into());
        }
        if !self.alpha_cutoff.is_finite() || self.alpha_cutoff < 0.0 {
            return Err(format!(
                "alpha_cutoff must be finite and nonnegative; received {}",
                self.alpha_cutoff
            ));
        }
        Ok(())
    }
}

/// Named starting points for room surfaces and props.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MaterialPreset {
    Plaster,
    Oak,
    Concrete,
    Ceramic,
    BrushedMetal,
    Fabric,
}

impl MaterialPreset {
    /// The shared editor and agent preset library.
    pub const ALL: [Self; 6] = [
        Self::Plaster,
        Self::Oak,
        Self::Concrete,
        Self::Ceramic,
        Self::BrushedMetal,
        Self::Fabric,
    ];

    /// Display label used in the inspector.
    pub fn label(self) -> &'static str {
        match self {
            Self::Plaster => "Plaster",
            Self::Oak => "Oak",
            Self::Concrete => "Concrete",
            Self::Ceramic => "Ceramic",
            Self::BrushedMetal => "Brushed metal",
            Self::Fabric => "Fabric",
        }
    }

    /// Flat PBR factors; presets do not install textures or alter mesh geometry.
    pub fn values(self) -> MaterialValues {
        let (rgb, metallic, roughness) = match self {
            Self::Plaster => ([0.88, 0.85, 0.79], 0.0, 0.9),
            Self::Oak => ([0.55, 0.35, 0.18], 0.0, 0.6),
            Self::Concrete => ([0.48, 0.49, 0.5], 0.0, 0.95),
            Self::Ceramic => ([0.9, 0.93, 0.94], 0.0, 0.18),
            Self::BrushedMetal => ([0.68, 0.72, 0.76], 1.0, 0.32),
            Self::Fabric => ([0.28, 0.38, 0.42], 0.0, 1.0),
        };
        MaterialValues {
            base_color: [rgb[0], rgb[1], rgb[2], 1.0],
            metallic,
            roughness,
            ao: 1.0,
            emissive_factor: [0.0; 3],
            normal_scale: 1.0,
            occlusion_strength: 1.0,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
        }
    }
}

/// Discover, inspect or edit rendered objects. IDs are decimal generational strings.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaterialOp {
    /// Patch one texture role’s UV transform and sampler as one undoable batch.
    SetSampling {
        entity_ids: Vec<String>,
        role: crate::material_sampling::TextureRole,
        patch: crate::material_sampling::SamplingPatch,
    },
    /// List named presets and their PBR factors.
    Presets,
    /// Read material factors on one object.
    Inspect { entity_id: String },
    /// Patch any subset of factors on up to 256 objects as one undoable edit.
    Set {
        entity_ids: Vec<String>,
        #[serde(default)]
        preset: Option<MaterialPreset>,
        #[serde(default)]
        base_color: Option<[f32; 4]>,
        #[serde(default)]
        metallic: Option<f32>,
        #[serde(default)]
        roughness: Option<f32>,
        #[serde(default)]
        ao: Option<f32>,
        #[serde(default)]
        emissive_factor: Option<[f32; 3]>,
        #[serde(default)]
        normal_scale: Option<f32>,
        #[serde(default)]
        occlusion_strength: Option<f32>,
        #[serde(default)]
        alpha_mode: Option<AlphaMode>,
        #[serde(default)]
        alpha_cutoff: Option<f32>,
        #[serde(default)]
        double_sided: Option<bool>,
    },
}

impl MaterialOp {
    /// Shared input contract for MCP and the in-editor co-creator.
    pub fn tool_schema() -> serde_json::Value {
        serde_json::Value::Object(Self::schema_object())
    }

    pub(crate) fn schema_object() -> serde_json::Map<String, serde_json::Value> {
        use serde_json::json;
        let properties = serde_json::Map::from_iter([
            (
                "action".into(),
                json!({"type":"string","enum":["presets","inspect","set","set_sampling"]}),
            ),
            (
                "entity_id".into(),
                json!({"type":"string","pattern":"^[0-9]+$","description":"Decimal generational ID returned by scene queries; required for inspect"}),
            ),
            (
                "entity_ids".into(),
                json!({"type":"array","items":{"type":"string","pattern":"^[0-9]+$"},"minItems":1,"maxItems":256,"uniqueItems":true}),
            ),
            (
                "preset".into(),
                json!({"type":"string","enum":["plaster","oak","concrete","ceramic","brushed_metal","fabric"],"description":"Isotropic PBR factor preset; textures and directional brushing are not installed"}),
            ),
            (
                "base_color".into(),
                json!({"type":"array","items":{"type":"number","minimum":0,"maximum":1},"minItems":4,"maxItems":4,"description":"sRGB RGB and linear alpha multiplier; alpha does not switch render mode"}),
            ),
            (
                "metallic".into(),
                json!({"type":"number","minimum":0,"maximum":1}),
            ),
            (
                "roughness".into(),
                json!({"type":"number","minimum":0,"maximum":1}),
            ),
            (
                "ao".into(),
                json!({"type":"number","minimum":0,"maximum":1}),
            ),
            (
                "emissive_factor".into(),
                json!({"type":"array","items":{"type":"number","minimum":0},"minItems":3,"maxItems":3,"description":"Linear RGB self-emission; values above 1 produce HDR emission; works without an emissive texture"}),
            ),
            (
                "alpha_mode".into(),
                json!({"type":"string","enum":["opaque","mask","blend"],"description":"Opaque ignores alpha; mask rejects sampled alpha below alpha_cutoff; blend composites back-to-front without depth writes"}),
            ),
            (
                "alpha_cutoff".into(),
                json!({"type":"number","minimum":0,"description":"Mask coverage threshold, default 0.5; values above 1 hide the entire masked surface"}),
            ),
            (
                "double_sided".into(),
                json!({"type":"boolean","description":"Render both faces with reversed back-face shading normals"}),
            ),
            (
                "normal_scale".into(),
                json!({"type":"number","description":"Finite tangent-space normal X/Y multiplier; 0 flattens the normal map, negative values invert both tangent axes; typical range 0..2"}),
            ),
            (
                "occlusion_strength".into(),
                json!({"type":"number","minimum":0,"maximum":1,"description":"Occlusion texture influence on ambient light; 0 ignores the texture"}),
            ),
        ]);
        let mut properties = properties;
        properties.insert("role".into(), json!({"type":"string","enum":["albedo","normal","metallic_roughness","occlusion","emission"]}));
        properties.insert(
            "patch".into(),
            crate::material_sampling::SamplingPatch::tool_schema(),
        );
        let branches: Vec<_> = [
            ("set_sampling",vec!["action","entity_ids","role","patch"],vec!["action","entity_ids","role","patch"]),
            ("presets", vec!["action"], vec!["action"]),
            ("inspect", vec!["action", "entity_id"], vec!["action", "entity_id"]),
            ("set", vec!["action", "entity_ids", "preset", "base_color", "metallic", "roughness", "ao", "emissive_factor", "normal_scale", "occlusion_strength", "alpha_mode", "alpha_cutoff", "double_sided"], vec!["action", "entity_ids"]),
        ].into_iter().map(|(action, allowed, required)| {
            let mut fields: serde_json::Map<_, _> = properties.iter()
                .filter(|(name, _)| allowed.contains(&name.as_str()))
                .map(|(name, value)| (name.clone(), value.clone())).collect();
            fields.insert("action".into(), json!({"const":action}));
            let mut branch = json!({"type":"object","properties":fields,"required":required,"additionalProperties":false});
            if action == "set" {
                branch["anyOf"] = json!(allowed.iter().filter(|name| !["action", "entity_ids"].contains(name)).map(|name| json!({"required":[name]})).collect::<Vec<_>>());
            }
            branch
        }).collect();
        serde_json::Map::from_iter([
            ("type".into(), json!("object")),
            ("properties".into(), json!(properties)),
            ("required".into(), json!(["action"])),
            ("additionalProperties".into(), json!(false)),
            ("oneOf".into(), json!(branches)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_presets_and_invalid_factors() {
        for preset in MaterialPreset::ALL {
            assert!(preset.values().validate().is_ok());
        }
        let mut values = MaterialPreset::Oak.values();
        values.roughness = f32::NAN;
        assert!(values.validate().is_err());
        values.roughness = 1.1;
        assert!(values.validate().is_err());
    }
    #[test]
    fn test_material_request_rejects_unknown_fields() {
        assert!(
            serde_json::from_value::<MaterialOp>(
                serde_json::json!({"action":"set","entity_ids":["1"],"roughnes":0.5})
            )
            .is_err()
        );
    }
}
