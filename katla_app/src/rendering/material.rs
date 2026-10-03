//! Portable scene surface properties and their application-owned shader layout.

pub use katla_agent::material::AlphaMode;
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
    /// Coverage and compositing policy, independent of the base color alpha.
    pub alpha_mode: AlphaMode,
    /// Sampled alpha threshold for masked surfaces.
    pub alpha_cutoff: f32,
    /// Render both sides with back-face shading normals reversed.
    pub double_sided: bool,
}

impl Default for MaterialSurface {
    fn default() -> Self {
        Self {
            emissive_factor: [0.0; 3],
            normal_scale: 1.0,
            occlusion_strength: 1.0,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
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
        if !self.alpha_cutoff.is_finite() || self.alpha_cutoff < 0.0 {
            return Err("alpha_cutoff must be finite and nonnegative");
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SurfaceParameters {
    pub(crate) emissive: [f32; 4],
    pub(crate) normal_occlusion: [f32; 4],
    pub(crate) coverage: [f32; 4],
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
            coverage: [
                surface.alpha_cutoff,
                match surface.alpha_mode {
                    AlphaMode::Opaque => 0.0,
                    AlphaMode::Mask => 1.0,
                    AlphaMode::Blend => 2.0,
                },
                f32::from(surface.double_sided),
                0.0,
            ],
        }
    }
}

impl SurfaceParameters {
    pub(crate) fn transparent(self) -> bool {
        self.coverage[1] > 1.5
    }

    pub(crate) fn cull_mode(self) -> katla_gfx::CullMode {
        if self.coverage[2] > 0.5 {
            katla_gfx::CullMode::None
        } else if self.coverage[3] > 0.5 {
            katla_gfx::CullMode::Front
        } else {
            katla_gfx::CullMode::Back
        }
    }
}

#[inline]
pub(crate) fn mirrored_transform(matrix: &[f32; 16]) -> bool {
    let x = katla_math::Vec3::new(matrix[0], matrix[1], matrix[2]);
    let y = katla_math::Vec3::new(matrix[4], matrix[5], matrix[6]);
    let z = katla_math::Vec3::new(matrix[8], matrix[9], matrix[10]);
    x.dot(y.cross(z)) < 0.0
}

impl Default for SurfaceParameters {
    fn default() -> Self {
        MaterialSurface::default().into()
    }
}
