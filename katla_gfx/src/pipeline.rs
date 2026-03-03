//! Rendering pipeline configuration types.
//!
//! This module provides Katla-native enum and struct types for pipeline configuration.
//! These types are independent of the underlying graphics API and convert to the
//! appropriate internal types when needed.

pub use crate::vulkan::material::builder::{Pipeline, PipelineBuilder, PipelineError};
pub use crate::vulkan::material::shadermodule::{ShaderCache, ShaderError, ShaderModule};
pub use crate::vulkan::material::{ComputePipeline, ComputePipelineBuilder, ComputePipelineError};

//=============================================================================
// Vertex Layout Types
//=============================================================================

/// Describes the format of a single vertex attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VertexAttributeFormat {
    /// Single 32-bit float.
    Float,
    /// Two 32-bit floats (vec2).
    Float2,
    /// Three 32-bit floats (vec3).
    Float3,
    /// Four 32-bit floats (vec4).
    Float4,
    /// Four unsigned bytes, not normalized.
    UByte4,
    /// Four unsigned bytes, normalized to [0, 1].
    UByte4Norm,
    /// Four unsigned 16-bit shorts, not normalized.
    UShort4,
    /// Four unsigned 16-bit shorts, normalized to [0, 1].
    UShort4Norm,
    /// Single 32-bit signed integer.
    Int,
    /// Single 32-bit unsigned integer.
    UInt,
}

impl VertexAttributeFormat {
    /// Get the size in bytes of this format.
    pub const fn size_bytes(&self) -> usize {
        match self {
            Self::Float => 4,
            Self::Float2 => 8,
            Self::Float3 => 12,
            Self::Float4 => 16,
            Self::UByte4 => 4,
            Self::UByte4Norm => 4,
            Self::UShort4 => 8,
            Self::UShort4Norm => 8,
            Self::Int => 4,
            Self::UInt => 4,
        }
    }
}

/// Describes the layout of vertex attributes in a buffer.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct VertexLayout {
    formats: Vec<VertexAttributeFormat>,
}

impl VertexLayout {
    /// Create a vertex layout from attribute formats.
    pub fn new(formats: Vec<VertexAttributeFormat>) -> Self {
        Self { formats }
    }

    /// Empty vertex layout (for fullscreen passes).
    pub fn empty() -> Self {
        Self {
            formats: Vec::new(),
        }
    }

    /// Standard PBR vertex layout: position, normal, tangent, uv.
    pub fn pbr() -> Self {
        Self::new(vec![
            VertexAttributeFormat::Float3, // position
            VertexAttributeFormat::Float3, // normal
            VertexAttributeFormat::Float4, // tangent
            VertexAttributeFormat::Float2, // uv
        ])
    }

    /// Skinned PBR vertex layout with joint indices and weights.
    pub fn pbr_skinned() -> Self {
        Self::new(vec![
            VertexAttributeFormat::Float3,  // position
            VertexAttributeFormat::Float3,  // normal
            VertexAttributeFormat::Float4,  // tangent
            VertexAttributeFormat::Float2,  // uv
            VertexAttributeFormat::UShort4, // joint indices
            VertexAttributeFormat::Float4,  // joint weights
        ])
    }

    /// Position-only vertex layout.
    pub fn position() -> Self {
        Self::new(vec![VertexAttributeFormat::Float3])
    }

    /// Position + normal vertex layout.
    pub fn position_normal() -> Self {
        Self::new(vec![
            VertexAttributeFormat::Float3,
            VertexAttributeFormat::Float3,
        ])
    }

    /// Position + normal + uv vertex layout.
    pub fn position_normal_uv() -> Self {
        Self::new(vec![
            VertexAttributeFormat::Float3,
            VertexAttributeFormat::Float3,
            VertexAttributeFormat::Float2,
        ])
    }

    /// Position + color vertex layout.
    pub fn position_color() -> Self {
        Self::new(vec![
            VertexAttributeFormat::Float3,
            VertexAttributeFormat::Float4,
        ])
    }

