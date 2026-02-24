//! Wrapper types for pipeline creation.
//!
//! This module provides wrapper enums and structs for Vulkan pipeline state,
//! avoiding the need to expose `ash::vk` types in the public API.

use ash::vk;

/// Descriptor type for descriptor set layout bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DescriptorType {
    /// Uniform buffer.
    UniformBuffer,
    /// Storage buffer.
    StorageBuffer,
    /// Sampled image.
    SampledImage,
    /// Sampler.
    Sampler,
    /// Combined image sampler.
    CombinedImageSampler,
    /// Uniform texel buffer.
    UniformTexelBuffer,
    /// Storage texel buffer.
    StorageTexelBuffer,
    /// Input attachment.
    InputAttachment,
    /// Storage image.
    StorageImage,
}

impl From<DescriptorType> for vk::DescriptorType {
    fn from(ty: DescriptorType) -> Self {
        match ty {
            DescriptorType::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
            DescriptorType::StorageBuffer => vk::DescriptorType::STORAGE_BUFFER,
            DescriptorType::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
            DescriptorType::Sampler => vk::DescriptorType::SAMPLER,
            DescriptorType::CombinedImageSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            DescriptorType::UniformTexelBuffer => vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
            DescriptorType::StorageTexelBuffer => vk::DescriptorType::STORAGE_TEXEL_BUFFER,
            DescriptorType::InputAttachment => vk::DescriptorType::INPUT_ATTACHMENT,
            DescriptorType::StorageImage => vk::DescriptorType::STORAGE_IMAGE,
        }
    }
}

/// Compare operation for depth/stencil tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    /// Never passes.
    Never,
    /// Passes if value is less.
    Less,
    /// Passes if values are equal.
    Equal,
    /// Passes if value is less or equal.
    LessOrEqual,
    /// Passes if value is greater.
    Greater,
    /// Passes if values are not equal.
    NotEqual,
    /// Passes if value is greater or equal.
    GreaterOrEqual,
    /// Always passes.
    Always,
}

impl From<CompareOp> for vk::CompareOp {
    fn from(op: CompareOp) -> Self {
        match op {
            CompareOp::Never => vk::CompareOp::NEVER,
            CompareOp::Less => vk::CompareOp::LESS,
            CompareOp::Equal => vk::CompareOp::EQUAL,
            CompareOp::LessOrEqual => vk::CompareOp::LESS_OR_EQUAL,
            CompareOp::Greater => vk::CompareOp::GREATER,
            CompareOp::NotEqual => vk::CompareOp::NOT_EQUAL,
            CompareOp::GreaterOrEqual => vk::CompareOp::GREATER_OR_EQUAL,
            CompareOp::Always => vk::CompareOp::ALWAYS,
        }
    }
}

/// Culling mode for rasterization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CullMode {
    /// No culling.
    None,
    /// Cull front-facing primitives.
    Front,
    /// Cull back-facing primitives.
    Back,
    /// Cull all primitives.
    FrontAndBack,
}

impl From<CullMode> for vk::CullModeFlags {
    fn from(mode: CullMode) -> Self {
        match mode {
            CullMode::None => vk::CullModeFlags::NONE,
            CullMode::Front => vk::CullModeFlags::FRONT,
            CullMode::Back => vk::CullModeFlags::BACK,
            CullMode::FrontAndBack => vk::CullModeFlags::FRONT_AND_BACK,
        }
    }
}

/// Front face winding order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontFace {
    /// Counter-clockwise winding is front-facing.
    CounterClockwise,
    /// Clockwise winding is front-facing.
    Clockwise,
}

impl From<FrontFace> for vk::FrontFace {
    fn from(face: FrontFace) -> Self {
        match face {
            // This implementation is correct.
            // We flip the winding order for front-facing primitives, since we use a right-handed coordinate system.
            FrontFace::CounterClockwise => vk::FrontFace::CLOCKWISE,
            FrontFace::Clockwise => vk::FrontFace::COUNTER_CLOCKWISE,
        }
    }
}

/// Blend factor for color blending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendFactor {
    /// Zero.
    Zero,
    /// One.
    One,
    /// Source color.
    SrcColor,
    /// One minus source color.
    OneMinusSrcColor,
    /// Destination color.
    DstColor,
    /// One minus destination color.
    OneMinusDstColor,
    /// Source alpha.
    SrcAlpha,
    /// One minus source alpha.
    OneMinusSrcAlpha,
    /// Destination alpha.
    DstAlpha,
    /// One minus destination alpha.
    OneMinusDstAlpha,
    /// Constant color.
    ConstantColor,
    /// One minus constant color.
    OneMinusConstantColor,
    /// Constant alpha.
    ConstantAlpha,
    /// One minus constant alpha.
    OneMinusConstantAlpha,
    /// Source alpha saturate.
    SrcAlphaSaturate,
}

