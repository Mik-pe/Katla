//! Portable scene surface properties and their application-owned shader layout.

use serde::{Deserialize, Serialize};

/// Surface multipliers applied after sampling the corresponding material textures.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaterialSurface {
    /// Linear RGB emission; a missing emissive texture samples white.
    pub emissive_factor: [f32; 3],
    /// Multiplier for the decoded tangent-space normal's X and Y components.
    pub normal_scale: f32,
    /// Blend from unoccluded ambient lighting to the sampled occlusion value.
    pub occlusion_strength: f32,
}

impl Default for MaterialSurface {
    fn default() -> Self {
        Self {
            emissive_factor: [0.0; 3],
            normal_scale: 1.0,
            occlusion_strength: 1.0,
        }
    }
}

impl MaterialSurface {
    /// Reject nonfinite properties and values outside their portable authoring ranges.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self
            .emissive_factor
            .into_iter()
            .all(|value| value.is_finite() && value >= 0.0)
        {
            return Err("emissive_factor must contain finite nonnegative linear RGB values");
        }
        if !self.normal_scale.is_finite() {
            return Err("normal_scale must be finite");
        }
        if !self.occlusion_strength.is_finite() || !(0.0..=1.0).contains(&self.occlusion_strength) {
            return Err("occlusion_strength must be finite and within 0..1");
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SurfaceParameters {
    pub(crate) emissive: [f32; 4],
    pub(crate) normal_occlusion: [f32; 4],
}

impl From<MaterialSurface> for SurfaceParameters {
    fn from(surface: MaterialSurface) -> Self {
        Self {
            emissive: [
                surface.emissive_factor[0],
                surface.emissive_factor[1],
                surface.emissive_factor[2],
                0.0,
            ],
            normal_occlusion: [surface.normal_scale, surface.occlusion_strength, 0.0, 0.0],
        }
    }
}

impl Default for SurfaceParameters {
    fn default() -> Self {
        MaterialSurface::default().into()
    }
}
