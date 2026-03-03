//! Internal wrapper types for pipeline creation.
//!
//! This module provides internal wrapper enums and structs for Vulkan pipeline state
//! that are not exposed in the public API. For Katla-native types, see `pipeline.rs`.

use ash::vk;

/// Primitive topology for input assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveTopology {
    /// Triangle list.
    TriangleList,
}

impl From<PrimitiveTopology> for vk::PrimitiveTopology {
    #[inline]
    fn from(topology: PrimitiveTopology) -> Self {
        match topology {
            PrimitiveTopology::TriangleList => vk::PrimitiveTopology::TRIANGLE_LIST,
        }
    }
}

/// Dynamic state for pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DynamicState {
    /// Viewport.
    Viewport,
    /// Scissor.
    Scissor,
}

impl From<DynamicState> for vk::DynamicState {
    #[inline]
    fn from(state: DynamicState) -> Self {
        match state {
            DynamicState::Viewport => vk::DynamicState::VIEWPORT,
            DynamicState::Scissor => vk::DynamicState::SCISSOR,
        }
    }
}

/// Color component flags for blending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct ColorComponentFlags {
    pub r: bool,
    pub g: bool,
    pub b: bool,
    pub a: bool,
}

impl ColorComponentFlags {}

impl From<ColorComponentFlags> for vk::ColorComponentFlags {
    #[inline]
    fn from(flags: ColorComponentFlags) -> Self {
        let mut result = vk::ColorComponentFlags::empty();
        if flags.r {
            result |= vk::ColorComponentFlags::R;
        }
        if flags.g {
            result |= vk::ColorComponentFlags::G;
        }
        if flags.b {
            result |= vk::ColorComponentFlags::B;
        }
        if flags.a {
            result |= vk::ColorComponentFlags::A;
        }
        result
    }
}

/// Shader stage flags for pipeline creation (extended version with all stages).
///
/// This type wraps Vulkan shader stage flags and provides a type-safe API
/// for specifying which shader stages are used in various operations.
/// For the simpler Katla-native version, see [`ShaderStageFlags`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShaderStages {
    pub vertex: bool,
    pub fragment: bool,
    pub compute: bool,
    pub geometry: bool,
    pub tessellation_control: bool,
    pub tessellation_evaluation: bool,
}

impl From<ShaderStages> for vk::ShaderStageFlags {
    #[inline]
    fn from(stages: ShaderStages) -> Self {
        let mut flags = vk::ShaderStageFlags::empty();
        if stages.vertex {
            flags |= vk::ShaderStageFlags::VERTEX;
        }
        if stages.fragment {
            flags |= vk::ShaderStageFlags::FRAGMENT;
        }
        if stages.compute {
            flags |= vk::ShaderStageFlags::COMPUTE;
        }
        if stages.geometry {
            flags |= vk::ShaderStageFlags::GEOMETRY;
        }
        if stages.tessellation_control {
            flags |= vk::ShaderStageFlags::TESSELLATION_CONTROL;
        }
        if stages.tessellation_evaluation {
            flags |= vk::ShaderStageFlags::TESSELLATION_EVALUATION;
        }
        flags
    }
}

//=============================================================================
// Tests
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_topology_conversion() {
        let topo = PrimitiveTopology::TriangleList;
        let vk_topo: vk::PrimitiveTopology = topo.into();
        assert_eq!(vk_topo, vk::PrimitiveTopology::TRIANGLE_LIST);
    }

    #[test]
    fn test_dynamic_state_conversion() {
        let state = DynamicState::Viewport;
        let vk_state: vk::DynamicState = state.into();
        assert_eq!(vk_state, vk::DynamicState::VIEWPORT);

        let state = DynamicState::Scissor;
        let vk_state: vk::DynamicState = state.into();
        assert_eq!(vk_state, vk::DynamicState::SCISSOR);
    }
}