    /// Get the attribute formats.
    pub fn formats(&self) -> &[VertexAttributeFormat] {
        &self.formats
    }

    /// Get the number of attributes.
    pub fn len(&self) -> usize {
        self.formats.len()
    }

    /// Check if layout is empty.
    pub fn is_empty(&self) -> bool {
        self.formats.is_empty()
    }

    /// Calculate the stride in bytes.
    pub fn stride(&self) -> usize {
        self.formats.iter().map(|f| f.size_bytes()).sum()
    }
}

//=============================================================================
// Vertex Layout Conversion Implementations
//=============================================================================

impl From<VertexAttributeFormat> for crate::vulkan::vertexbinding::VertexFormat {
    fn from(format: VertexAttributeFormat) -> Self {
        use crate::vulkan::vertexbinding::VertexFormat;
        match format {
            VertexAttributeFormat::Float => VertexFormat::R32f,
            VertexAttributeFormat::Float2 => VertexFormat::RG32f,
            VertexAttributeFormat::Float3 => VertexFormat::RGB32f,
            VertexAttributeFormat::Float4 => VertexFormat::RGBA32f,
            VertexAttributeFormat::UByte4 => VertexFormat::RGBA8u,
            VertexAttributeFormat::UByte4Norm => VertexFormat::RGBA8un,
            VertexAttributeFormat::UShort4 => VertexFormat::RGBA16u,
            VertexAttributeFormat::UShort4Norm => VertexFormat::RGBA16un,
            VertexAttributeFormat::Int => VertexFormat::R32i,
            VertexAttributeFormat::UInt => VertexFormat::R32u,
        }
    }
}

impl From<&VertexLayout> for crate::vulkan::vertexbinding::VertexBinding {
    fn from(layout: &VertexLayout) -> Self {
        use crate::vulkan::vertexbinding::VertexFormat;
        Self {
            formats: layout
                .formats()
                .iter()
                .map(|f| VertexFormat::from(*f))
                .collect(),
        }
    }
}

//=============================================================================
// Katla-native Pipeline State Enums
//=============================================================================

/// Compare operation for depth/stencil testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Face culling mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Front face winding order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontFace {
    /// Counter-clockwise winding is front-facing.
    CounterClockwise,
    /// Clockwise winding is front-facing.
    Clockwise,
}

/// Polygon rasterization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolygonMode {
    /// Fill polygons.
    Fill,
    /// Draw polygon edges as lines.
    Line,
    /// Draw polygon vertices as points.
    Point,
}

/// Blend factors for color blending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Blend operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Shader stage visibility flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShaderStageFlags {
    /// Vertex shader stage.
    pub vertex: bool,
    /// Fragment shader stage.
    pub fragment: bool,
    /// Compute shader stage.
    pub compute: bool,
}

impl ShaderStageFlags {
    /// No shader stages.
    pub const NONE: Self = Self {
        vertex: false,
        fragment: false,
        compute: false,
    };

    /// Vertex shader stage only.
    pub const VERTEX: Self = Self {
        vertex: true,
        fragment: false,
        compute: false,
    };

    /// Fragment shader stage only.
    pub const FRAGMENT: Self = Self {
        vertex: false,
        fragment: true,
        compute: false,
    };

    /// Vertex and fragment shader stages (common for graphics pipelines).
    pub const VERTEX_FRAGMENT: Self = Self {
        vertex: true,
        fragment: true,
        compute: false,
    };

    /// Compute shader stage only.
    pub const COMPUTE: Self = Self {
        vertex: false,
        fragment: false,
        compute: true,
    };

    /// All shader stages.
    pub const ALL: Self = Self {
        vertex: true,
        fragment: true,
        compute: true,
    };

    /// Create a new ShaderStageFlags with all stages disabled.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if any shader stage is enabled.
    #[inline]
    pub fn is_empty(&self) -> bool {
        !self.vertex && !self.fragment && !self.compute
    }

