//! Standard vertex types and layout definitions for mesh geometry.
//!
//! This module provides:
//! - [`VertexAttributeFormat`] - Describes the format of a single vertex attribute
//! - [`VertexLayout`] - Describes the layout of vertex attributes in a buffer
//! - [`Vertex`] - Trait for vertex types that can be used in mesh creation
//! - [`AttributeType`] - Semantic attribute types for SOA vertex data
//! - Standard vertex structs (VertexPBR, VertexPBRSkinned, etc.)
//!
//! All vertex types use raw arrays (`[f32; N]`) instead of math types to avoid dependencies
//! on `katla_math` and maintain compatibility with GPU memory layouts.

/// Semantic attribute types for vertex data.
///
/// Each attribute has a default location that follows the standard PBR layout convention.
/// Used for SOA (Structure of Arrays) vertex buffer layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeType {
    Position,
    Normal,
    Tangent,
    TexCoord0,
    TexCoord1,
    Color0,
    JointIndices,
    JointWeights,
}

// Vertex Attribute Format

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

// Vertex Layout

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

    /// UI vertex layout: position (2D), uv, color, texture_index.
    /// Used for immediate mode UI rendering with bindless textures.
    pub fn ui() -> Self {
        Self::new(vec![
            VertexAttributeFormat::Float2,     // position (screen coordinates)
            VertexAttributeFormat::Float2,     // uv
            VertexAttributeFormat::UByte4Norm, // color (RGBA, normalized)
            VertexAttributeFormat::UInt,       // texture_index (bindless array index)
        ])
    }

    /// Simple vertex layout: position only.
    /// Used for debug geometry and simple primitives.
    pub fn simple() -> Self {
        Self::new(vec![VertexAttributeFormat::Float3])
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

// Vertex Layout Conversion Implementations

#[cfg(feature = "vulkan")]
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

#[cfg(feature = "vulkan")]
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

// Vertex Trait

/// Trait for vertex types that can be used in mesh creation.
///
/// Implementations must provide the [`VertexLayout`] that describes
/// the attribute format for pipeline creation.
pub trait Vertex: bytemuck::Pod + bytemuck::Zeroable {
    /// Returns the vertex layout describing this vertex's attributes.
    fn layout() -> VertexLayout;
}

// Standard Vertex Types

/// Standard PBR vertex format with position, normal, tangent, and UV.
///
/// This is the most common vertex format for PBR rendering with normal mapping.
/// The tangent's 4th component stores the sign for bitangent calculation.
///
/// # Memory Layout
/// - `position`: 12 bytes (3 x f32)
/// - `normal`: 12 bytes (3 x f32)
/// - `tangent`: 16 bytes (4 x f32, w = handedness sign)
/// - `tex_coord0`: 8 bytes (2 x f32)
/// - Total: 48 bytes
///
/// # Example
///
/// ```
/// use katla_gfx::VertexPBR;
///
/// let vertex = VertexPBR {
///     position: [0.0, 1.0, 0.0],
///     normal: [0.0, 1.0, 0.0],
///     tangent: [1.0, 0.0, 0.0, 1.0],
///     tex_coord0: [0.5, 0.5],
/// };
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VertexPBR {
    /// Vertex position in object space.
    pub position: [f32; 3],
    /// Vertex normal (normalized).
    pub normal: [f32; 3],
    /// Vertex tangent with handedness sign in W component.
    /// The W component (1.0 or -1.0) determines the bitangent direction.
    pub tangent: [f32; 4],
    /// Primary texture coordinates (UV0).
    pub tex_coord0: [f32; 2],
}

impl VertexPBR {
    /// Create a new PBR vertex.
    #[inline]
    pub const fn new(
        position: [f32; 3],
        normal: [f32; 3],
        tangent: [f32; 4],
        tex_coord0: [f32; 2],
    ) -> Self {
        Self {
            position,
            normal,
            tangent,
            tex_coord0,
        }
    }

    /// Create a vertex with position only, using defaults for other attributes.
    #[inline]
    pub const fn from_position(position: [f32; 3]) -> Self {
        Self {
            position,
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [0.0, 0.0],
        }
    }
}

impl Vertex for VertexPBR {
    #[inline]
    fn layout() -> VertexLayout {
        VertexLayout::pbr()
    }
}

/// Skinned PBR vertex format with joint indices and weights for skeletal animation.
///
/// Extends [`VertexPBR`] with joint influence data for GPU skinning.
/// Each vertex can be influenced by up to 4 joints.
///
/// # Memory Layout
/// - Base PBR attributes: 48 bytes
/// - `joint_indices`: 8 bytes (4 x u16)
/// - `joint_weights`: 16 bytes (4 x f32)
/// - Total: 72 bytes
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VertexPBRSkinned {
    /// Vertex position in object space.
    pub position: [f32; 3],
    /// Vertex normal (normalized).
    pub normal: [f32; 3],
    /// Vertex tangent with handedness sign in W component.
    pub tangent: [f32; 4],
    /// Primary texture coordinates (UV0).
    pub tex_coord0: [f32; 2],
    /// Joint indices (up to 4 influencing joints).
    /// Stored as u16 to support up to 65,535 joints.
    pub joint_indices: [u16; 4],
    /// Joint weights (must sum to 1.0).
    pub joint_weights: [f32; 4],
}

impl VertexPBRSkinned {
    /// Create a new skinned PBR vertex.
    #[inline]
    pub const fn new(
        position: [f32; 3],
        normal: [f32; 3],
        tangent: [f32; 4],
        tex_coord0: [f32; 2],
        joint_indices: [u16; 4],
        joint_weights: [f32; 4],
    ) -> Self {
        Self {
            position,
            normal,
            tangent,
            tex_coord0,
            joint_indices,
            joint_weights,
        }
    }

    /// Create a skinned vertex from a base PBR vertex with skeletal data.
    #[inline]
    pub const fn from_pbr(
        base: VertexPBR,
        joint_indices: [u16; 4],
        joint_weights: [f32; 4],
    ) -> Self {
        Self {
            position: base.position,
            normal: base.normal,
            tangent: base.tangent,
            tex_coord0: base.tex_coord0,
            joint_indices,
            joint_weights,
        }
    }
}

impl Vertex for VertexPBRSkinned {
    #[inline]
    fn layout() -> VertexLayout {
        VertexLayout::pbr_skinned()
    }
}

/// Simple position-only vertex format.
///
/// Useful for depth-only passes, shadow mapping, or simple geometry.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VertexPosition {
    /// Vertex position in object space.
    pub position: [f32; 3],
}

