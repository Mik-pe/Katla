//! Explicit sampling policy for the canonical UI shader fixture.
use katla_gfx::{PassBindings, SamplerBinding, SamplingMode, ShaderStages};
pub fn bindings() -> PassBindings {
    PassBindings {
        samplers: vec![SamplerBinding {
            group: 0,
            binding: 1,
            stages: ShaderStages::FRAGMENT,
            sampling: SamplingMode::Linear,
        }],
        ..Default::default()
    }
}
