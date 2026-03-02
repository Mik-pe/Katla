//! Standard vertex types for mesh geometry.
//!
//! This module provides vertex structs that can be used with [`crate::VulkanRenderer::create_mesh`].
//! All vertex types use raw arrays (`[f32; N]`) instead of math types to avoid dependencies
//! on `katla_math` and maintain compatibility with GPU memory layouts.

use crate::pipeline::VertexLayout;

/// Trait for vertex types that can be used in mesh creation.
///
/// Implementations must provide the [`VertexLayout`] that describes
/// the attribute format for pipeline creation.
pub trait Vertex: bytemuck::Pod + bytemuck::Zeroable {
    /// Returns the vertex layout describing this vertex's attributes.
    fn layout() -> VertexLayout;
}

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
}