impl VertexPosition {
    /// Create a new position-only vertex.
    #[inline]
    pub const fn new(position: [f32; 3]) -> Self {
        Self { position }
    }
}

impl Vertex for VertexPosition {
    #[inline]
    fn layout() -> VertexLayout {
        VertexLayout::position()
    }
}

/// Position + normal vertex format.
///
/// Useful for simple lit geometry without textures.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VertexPositionNormal {
    /// Vertex position in object space.
    pub position: [f32; 3],
    /// Vertex normal (normalized).
    pub normal: [f32; 3],
}

impl VertexPositionNormal {
    /// Create a new position + normal vertex.
    #[inline]
    pub const fn new(position: [f32; 3], normal: [f32; 3]) -> Self {
        Self { position, normal }
    }
}

impl Vertex for VertexPositionNormal {
    #[inline]
    fn layout() -> VertexLayout {
        VertexLayout::position_normal()
    }
}

/// Position + normal + UV vertex format.
///
/// Useful for textured geometry without normal mapping.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VertexPositionNormalUV {
    /// Vertex position in object space.
    pub position: [f32; 3],
    /// Vertex normal (normalized).
    pub normal: [f32; 3],
    /// Texture coordinates.
    pub tex_coord0: [f32; 2],
}

impl VertexPositionNormalUV {
    /// Create a new position + normal + UV vertex.
    #[inline]
    pub const fn new(position: [f32; 3], normal: [f32; 3], tex_coord0: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            tex_coord0,
        }
    }
}

