//! Pure data types for UI rendering.
//!
//! These types are GPU-agnostic and define the data contract between
//! katla_ui and the rendering layer (katla_app).
//!
//! # Architecture
//!
//! ```text
//! katla_ui produces:
//!   DrawList { vertices, indices, commands }
//!        |
//!        |  (TextureId -> TextureHandle mapping in katla_app)
//!        v
//! katla_gfx renders:
//!   UIDrawList { VertexUI, indices, UiDrawCommand }
//! ```

use katla_math::Vec2;

/// Opaque texture identifier.
///
/// This is a pure ID with no GPU knowledge. katla_app maintains
/// a registry that maps `TextureId` to `katla_gfx::TextureHandle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextureId(pub u64);

impl TextureId {
    /// No texture (solid color rendering).
    pub const NONE: TextureId = TextureId(0);

    /// Font atlas texture (conventional ID).
    pub const FONT_ATLAS: TextureId = TextureId(1);

    /// Create a new texture ID.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Create a TextureId from a handle index.
    ///
    /// This is used by katla_app to convert GPU handle indices to texture IDs.
    #[inline]
    pub const fn from_handle_index(index: u32) -> Self {
        Self(index as u64)
    }
}

/// A single vertex in the UI draw list.
///
/// Uses katla_math types for convenience. katla_app converts this
/// to `katla_gfx::VertexUI` for GPU upload.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    /// Position in screen coordinates (pixels).
    pub pos: Vec2,
    /// Texture coordinates (0.0 - 1.0).
    pub uv: Vec2,
    /// Vertex color (RGBA as bytes).
    pub color: [u8; 4],
}

impl Vertex {
    /// Create a new vertex.
    #[inline]
    pub const fn new(pos: Vec2, uv: Vec2, color: [u8; 4]) -> Self {
        Self { pos, uv, color }
    }

    /// Create a position-only vertex for solid color rendering.
    ///
    /// UV is set to (0, 0) which samples the default white texture.
    #[inline]
    pub fn position_only(pos: Vec2, color: [u8; 4]) -> Self {
        Self {
            pos,
            uv: Vec2::ZERO,
            color,
        }
    }

    /// Create from raw arrays (for conversion from GPU types).
    #[inline]
    pub fn from_raw(position: [f32; 2], uv: [f32; 2], color: [u8; 4]) -> Self {
        Self {
            pos: Vec2::new(position[0], position[1]),
            uv: Vec2::new(uv[0], uv[1]),
            color,
        }
    }
}

/// A single draw command in the UI draw list.
///
/// Each command represents a batch of primitives that share the same
/// texture and clipping rectangle.
#[derive(Debug, Clone, Copy)]
pub struct DrawCmd {
    /// Starting index in the index buffer.
    pub index_offset: u32,
    /// Number of indices to draw.
    pub index_count: u32,
    /// Clipping rectangle in pixels: [x, y, width, height].
    /// None = no clipping (full screen).
    pub clip_rect: Option<[f32; 4]>,
    /// Texture to sample from.
    /// Use `TextureId::NONE` for solid color rendering.
    pub texture: TextureId,
}

impl DrawCmd {
    /// Create a new draw command.
    #[inline]
    pub const fn new(
        index_offset: u32,
        index_count: u32,
        clip_rect: Option<[f32; 4]>,
        texture: TextureId,
    ) -> Self {
        Self {
            index_offset,
            index_count,
            clip_rect,
            texture,
        }
    }
}

impl Default for DrawCmd {
    fn default() -> Self {
        Self::new(0, 0, None, TextureId::NONE)
    }
}