impl From<BlendFactor> for vk::BlendFactor {
    fn from(factor: BlendFactor) -> Self {
        match factor {
            BlendFactor::Zero => vk::BlendFactor::ZERO,
            BlendFactor::One => vk::BlendFactor::ONE,
            BlendFactor::SrcColor => vk::BlendFactor::SRC_COLOR,
            BlendFactor::OneMinusSrcColor => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
            BlendFactor::DstColor => vk::BlendFactor::DST_COLOR,
            BlendFactor::OneMinusDstColor => vk::BlendFactor::ONE_MINUS_DST_COLOR,
            BlendFactor::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
            BlendFactor::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            BlendFactor::DstAlpha => vk::BlendFactor::DST_ALPHA,
            BlendFactor::OneMinusDstAlpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
            BlendFactor::ConstantColor => vk::BlendFactor::CONSTANT_COLOR,
            BlendFactor::OneMinusConstantColor => vk::BlendFactor::ONE_MINUS_CONSTANT_COLOR,
            BlendFactor::ConstantAlpha => vk::BlendFactor::CONSTANT_ALPHA,
            BlendFactor::OneMinusConstantAlpha => vk::BlendFactor::ONE_MINUS_CONSTANT_ALPHA,
            BlendFactor::SrcAlphaSaturate => vk::BlendFactor::SRC_ALPHA_SATURATE,
        }
    }
}

/// Blend operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendOp {
    /// Add.
    Add,
    /// Subtract.
    Subtract,
    /// Reverse subtract.
    ReverseSubtract,
    /// Min.
    Min,
    /// Max.
    Max,
}

impl From<BlendOp> for vk::BlendOp {
    fn from(op: BlendOp) -> Self {
        match op {
            BlendOp::Add => vk::BlendOp::ADD,
            BlendOp::Subtract => vk::BlendOp::SUBTRACT,
            BlendOp::ReverseSubtract => vk::BlendOp::REVERSE_SUBTRACT,
            BlendOp::Min => vk::BlendOp::MIN,
            BlendOp::Max => vk::BlendOp::MAX,
        }
    }
}

/// Primitive topology for input assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveTopology {
    /// Point list.
    PointList,
    /// Line list.
    LineList,
    /// Line strip.
    LineStrip,
    /// Triangle list.
    TriangleList,
    /// Triangle strip.
    TriangleStrip,
    /// Triangle fan.
    TriangleFan,
}

impl From<PrimitiveTopology> for vk::PrimitiveTopology {
    fn from(topology: PrimitiveTopology) -> Self {
        match topology {
            PrimitiveTopology::PointList => vk::PrimitiveTopology::POINT_LIST,
            PrimitiveTopology::LineList => vk::PrimitiveTopology::LINE_LIST,
            PrimitiveTopology::LineStrip => vk::PrimitiveTopology::LINE_STRIP,
            PrimitiveTopology::TriangleList => vk::PrimitiveTopology::TRIANGLE_LIST,
            PrimitiveTopology::TriangleStrip => vk::PrimitiveTopology::TRIANGLE_STRIP,
            PrimitiveTopology::TriangleFan => vk::PrimitiveTopology::TRIANGLE_FAN,
        }
    }
}

/// Polygon mode for rasterization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolygonMode {
    /// Fill polygons.
    Fill,
    /// Draw polygon edges as lines.
    Line,
    /// Draw polygon vertices as points.
    Point,
}

impl From<PolygonMode> for vk::PolygonMode {
    fn from(mode: PolygonMode) -> Self {
        match mode {
            PolygonMode::Fill => vk::PolygonMode::FILL,
            PolygonMode::Line => vk::PolygonMode::LINE,
            PolygonMode::Point => vk::PolygonMode::POINT,
        }
    }
}

/// Dynamic state for pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicState {
    /// Viewport.
    Viewport,
    /// Scissor.
    Scissor,
    /// Line width.
    LineWidth,
    /// Depth bias.
    DepthBias,
    /// Blend constants.
    BlendConstants,
    /// Depth bounds.
    DepthBounds,
    /// Stencil compare mask.
    StencilCompareMask,
    /// Stencil write mask.
    StencilWriteMask,
    /// Stencil reference.
    StencilReference,
}

impl From<DynamicState> for vk::DynamicState {
    fn from(state: DynamicState) -> Self {
        match state {
            DynamicState::Viewport => vk::DynamicState::VIEWPORT,
            DynamicState::Scissor => vk::DynamicState::SCISSOR,
            DynamicState::LineWidth => vk::DynamicState::LINE_WIDTH,
            DynamicState::DepthBias => vk::DynamicState::DEPTH_BIAS,
            DynamicState::BlendConstants => vk::DynamicState::BLEND_CONSTANTS,
            DynamicState::DepthBounds => vk::DynamicState::DEPTH_BOUNDS,
            DynamicState::StencilCompareMask => vk::DynamicState::STENCIL_COMPARE_MASK,
            DynamicState::StencilWriteMask => vk::DynamicState::STENCIL_WRITE_MASK,
            DynamicState::StencilReference => vk::DynamicState::STENCIL_REFERENCE,
        }
    }
}

