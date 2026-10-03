//! Immutable sampling policies, independent of image storage and shader bindings.

use serde::{Deserialize, Serialize};

use crate::CompareOp;

/// Spatial filtering within a texture level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    Nearest,
    Linear,
}

/// Selection and interpolation between texture levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MipFilter {
    /// Sampling is restricted to level zero, including explicit-LOD shader calls.
    None,
    Nearest,
    Linear,
}

/// Texture coordinate behavior outside the normalized unit interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressMode {
    Repeat,
    ClampToEdge,
    MirroredRepeat,
}

/// A portable sampler policy cached by value by each renderer.
///
/// Anisotropy must be in `1..=16`; one disables it. The Vulkan backend clamps the
/// requested value to the device limit. Mipmapped sampling uses the image's
/// complete view, while [`MipFilter::None`] clamps the maximum LOD to zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamplerDescriptor {
    pub min_filter: FilterMode,
    pub mag_filter: FilterMode,
    pub mip_filter: MipFilter,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
    pub address_w: AddressMode,
    pub comparison: Option<CompareOp>,
    pub anisotropy: u8,
}

impl Default for SamplerDescriptor {
    fn default() -> Self {
        Self::linear_clamp()
    }
}

impl SamplerDescriptor {
    /// Linear filtering, clamped coordinates and level-zero sampling.
    pub const fn linear_clamp() -> Self {
        Self {
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mip_filter: MipFilter::None,
            address_u: AddressMode::ClampToEdge,
            address_v: AddressMode::ClampToEdge,
            address_w: AddressMode::ClampToEdge,
            comparison: None,
            anisotropy: 1,
        }
    }

    /// Nearest filtering, clamped coordinates and level-zero sampling.
    pub const fn nearest_clamp() -> Self {
        Self {
            min_filter: FilterMode::Nearest,
            mag_filter: FilterMode::Nearest,
            ..Self::linear_clamp()
        }
    }

    /// Linear filtering across the mip chain with repeated coordinates.
    pub const fn linear_repeat() -> Self {
        Self {
            mip_filter: MipFilter::Linear,
            address_u: AddressMode::Repeat,
            address_v: AddressMode::Repeat,
            address_w: AddressMode::Repeat,
            ..Self::linear_clamp()
        }
    }

    /// Linear filtering and clamped coordinates with explicit depth comparison.
    pub const fn depth_comparison(comparison: CompareOp) -> Self {
        Self {
            comparison: Some(comparison),
            ..Self::linear_clamp()
        }
    }

    /// Reject unsupported anisotropy and incompatible filtering policies.
    pub fn validate(self) -> Result<(), &'static str> {
        if !(1..=16).contains(&self.anisotropy) {
            return Err("Sampler anisotropy must be in 1..=16");
        }
        if self.anisotropy > 1
            && (self.min_filter != FilterMode::Linear || self.mag_filter != FilterMode::Linear)
        {
            return Err("Anisotropic sampling requires linear minification and magnification");
        }
        Ok(())
    }
}