    /// Combine two ShaderStageFlags with bitwise OR.
    #[inline]
    pub fn union(self, other: Self) -> Self {
        Self {
            vertex: self.vertex || other.vertex,
            fragment: self.fragment || other.fragment,
            compute: self.compute || other.compute,
        }
    }
}

//=============================================================================
// Vulkan Conversion Implementations
//=============================================================================

impl From<CompareOp> for ash::vk::CompareOp {
    #[inline]
    fn from(op: CompareOp) -> Self {
        match op {
            CompareOp::Never => ash::vk::CompareOp::NEVER,
            CompareOp::Less => ash::vk::CompareOp::LESS,
            CompareOp::Equal => ash::vk::CompareOp::EQUAL,
            CompareOp::LessOrEqual => ash::vk::CompareOp::LESS_OR_EQUAL,
            CompareOp::Greater => ash::vk::CompareOp::GREATER,
            CompareOp::NotEqual => ash::vk::CompareOp::NOT_EQUAL,
            CompareOp::GreaterOrEqual => ash::vk::CompareOp::GREATER_OR_EQUAL,
            CompareOp::Always => ash::vk::CompareOp::ALWAYS,
        }
    }
}

impl From<CullMode> for ash::vk::CullModeFlags {
    #[inline]
    fn from(mode: CullMode) -> Self {
        match mode {
            CullMode::None => ash::vk::CullModeFlags::NONE,
            CullMode::Front => ash::vk::CullModeFlags::FRONT,
            CullMode::Back => ash::vk::CullModeFlags::BACK,
            CullMode::FrontAndBack => ash::vk::CullModeFlags::FRONT_AND_BACK,
        }
    }
}

impl From<FrontFace> for ash::vk::FrontFace {
    #[inline]
    fn from(face: FrontFace) -> Self {
        match face {
            // Flip winding order for right-handed coordinate system.
            FrontFace::CounterClockwise => ash::vk::FrontFace::CLOCKWISE,
            FrontFace::Clockwise => ash::vk::FrontFace::COUNTER_CLOCKWISE,
        }
    }
}

impl From<PolygonMode> for ash::vk::PolygonMode {
    #[inline]
    fn from(mode: PolygonMode) -> Self {
        match mode {
            PolygonMode::Fill => ash::vk::PolygonMode::FILL,
            PolygonMode::Line => ash::vk::PolygonMode::LINE,
            PolygonMode::Point => ash::vk::PolygonMode::POINT,
        }
    }
}

impl From<BlendFactor> for ash::vk::BlendFactor {
    #[inline]
    fn from(factor: BlendFactor) -> Self {
        match factor {
            BlendFactor::Zero => ash::vk::BlendFactor::ZERO,
            BlendFactor::One => ash::vk::BlendFactor::ONE,
            BlendFactor::SrcColor => ash::vk::BlendFactor::SRC_COLOR,
            BlendFactor::OneMinusSrcColor => ash::vk::BlendFactor::ONE_MINUS_SRC_COLOR,
            BlendFactor::DstColor => ash::vk::BlendFactor::DST_COLOR,
            BlendFactor::OneMinusDstColor => ash::vk::BlendFactor::ONE_MINUS_DST_COLOR,
            BlendFactor::SrcAlpha => ash::vk::BlendFactor::SRC_ALPHA,
            BlendFactor::OneMinusSrcAlpha => ash::vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            BlendFactor::DstAlpha => ash::vk::BlendFactor::DST_ALPHA,
            BlendFactor::OneMinusDstAlpha => ash::vk::BlendFactor::ONE_MINUS_DST_ALPHA,
            BlendFactor::ConstantColor => ash::vk::BlendFactor::CONSTANT_COLOR,
            BlendFactor::OneMinusConstantColor => ash::vk::BlendFactor::ONE_MINUS_CONSTANT_COLOR,
            BlendFactor::ConstantAlpha => ash::vk::BlendFactor::CONSTANT_ALPHA,
            BlendFactor::OneMinusConstantAlpha => ash::vk::BlendFactor::ONE_MINUS_CONSTANT_ALPHA,
            BlendFactor::SrcAlphaSaturate => ash::vk::BlendFactor::SRC_ALPHA_SATURATE,
        }
    }
}

