//! Backend-neutral renderer capability vocabulary.
//!
//! [`RendererFeature`] names the *optional* operations on [`GpuRenderer`].
//! Required renderer operations carry no flag: every backend implements them
//! and they have no successful no-op default. Optional operations declare
//! their flag here, report it through `GpuRenderer::supports_feature`, and
//! fail with `RendererError::UnsupportedFeature` when unavailable — they must
//! never succeed while doing no work.
//!
//! Callers pick fallback behavior from `supports_feature`, never from backend
//! names or `as_vulkan`/`as_metal` branching.
//!
//! [`GpuRenderer`]: super::gpu_renderer::GpuRenderer

/// Optional capability a renderer backend may support.
///
/// Each variant documents the `GpuRenderer` operation it gates. A backend
/// reports `true` only when the operation is fully implemented; the default
/// trait implementation of every gated operation returns
/// `RendererError::UnsupportedFeature` without mutating state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RendererFeature {
    /// Skeletal-animation compute pipeline (`init_animation_pipeline`).
    AnimationCompute,
    /// Forward+ tile-based light culling (`init_light_culling`).
    LightCulling,
    /// Built-in render-pass pipelines (`init_pass_pipeline`).
    PassPipelines,
    /// Shadow-map resources (`init_shadow_resources`).
    ShadowMaps,
    /// GPU particle system (`init_particle_system`).
    ParticleSystem,
    /// GPU timestamp profiling queries (`begin_timestamp`, `end_timestamp`,
    /// `read_timestamps`). Timestamps stay silent no-ops only while the
    /// backend reports this as unsupported.
    TimestampQueries,
    /// In-place texture upload (`update_texture`).
    TextureInPlaceUpdate,
    /// Per-frame depth-texture bindless registration
    /// (`register_depth_textures_bindless`).
    DepthBindlessRegistration,
    /// Direct-to-screen UI pass (`render_ui_pass`). Backends that composite
    /// UI through the frame graph instead (Vulkan) report `false` and use an
    /// explicit documented no-op.
    DirectUiPass,
}

impl RendererFeature {
    /// Every defined feature, for exhaustive testing and diagnostics.
    pub const ALL: &'static [Self] = &[
        Self::AnimationCompute,
        Self::LightCulling,
        Self::PassPipelines,
        Self::ShadowMaps,
        Self::ParticleSystem,
        Self::TimestampQueries,
        Self::TextureInPlaceUpdate,
        Self::DepthBindlessRegistration,
        Self::DirectUiPass,
    ];

    /// Stable machine-readable name for diagnostics and tests.
    pub fn name(self) -> &'static str {
        match self {
            Self::AnimationCompute => "animation_compute",
            Self::LightCulling => "light_culling",
            Self::PassPipelines => "pass_pipelines",
            Self::ShadowMaps => "shadow_maps",
            Self::ParticleSystem => "particle_system",
            Self::TimestampQueries => "timestamp_queries",
            Self::TextureInPlaceUpdate => "texture_in_place_update",
            Self::DepthBindlessRegistration => "depth_bindless_registration",
            Self::DirectUiPass => "direct_ui_pass",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_all_features_have_unique_names() {
        let names: HashSet<_> = RendererFeature::ALL.iter().map(|f| f.name()).collect();
        assert_eq!(names.len(), RendererFeature::ALL.len());
        for name in names {
            assert!(!name.is_empty());
        }
    }
}