/// Color component flags for blending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColorComponentFlags {
    pub r: bool,
    pub g: bool,
    pub b: bool,
    pub a: bool,
}

impl ColorComponentFlags {
    /// All components enabled.
    pub const ALL: Self = Self {
        r: true,
        g: true,
        b: true,
        a: true,
    };
    /// No components enabled.
    pub const NONE: Self = Self {
        r: false,
        g: false,
        b: false,
        a: false,
    };
    /// RGB components only.
    pub const RGB: Self = Self {
        r: true,
        g: true,
        b: true,
        a: false,
    };
    /// Alpha component only.
    pub const A: Self = Self {
        r: false,
        g: false,
        b: false,
        a: true,
    };
}

impl From<ColorComponentFlags> for vk::ColorComponentFlags {
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

/// Vertex input rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexInputRate {
    /// Vertex rate.
    Vertex,
    /// Instance rate.
    Instance,
}

impl From<VertexInputRate> for vk::VertexInputRate {
    fn from(rate: VertexInputRate) -> Self {
        match rate {
            VertexInputRate::Vertex => vk::VertexInputRate::VERTEX,
            VertexInputRate::Instance => vk::VertexInputRate::INSTANCE,
        }
    }
}

//=============================================================================
// Tests
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor_type_conversion() {
        let ty = DescriptorType::UniformBuffer;
        let vk_ty: vk::DescriptorType = ty.into();
        assert_eq!(vk_ty, vk::DescriptorType::UNIFORM_BUFFER);

        let ty = DescriptorType::StorageBuffer;
        let vk_ty: vk::DescriptorType = ty.into();
        assert_eq!(vk_ty, vk::DescriptorType::STORAGE_BUFFER);
    }

    #[test]
    fn test_compare_op_conversion() {
        let op = CompareOp::Greater;
        let vk_op: vk::CompareOp = op.into();
        assert_eq!(vk_op, vk::CompareOp::GREATER);
    }

    #[test]
    fn test_cull_mode_conversion() {
        let mode = CullMode::None;
        let vk_mode: vk::CullModeFlags = mode.into();
        assert_eq!(vk_mode, vk::CullModeFlags::NONE);

        let mode = CullMode::Back;
        let vk_mode: vk::CullModeFlags = mode.into();
        assert_eq!(vk_mode, vk::CullModeFlags::BACK);
    }

    #[test]
    fn test_front_face_conversion() {
        let face = FrontFace::CounterClockwise;
        let vk_face: vk::FrontFace = face.into();
        assert_eq!(vk_face, vk::FrontFace::COUNTER_CLOCKWISE);

        let face = FrontFace::Clockwise;
        let vk_face: vk::FrontFace = face.into();
        assert_eq!(vk_face, vk::FrontFace::CLOCKWISE);
    }

    #[test]
    fn test_blend_factor_conversion() {
        let factor = BlendFactor::SrcAlpha;
        let vk_factor: vk::BlendFactor = factor.into();
        assert_eq!(vk_factor, vk::BlendFactor::SRC_ALPHA);

        let factor = BlendFactor::OneMinusSrcAlpha;
        let vk_factor: vk::BlendFactor = factor.into();
        assert_eq!(vk_factor, vk::BlendFactor::ONE_MINUS_SRC_ALPHA);
    }

    #[test]
    fn test_blend_op_conversion() {
        let op = BlendOp::Add;
        let vk_op: vk::BlendOp = op.into();
        assert_eq!(vk_op, vk::BlendOp::ADD);
    }

    #[test]
    fn test_primitive_topology_conversion() {
        let topo = PrimitiveTopology::TriangleList;
        let vk_topo: vk::PrimitiveTopology = topo.into();
        assert_eq!(vk_topo, vk::PrimitiveTopology::TRIANGLE_LIST);
    }

    #[test]
    fn test_polygon_mode_conversion() {
        let mode = PolygonMode::Fill;
        let vk_mode: vk::PolygonMode = mode.into();
        assert_eq!(vk_mode, vk::PolygonMode::FILL);
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

    #[test]
    fn test_color_component_flags() {
        let flags = ColorComponentFlags::ALL;
        let vk_flags: vk::ColorComponentFlags = flags.into();
        assert_eq!(
            vk_flags,
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A
        );

        let flags = ColorComponentFlags::RGB;
        let vk_flags: vk::ColorComponentFlags = flags.into();
        assert_eq!(
            vk_flags,
            vk::ColorComponentFlags::R | vk::ColorComponentFlags::G | vk::ColorComponentFlags::B
        );
    }
}
