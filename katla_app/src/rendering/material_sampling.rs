//! Independent texture coordinates and sampling policies for scene material roles.

use katla_gfx::SamplerDescriptor;
use serde::{Deserialize, Serialize};

/// UV selection followed by scale, rotation in radians and translation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UvTransform {
    /// Coordinate set: zero or one.
    pub tex_coord: u32,
    /// Translation applied after scale and rotation.
    pub offset: [f32; 2],
    /// Counterclockwise rotation in UV coordinates, in radians.
    pub rotation: f32,
    /// Independent axis scales; negative and zero scales are valid.
    pub scale: [f32; 2],
}

impl Default for UvTransform {
    fn default() -> Self {
        Self {
            tex_coord: 0,
            offset: [0.0; 2],
            rotation: 0.0,
            scale: [1.0; 2],
        }
    }
}

impl UvTransform {
    /// Validate the supported coordinate sets and finite transform components.
    pub fn validate(self) -> Result<(), &'static str> {
        if self.tex_coord > 1 {
            return Err("Only TEXCOORD_0 and TEXCOORD_1 are supported");
        }
        if !self
            .offset
            .into_iter()
            .chain(self.scale)
            .chain([self.rotation])
            .all(f32::is_finite)
        {
            return Err("UV transform components must be finite");
        }
        Ok(())
    }

    /// Apply this transform to coordinates from its selected set.
    #[inline]
    pub fn transform(self, uv: [f32; 2]) -> [f32; 2] {
        let (sin, cos) = self.rotation.sin_cos();
        [
            cos * self.scale[0] * uv[0] - sin * self.scale[1] * uv[1] + self.offset[0],
            sin * self.scale[0] * uv[0] + cos * self.scale[1] * uv[1] + self.offset[1],
        ]
    }
}

/// One material role's coordinate and sampler policy, independent of its image.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextureSampling {
    /// Coordinate selection and affine transformation.
    pub uv: UvTransform,
    /// Filtering and address policy for this role.
    pub sampler: SamplerDescriptor,
}

impl Default for TextureSampling {
    fn default() -> Self {
        Self {
            uv: UvTransform::default(),
            sampler: SamplerDescriptor::linear_repeat(),
        }
    }
}

/// Portable sampling settings in albedo, normal, metallic/roughness, AO and emission order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaterialSampling {
    pub albedo: TextureSampling,
    pub normal: TextureSampling,
    pub metallic_roughness: TextureSampling,
    pub occlusion: TextureSampling,
    pub emission: TextureSampling,
}

impl MaterialSampling {
    pub(crate) fn roles(self) -> [TextureSampling; 5] {
        [
            self.albedo,
            self.normal,
            self.metallic_roughness,
            self.occlusion,
            self.emission,
        ]
    }

    /// Reject unsupported UV sets, nonfinite transforms and invalid color samplers.
    pub fn validate(self) -> Result<(), &'static str> {
        for role in self.roles() {
            role.uv.validate()?;
            role.sampler.validate()?;
            if role.sampler.comparison.is_some() {
                return Err("Material images cannot use comparison samplers");
            }
        }
        Ok(())
    }

    pub(crate) fn samplers(self) -> [SamplerDescriptor; 5] {
        self.roles().map(|role| role.sampler)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TextureCoordinates {
    matrix: [f32; 4],
    offset_set: [f32; 4],
}

impl From<UvTransform> for TextureCoordinates {
    fn from(uv: UvTransform) -> Self {
        let (sin, cos) = uv.rotation.sin_cos();
        Self {
            matrix: [
                cos * uv.scale[0],
                sin * uv.scale[0],
                -sin * uv.scale[1],
                cos * uv.scale[1],
            ],
            offset_set: [uv.offset[0], uv.offset[1], uv.tex_coord as f32, 0.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_uv_transform_composes_scale_then_rotation_then_translation() {
        let uv = UvTransform {
            scale: [-2., 3.],
            rotation: std::f32::consts::FRAC_PI_2,
            offset: [4., 5.],
            ..Default::default()
        };
        let result = uv.transform([0.25, 0.5]);
        assert!((result[0] - 2.5).abs() < 1e-6 && (result[1] - 4.5).abs() < 1e-6);
        let gpu = TextureCoordinates::from(uv);
        let result_gpu = [
            gpu.matrix[0] * 0.25 + gpu.matrix[2] * 0.5 + gpu.offset_set[0],
            gpu.matrix[1] * 0.25 + gpu.matrix[3] * 0.5 + gpu.offset_set[1],
        ];
        assert_eq!(result, result_gpu);
        assert_eq!(std::mem::size_of::<super::super::SurfaceParameters>(), 208);
    }
    #[test]
    fn test_sampling_validation_and_omission_are_independent_of_surface_fields() {
        let descriptor: crate::scene::DrawableDescriptor = serde_json::from_value(
            serde_json::json!({"surface":{"normal_scale":2},"metallic":0.5,"roughness":0.5,"ao":1}),
        )
        .unwrap();
        assert!(descriptor.sampling.is_none());
        let mut sampling = MaterialSampling::default();
        sampling.normal.uv.scale = [0., -1.];
        sampling.validate().unwrap();
        sampling.normal.uv.tex_coord = 2;
        assert!(sampling.validate().is_err());
        sampling.normal.uv.tex_coord = 1;
        sampling.normal.uv.rotation = f32::NAN;
        assert!(sampling.validate().is_err());
        sampling.normal.uv.rotation = 0.;
        sampling.normal.sampler = SamplerDescriptor::depth_comparison(katla_gfx::CompareOp::Less);
        assert!(sampling.validate().is_err());
    }
}
