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

use katla_math::{Rect2D, Vec2};

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
    /// Bindless texture index (index into the bindless texture array).
    pub texture_index: u32,
}

impl Vertex {
    /// Create a new vertex.
    #[inline]
    pub const fn new(pos: Vec2, uv: Vec2, color: [u8; 4], texture_index: u32) -> Self {
        Self { pos, uv, color, texture_index }
    }

    /// Create a position-only vertex for solid color rendering.
    ///
    /// UV is set to (0, 0) which should point to a white pixel.
    #[inline]
    pub fn position_only(pos: Vec2, color: [u8; 4]) -> Self {
        Self {
            pos,
            uv: Vec2::ZERO,
            color,
            texture_index: 0, // Will be overridden with font atlas index
        }
    }

    /// Create from raw arrays (for conversion from GPU types).
    #[inline]
    pub fn from_raw(position: [f32; 2], uv: [f32; 2], color: [u8; 4], texture_index: u32) -> Self {
        Self {
            pos: Vec2::new(position[0], position[1]),
            uv: Vec2::new(uv[0], uv[1]),
            color,
            texture_index,
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

/// A rect for clipping (convenience type).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ClipRect {
    /// Create a new clip rect.
    #[inline]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Convert to array format for GPU.
    #[inline]
    pub const fn to_array(&self) -> [f32; 4] {
        [self.x, self.y, self.width, self.height]
    }

    /// Create from Rect2D.
    #[inline]
    pub fn from_rect(rect: &Rect2D) -> Self {
        Self {
            x: rect.min.x(),
            y: rect.min.y(),
            width: rect.width(),
            height: rect.height(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_id_default() {
        assert_eq!(TextureId::default(), TextureId::NONE);
    }

    #[test]
    fn test_texture_id_equality() {
        assert_eq!(TextureId::new(42), TextureId::new(42));
        assert_ne!(TextureId::new(1), TextureId::new(2));
    }

    #[test]
    fn test_vertex_new() {
        let v = Vertex::new(Vec2::new(10.0, 20.0), Vec2::new(0.5, 0.5), [255, 0, 0, 255], 5);
        assert_eq!(v.pos.x(), 10.0);
        assert_eq!(v.uv.y(), 0.5);
        assert_eq!(v.color, [255, 0, 0, 255]);
        assert_eq!(v.texture_index, 5);
    }

    #[test]
    fn test_vertex_position_only() {
        let v = Vertex::position_only(Vec2::new(100.0, 200.0), [128, 128, 128, 255]);
        assert_eq!(v.pos.x(), 100.0);
        assert_eq!(v.pos.y(), 200.0);
        assert_eq!(v.uv, Vec2::ZERO);
        assert_eq!(v.color, [128, 128, 128, 255]);
    }

    #[test]
    fn test_vertex_position_only_uv_coordinates() {
        // VAL-ATLAS-002: Vertex::position_only() sets UV to (0,0) for white pixel sampling
        let v = Vertex::position_only(Vec2::new(50.0, 75.0), [255, 255, 255, 255]);

        // UV should be (0, 0) to sample the white pixel at atlas origin
        assert_eq!(v.uv.x(), 0.0, "UV x should be 0 for white pixel sampling");
        assert_eq!(v.uv.y(), 0.0, "UV y should be 0 for white pixel sampling");
        assert_eq!(v.uv, Vec2::ZERO, "UV should be exactly (0, 0)");
    }

    #[test]
    fn test_vertex_color_application() {
        // VAL-ATLAS-002: Vertex colors are correctly applied
        let test_cases = [
            ([255, 0, 0, 255], "Red"),
            ([0, 255, 0, 128], "Green with 50% alpha"),
            ([0, 0, 255, 64], "Blue with 25% alpha"),
            ([255, 255, 255, 255], "White"),
            ([0, 0, 0, 255], "Black"),
            ([128, 128, 128, 255], "Gray"),
        ];

        for (color_bytes, description) in test_cases {
            let v = Vertex::position_only(Vec2::new(0.0, 0.0), color_bytes);
            assert_eq!(
                v.color, color_bytes,
                "{}: Color should be preserved exactly",
                description
            );
        }
    }

    #[test]
    fn test_draw_cmd_default() {
        let cmd = DrawCmd::default();
        assert_eq!(cmd.index_offset, 0);
        assert_eq!(cmd.index_count, 0);
        assert!(cmd.clip_rect.is_none());
        assert_eq!(cmd.texture, TextureId::NONE);
    }

    #[test]
    fn test_clip_rect_to_array() {
        let clip = ClipRect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(clip.to_array(), [10.0, 20.0, 100.0, 50.0]);
    }
}