impl Vertex for VertexPositionNormalUV {
    #[inline]
    fn layout() -> VertexLayout {
        VertexLayout::position_normal_uv()
    }
}

/// Position + color vertex format.
///
/// Useful for debug visualization and simple colored geometry.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VertexPositionColor {
    /// Vertex position in object space.
    pub position: [f32; 3],
    /// Vertex color (RGBA).
    pub color: [f32; 4],
}

impl VertexPositionColor {
    /// Create a new position + color vertex.
    #[inline]
    pub const fn new(position: [f32; 3], color: [f32; 4]) -> Self {
        Self { position, color }
    }
}

impl Vertex for VertexPositionColor {
    #[inline]
    fn layout() -> VertexLayout {
        VertexLayout::position_color()
    }
}

/// UI vertex format for 2D screen-space rendering.
///
/// Used for immediate mode UI rendering with orthographic projection.
///
/// # Memory Layout
/// - `position`: 8 bytes (2 x f32, screen coordinates in pixels)
/// - `uv`: 8 bytes (2 x f32, texture coordinates 0.0-1.0)
/// - `color`: 4 bytes (4 x u8, RGBA normalized to 0.0-1.0 by GPU)
/// - Total: 24 bytes
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VertexUI {
    /// Position in screen coordinates (pixels).
    pub position: [f32; 2],
    /// Texture coordinates (0.0 - 1.0).
    pub uv: [f32; 2],
    /// Vertex color (RGBA as u8, GPU normalizes to 0.0-1.0).
    pub color: [u8; 4],
    /// Bindless texture index (index into the bindless texture array).
    pub texture_index: u32,
}

impl VertexUI {
    /// Create a new UI vertex.
    #[inline]
    pub const fn new(position: [f32; 2], uv: [f32; 2], color: [u8; 4], texture_index: u32) -> Self {
        Self {
            position,
            uv,
            color,
            texture_index,
        }
    }

    /// Create a position-only vertex for solid color rendering.
    ///
    /// UV is set to (0, 0) which samples the default white texture.
    #[inline]
    pub const fn position_only(position: [f32; 2], color: [u8; 4]) -> Self {
        Self {
            position,
            uv: [0.0, 0.0],
            color,
            texture_index: 0, // Will be set during batch conversion
        }
    }
}