impl From<BlendOp> for ash::vk::BlendOp {
    #[inline]
    fn from(op: BlendOp) -> Self {
        match op {
            BlendOp::Add => ash::vk::BlendOp::ADD,
            BlendOp::Subtract => ash::vk::BlendOp::SUBTRACT,
            BlendOp::ReverseSubtract => ash::vk::BlendOp::REVERSE_SUBTRACT,
            BlendOp::Min => ash::vk::BlendOp::MIN,
            BlendOp::Max => ash::vk::BlendOp::MAX,
        }
    }
}

impl From<ShaderStageFlags> for ash::vk::ShaderStageFlags {
    #[inline]
    fn from(stages: ShaderStageFlags) -> Self {
        let mut flags = ash::vk::ShaderStageFlags::empty();
        if stages.vertex {
            flags |= ash::vk::ShaderStageFlags::VERTEX;
        }
        if stages.fragment {
            flags |= ash::vk::ShaderStageFlags::FRAGMENT;
        }
        if stages.compute {
            flags |= ash::vk::ShaderStageFlags::COMPUTE;
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
    fn test_compare_op_conversion() {
        assert_eq!(
            ash::vk::CompareOp::from(CompareOp::Never),
            ash::vk::CompareOp::NEVER
        );
        assert_eq!(
            ash::vk::CompareOp::from(CompareOp::Less),
            ash::vk::CompareOp::LESS
        );
        assert_eq!(
            ash::vk::CompareOp::from(CompareOp::Equal),
            ash::vk::CompareOp::EQUAL
        );
        assert_eq!(
            ash::vk::CompareOp::from(CompareOp::LessOrEqual),
            ash::vk::CompareOp::LESS_OR_EQUAL
        );
        assert_eq!(
            ash::vk::CompareOp::from(CompareOp::Greater),
            ash::vk::CompareOp::GREATER
        );
        assert_eq!(
            ash::vk::CompareOp::from(CompareOp::NotEqual),
            ash::vk::CompareOp::NOT_EQUAL
        );
        assert_eq!(
            ash::vk::CompareOp::from(CompareOp::GreaterOrEqual),
            ash::vk::CompareOp::GREATER_OR_EQUAL
        );
        assert_eq!(
            ash::vk::CompareOp::from(CompareOp::Always),
            ash::vk::CompareOp::ALWAYS
        );
    }

    #[test]
    fn test_cull_mode_conversion() {
        assert_eq!(
            ash::vk::CullModeFlags::from(CullMode::None),
            ash::vk::CullModeFlags::NONE
        );
        assert_eq!(
            ash::vk::CullModeFlags::from(CullMode::Front),
            ash::vk::CullModeFlags::FRONT
        );
        assert_eq!(
            ash::vk::CullModeFlags::from(CullMode::Back),
            ash::vk::CullModeFlags::BACK
        );
        assert_eq!(
            ash::vk::CullModeFlags::from(CullMode::FrontAndBack),
            ash::vk::CullModeFlags::FRONT_AND_BACK
        );
    }

    #[test]
    fn test_front_face_conversion() {
        // Note: Winding order is flipped for right-handed coordinate system
        assert_eq!(
            ash::vk::FrontFace::from(FrontFace::CounterClockwise),
            ash::vk::FrontFace::CLOCKWISE
        );
        assert_eq!(
            ash::vk::FrontFace::from(FrontFace::Clockwise),
            ash::vk::FrontFace::COUNTER_CLOCKWISE
        );
    }

    #[test]
    fn test_polygon_mode_conversion() {
        assert_eq!(
            ash::vk::PolygonMode::from(PolygonMode::Fill),
            ash::vk::PolygonMode::FILL
        );
        assert_eq!(
            ash::vk::PolygonMode::from(PolygonMode::Line),
            ash::vk::PolygonMode::LINE
        );
        assert_eq!(
            ash::vk::PolygonMode::from(PolygonMode::Point),
            ash::vk::PolygonMode::POINT
        );
    }

    #[test]
    fn test_blend_factor_conversion() {
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::Zero),
            ash::vk::BlendFactor::ZERO
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::One),
            ash::vk::BlendFactor::ONE
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::SrcColor),
            ash::vk::BlendFactor::SRC_COLOR
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::OneMinusSrcColor),
            ash::vk::BlendFactor::ONE_MINUS_SRC_COLOR
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::SrcAlpha),
            ash::vk::BlendFactor::SRC_ALPHA
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::OneMinusSrcAlpha),
            ash::vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::DstAlpha),
            ash::vk::BlendFactor::DST_ALPHA
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::OneMinusDstAlpha),
            ash::vk::BlendFactor::ONE_MINUS_DST_ALPHA
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::DstColor),
            ash::vk::BlendFactor::DST_COLOR
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::OneMinusDstColor),
            ash::vk::BlendFactor::ONE_MINUS_DST_COLOR
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::SrcAlphaSaturate),
            ash::vk::BlendFactor::SRC_ALPHA_SATURATE
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::ConstantColor),
            ash::vk::BlendFactor::CONSTANT_COLOR
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::OneMinusConstantColor),
            ash::vk::BlendFactor::ONE_MINUS_CONSTANT_COLOR
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::ConstantAlpha),
            ash::vk::BlendFactor::CONSTANT_ALPHA
        );
        assert_eq!(
            ash::vk::BlendFactor::from(BlendFactor::OneMinusConstantAlpha),
            ash::vk::BlendFactor::ONE_MINUS_CONSTANT_ALPHA
        );
    }

    #[test]
    fn test_blend_op_conversion() {
        assert_eq!(ash::vk::BlendOp::from(BlendOp::Add), ash::vk::BlendOp::ADD);
        assert_eq!(
            ash::vk::BlendOp::from(BlendOp::Subtract),
            ash::vk::BlendOp::SUBTRACT
        );
        assert_eq!(
            ash::vk::BlendOp::from(BlendOp::ReverseSubtract),
            ash::vk::BlendOp::REVERSE_SUBTRACT
        );
        assert_eq!(ash::vk::BlendOp::from(BlendOp::Min), ash::vk::BlendOp::MIN);
        assert_eq!(ash::vk::BlendOp::from(BlendOp::Max), ash::vk::BlendOp::MAX);
    }

    #[test]
    fn test_shader_stage_flags_constants() {
        assert!(ShaderStageFlags::VERTEX.vertex);
        assert!(!ShaderStageFlags::VERTEX.fragment);
        assert!(!ShaderStageFlags::VERTEX.compute);

        assert!(!ShaderStageFlags::FRAGMENT.vertex);
        assert!(ShaderStageFlags::FRAGMENT.fragment);
        assert!(!ShaderStageFlags::FRAGMENT.compute);

        assert!(!ShaderStageFlags::COMPUTE.vertex);
        assert!(!ShaderStageFlags::COMPUTE.fragment);
        assert!(ShaderStageFlags::COMPUTE.compute);

        assert!(ShaderStageFlags::VERTEX_FRAGMENT.vertex);
        assert!(ShaderStageFlags::VERTEX_FRAGMENT.fragment);
        assert!(!ShaderStageFlags::VERTEX_FRAGMENT.compute);

        assert!(ShaderStageFlags::ALL.vertex);
        assert!(ShaderStageFlags::ALL.fragment);
        assert!(ShaderStageFlags::ALL.compute);

        assert!(!ShaderStageFlags::NONE.vertex);
        assert!(!ShaderStageFlags::NONE.fragment);
        assert!(!ShaderStageFlags::NONE.compute);
    }

    #[test]
    fn test_shader_stage_flags_is_empty() {
        assert!(ShaderStageFlags::NONE.is_empty());
        assert!(ShaderStageFlags::new().is_empty());
        assert!(!ShaderStageFlags::VERTEX.is_empty());
        assert!(!ShaderStageFlags::ALL.is_empty());
    }

    #[test]
    fn test_shader_stage_flags_union() {
        let combined = ShaderStageFlags::VERTEX.union(ShaderStageFlags::FRAGMENT);
        assert!(combined.vertex);
        assert!(combined.fragment);
        assert!(!combined.compute);

        let all = ShaderStageFlags::VERTEX_FRAGMENT.union(ShaderStageFlags::COMPUTE);
        assert!(all.vertex);
        assert!(all.fragment);
        assert!(all.compute);
    }

    #[test]
    fn test_shader_stage_flags_conversion() {
        assert_eq!(
            ash::vk::ShaderStageFlags::from(ShaderStageFlags::VERTEX),
            ash::vk::ShaderStageFlags::VERTEX
        );
        assert_eq!(
            ash::vk::ShaderStageFlags::from(ShaderStageFlags::FRAGMENT),
            ash::vk::ShaderStageFlags::FRAGMENT
        );
        assert_eq!(
            ash::vk::ShaderStageFlags::from(ShaderStageFlags::COMPUTE),
            ash::vk::ShaderStageFlags::COMPUTE
        );
        assert_eq!(
            ash::vk::ShaderStageFlags::from(ShaderStageFlags::VERTEX_FRAGMENT),
            ash::vk::ShaderStageFlags::VERTEX | ash::vk::ShaderStageFlags::FRAGMENT
        );
        assert_eq!(
            ash::vk::ShaderStageFlags::from(ShaderStageFlags::ALL),
            ash::vk::ShaderStageFlags::VERTEX
                | ash::vk::ShaderStageFlags::FRAGMENT
                | ash::vk::ShaderStageFlags::COMPUTE
        );
        assert_eq!(
            ash::vk::ShaderStageFlags::from(ShaderStageFlags::NONE),
            ash::vk::ShaderStageFlags::empty()
        );
    }

    #[test]
    fn test_enum_hash() {
        use std::collections::HashSet;

        // Verify Hash is implemented by using HashSet
        let mut set = HashSet::new();
        set.insert(CompareOp::Less);
        set.insert(CompareOp::Greater);
        assert_eq!(set.len(), 2);

        let mut cull_set = HashSet::new();
        cull_set.insert(CullMode::Back);
        cull_set.insert(CullMode::None);
        assert_eq!(cull_set.len(), 2);

        let mut blend_set = HashSet::new();
        blend_set.insert(BlendFactor::SrcAlpha);
        blend_set.insert(BlendFactor::One);
        assert_eq!(blend_set.len(), 2);
    }

    //=========================================================================
    // Vertex Layout Tests
    //=========================================================================

    mod vertex_attribute_format {
        use super::*;

        #[test]
        fn test_size_bytes() {
            assert_eq!(VertexAttributeFormat::Float.size_bytes(), 4);
            assert_eq!(VertexAttributeFormat::Float2.size_bytes(), 8);
            assert_eq!(VertexAttributeFormat::Float3.size_bytes(), 12);
            assert_eq!(VertexAttributeFormat::Float4.size_bytes(), 16);
            assert_eq!(VertexAttributeFormat::UByte4.size_bytes(), 4);
            assert_eq!(VertexAttributeFormat::UByte4Norm.size_bytes(), 4);
            assert_eq!(VertexAttributeFormat::UShort4.size_bytes(), 8);
            assert_eq!(VertexAttributeFormat::UShort4Norm.size_bytes(), 8);
            assert_eq!(VertexAttributeFormat::Int.size_bytes(), 4);
            assert_eq!(VertexAttributeFormat::UInt.size_bytes(), 4);
        }

        #[test]
        fn test_format_hash() {
            use std::collections::HashSet;

            let mut set = HashSet::new();
            set.insert(VertexAttributeFormat::Float3);
            set.insert(VertexAttributeFormat::Float3);
            set.insert(VertexAttributeFormat::Float4);
            assert_eq!(set.len(), 2);
        }
    }

    mod vertex_layout {
        use super::*;

        #[test]
        fn test_empty_layout() {
            let layout = VertexLayout::empty();
            assert!(layout.is_empty());
            assert_eq!(layout.len(), 0);
            assert_eq!(layout.stride(), 0);
            assert_eq!(layout.formats(), &[]);
        }

        #[test]
        fn test_new_layout() {
            let layout = VertexLayout::new(vec![
                VertexAttributeFormat::Float3,
                VertexAttributeFormat::Float2,
            ]);
            assert!(!layout.is_empty());
            assert_eq!(layout.len(), 2);
            assert_eq!(layout.stride(), 20); // 12 + 8
        }

        #[test]
        fn test_pbr_layout() {
            let layout = VertexLayout::pbr();
            assert_eq!(layout.len(), 4);
            assert_eq!(
                layout.formats(),
                &[
                    VertexAttributeFormat::Float3, // position
                    VertexAttributeFormat::Float3, // normal
                    VertexAttributeFormat::Float4, // tangent
                    VertexAttributeFormat::Float2, // uv
                ]
            );
            assert_eq!(layout.stride(), 48); // 12 + 12 + 16 + 8
        }

        #[test]
        fn test_pbr_skinned_layout() {
            let layout = VertexLayout::pbr_skinned();
            assert_eq!(layout.len(), 6);
            assert_eq!(
                layout.formats(),
                &[
                    VertexAttributeFormat::Float3,  // position
                    VertexAttributeFormat::Float3,  // normal
                    VertexAttributeFormat::Float4,  // tangent
                    VertexAttributeFormat::Float2,  // uv
                    VertexAttributeFormat::UShort4, // joint indices
                    VertexAttributeFormat::Float4,  // joint weights
                ]
            );
            assert_eq!(layout.stride(), 72); // 12 + 12 + 16 + 8 + 8 + 16
        }

        #[test]
        fn test_position_layout() {
            let layout = VertexLayout::position();
            assert_eq!(layout.len(), 1);
            assert_eq!(layout.formats(), &[VertexAttributeFormat::Float3]);
            assert_eq!(layout.stride(), 12);
        }

        #[test]
        fn test_position_normal_layout() {
            let layout = VertexLayout::position_normal();
            assert_eq!(layout.len(), 2);
            assert_eq!(
                layout.formats(),
                &[VertexAttributeFormat::Float3, VertexAttributeFormat::Float3]
            );
            assert_eq!(layout.stride(), 24);
        }

        #[test]
        fn test_position_normal_uv_layout() {
            let layout = VertexLayout::position_normal_uv();
            assert_eq!(layout.len(), 3);
            assert_eq!(
                layout.formats(),
                &[
                    VertexAttributeFormat::Float3,
                    VertexAttributeFormat::Float3,
                    VertexAttributeFormat::Float2,
                ]
            );
            assert_eq!(layout.stride(), 32);
        }

        #[test]
        fn test_position_color_layout() {
            let layout = VertexLayout::position_color();
            assert_eq!(layout.len(), 2);
            assert_eq!(
                layout.formats(),
                &[VertexAttributeFormat::Float3, VertexAttributeFormat::Float4]
            );
            assert_eq!(layout.stride(), 28);
        }

        #[test]
        fn test_layout_hash() {
            use std::collections::HashSet;

            let mut set = HashSet::new();
            set.insert(VertexLayout::pbr());
            set.insert(VertexLayout::pbr());
            set.insert(VertexLayout::position());
            assert_eq!(set.len(), 2);
        }
    }

    mod vertex_layout_conversion {
        use super::*;
        use crate::vulkan::vertexbinding::VertexFormat;

        #[test]
        fn test_format_conversion_float() {
            assert_eq!(
                VertexFormat::from(VertexAttributeFormat::Float),
                VertexFormat::R32f
            );
            assert_eq!(
                VertexFormat::from(VertexAttributeFormat::Float2),
                VertexFormat::RG32f
            );
            assert_eq!(
                VertexFormat::from(VertexAttributeFormat::Float3),
                VertexFormat::RGB32f
            );
            assert_eq!(
                VertexFormat::from(VertexAttributeFormat::Float4),
                VertexFormat::RGBA32f
            );
        }

        #[test]
        fn test_format_conversion_int() {
            assert_eq!(
                VertexFormat::from(VertexAttributeFormat::Int),
                VertexFormat::R32i
            );
            assert_eq!(
                VertexFormat::from(VertexAttributeFormat::UInt),
                VertexFormat::R32u
            );
        }

        #[test]
        fn test_format_conversion_packed() {
            assert_eq!(
                VertexFormat::from(VertexAttributeFormat::UByte4),
                VertexFormat::RGBA8u
            );
            assert_eq!(
                VertexFormat::from(VertexAttributeFormat::UByte4Norm),
                VertexFormat::RGBA8un
            );
            assert_eq!(
                VertexFormat::from(VertexAttributeFormat::UShort4),
                VertexFormat::RGBA16u
            );
            assert_eq!(
                VertexFormat::from(VertexAttributeFormat::UShort4Norm),
                VertexFormat::RGBA16un
            );
        }

        #[test]
        fn test_layout_to_binding_empty() {
            let layout = VertexLayout::empty();
            let binding = crate::vulkan::vertexbinding::VertexBinding::from(&layout);
            assert!(binding.formats.is_empty());
        }

        #[test]
        fn test_layout_to_binding_pbr() {
            let layout = VertexLayout::pbr();
            let binding = crate::vulkan::vertexbinding::VertexBinding::from(&layout);

            assert_eq!(binding.formats.len(), 4);
            assert_eq!(binding.formats[0], VertexFormat::RGB32f); // position
            assert_eq!(binding.formats[1], VertexFormat::RGB32f); // normal
            assert_eq!(binding.formats[2], VertexFormat::RGBA32f); // tangent
            assert_eq!(binding.formats[3], VertexFormat::RG32f); // uv
        }

        #[test]
        fn test_layout_to_binding_pbr_skinned() {
            let layout = VertexLayout::pbr_skinned();
            let binding = crate::vulkan::vertexbinding::VertexBinding::from(&layout);

            assert_eq!(binding.formats.len(), 6);
            assert_eq!(binding.formats[0], VertexFormat::RGB32f); // position
            assert_eq!(binding.formats[1], VertexFormat::RGB32f); // normal
            assert_eq!(binding.formats[2], VertexFormat::RGBA32f); // tangent
            assert_eq!(binding.formats[3], VertexFormat::RG32f); // uv
            assert_eq!(binding.formats[4], VertexFormat::RGBA16u); // joint indices
            assert_eq!(binding.formats[5], VertexFormat::RGBA32f); // joint weights
        }

        #[test]
        fn test_layout_stride_matches_binding() {
            let layout = VertexLayout::pbr();
            let binding = crate::vulkan::vertexbinding::VertexBinding::from(&layout);

            let layout_stride = layout.stride() as u32;
            let binding_stride: u32 = binding.formats.iter().map(|f| f.get_offset()).sum();

            assert_eq!(layout_stride, binding_stride);
        }

        #[test]
        fn test_layout_skinned_stride_matches_binding() {
            let layout = VertexLayout::pbr_skinned();
            let binding = crate::vulkan::vertexbinding::VertexBinding::from(&layout);

            let layout_stride = layout.stride() as u32;
            let binding_stride: u32 = binding.formats.iter().map(|f| f.get_offset()).sum();

            assert_eq!(layout_stride, binding_stride);
        }
    }
}