impl Vertex for VertexUI {
    #[inline]
    fn layout() -> VertexLayout {
        VertexLayout::ui()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex_pbr_size() {
        assert_eq!(std::mem::size_of::<VertexPBR>(), 48);
    }

    #[test]
    fn test_vertex_pbr_skinned_size() {
        assert_eq!(std::mem::size_of::<VertexPBRSkinned>(), 72);
    }

    #[test]
    fn test_vertex_position_size() {
        assert_eq!(std::mem::size_of::<VertexPosition>(), 12);
    }

    #[test]
    fn test_vertex_position_normal_size() {
        assert_eq!(std::mem::size_of::<VertexPositionNormal>(), 24);
    }

    #[test]
    fn test_vertex_position_normal_uv_size() {
        assert_eq!(std::mem::size_of::<VertexPositionNormalUV>(), 32);
    }

    #[test]
    fn test_vertex_position_color_size() {
        assert_eq!(std::mem::size_of::<VertexPositionColor>(), 28);
    }

    #[test]
    fn test_vertex_ui_size() {
        assert_eq!(std::mem::size_of::<VertexUI>(), 24);
    }

    #[test]
    fn test_vertex_pbr_layout() {
        let layout = VertexPBR::layout();
        assert_eq!(layout.len(), 4);
        assert_eq!(layout.stride(), 48);
    }

    #[test]
    fn test_vertex_pbr_skinned_layout() {
        let layout = VertexPBRSkinned::layout();
        assert_eq!(layout.len(), 6);
        assert_eq!(layout.stride(), 72);
    }

    #[test]
    fn test_vertex_position_layout() {
        let layout = VertexPosition::layout();
        assert_eq!(layout.len(), 1);
        assert_eq!(layout.stride(), 12);
    }

    #[test]
    fn test_vertex_position_normal_layout() {
        let layout = VertexPositionNormal::layout();
        assert_eq!(layout.len(), 2);
        assert_eq!(layout.stride(), 24);
    }

    #[test]
    fn test_vertex_ui_layout() {
        let layout = VertexUI::layout();
        assert_eq!(layout.len(), 4);
        assert_eq!(
            layout.formats(),
            &[
                VertexAttributeFormat::Float2,     // position
                VertexAttributeFormat::Float2,     // uv
                VertexAttributeFormat::UByte4Norm, // color
                VertexAttributeFormat::UInt,       // texture_index
            ]
        );
        assert_eq!(layout.stride(), 24); // 8 + 8 + 4 + 4
    }

    #[test]
    fn test_vertex_pbr_creation() {
        let vertex = VertexPBR::new(
            [1.0, 2.0, 3.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.5, 0.5],
        );
        assert_eq!(vertex.position, [1.0, 2.0, 3.0]);
        assert_eq!(vertex.normal, [0.0, 1.0, 0.0]);
        assert_eq!(vertex.tangent, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(vertex.tex_coord0, [0.5, 0.5]);
    }

    #[test]
    fn test_vertex_ui_creation() {
        let vertex = VertexUI::new([10.0, 20.0], [0.5, 0.5], [255, 0, 0, 255], 5);
        assert_eq!(vertex.position, [10.0, 20.0]);
        assert_eq!(vertex.uv, [0.5, 0.5]);
        assert_eq!(vertex.color, [255, 0, 0, 255]);
        assert_eq!(vertex.texture_index, 5);
    }

    #[test]
    fn test_vertex_ui_position_only() {
        let vertex = VertexUI::position_only([100.0, 200.0], [128, 128, 128, 255]);
        assert_eq!(vertex.position, [100.0, 200.0]);
        assert_eq!(vertex.uv, [0.0, 0.0]); // White pixel UV
        assert_eq!(vertex.color, [128, 128, 128, 255]);
        assert_eq!(vertex.texture_index, 0); // Default value
    }

    #[test]
    fn test_vertex_pbr_from_position() {
        let vertex = VertexPBR::from_position([1.0, 2.0, 3.0]);
        assert_eq!(vertex.position, [1.0, 2.0, 3.0]);
        assert_eq!(vertex.normal, [0.0, 1.0, 0.0]); // Default up
        assert_eq!(vertex.tex_coord0, [0.0, 0.0]); // Default UV
    }

    #[test]
    fn test_vertex_pbr_skinned_from_pbr() {
        let base = VertexPBR::new(
            [1.0, 2.0, 3.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.5, 0.5],
        );
        let skinned = VertexPBRSkinned::from_pbr(base, [0, 1, 2, 3], [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(skinned.position, base.position);
        assert_eq!(skinned.normal, base.normal);
        assert_eq!(skinned.tangent, base.tangent);
        assert_eq!(skinned.tex_coord0, base.tex_coord0);
        assert_eq!(skinned.joint_indices, [0, 1, 2, 3]);
        assert_eq!(skinned.joint_weights, [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_bytemuck_pod() {
        // Verify that VertexPBR can be cast to bytes
        let vertex = VertexPBR::new(
            [1.0, 2.0, 3.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.5, 0.5],
        );
        let bytes: &[u8] = bytemuck::bytes_of(&vertex);
        assert_eq!(bytes.len(), 48);
    }

    //=========================================================================
    // Vertex Attribute Format Tests
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

    //=========================================================================
    // Vertex Layout Tests
    //=========================================================================

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

    //=========================================================================
    // Vertex Layout Conversion Tests
    //=========================================================================

    #[cfg(feature = "vulkan")]
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
