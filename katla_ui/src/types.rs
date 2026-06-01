//! Pure data types for UI rendering.
//!
//! These types are GPU-agnostic and define the data contract between
//! katla_ui and the rendering layer (katla_app).
//!
//! # Architecture
//!
//! ```text
//! katla_ui produces:
//!   DrawList { instances, vertices, indices, commands }
//!        |
//!        |  Simple quads → GPU-instanced with shared unit quad
//!        |  Complex geometry → per-vertex/index emission
//!        |
//!        |  (TextureId -> TextureHandle mapping in katla_app)
//!        v
//! katla_gfx renders:
//!   UIDrawList { instances, vertices, indices, commands }
//! ```

use katla_math::{Color, Rect2D, Vec2};

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

/// Per-instance data for GPU-instanced quad rendering.
///
/// Each instance represents a single screen-space quad. The vertex shader
/// transforms a shared unit quad using this data.
///
/// # Memory Layout
/// - `position`: 8 bytes (2 x f32, top-left screen position)
/// - `size`: 8 bytes (2 x f32, width/height)
/// - `uv_min`: 8 bytes (2 x f32, atlas UV top-left)
/// - `uv_max`: 8 bytes (2 x f32, atlas UV bottom-right)
/// - `color`: 4 bytes (4 x u8, RGBA tint)
/// - `texture_index`: 4 bytes (u32, bindless texture slot)
/// - `clip_rect`: 16 bytes (4 x f32, shader clip region)
/// - Total: 56 bytes
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstanceData {
    /// Top-left screen position in logical pixels.
    pub position: [f32; 2],
    /// Width and height in logical pixels.
    pub size: [f32; 2],
    /// Top-left UV coordinate in the atlas/texture.
    pub uv_min: [f32; 2],
    /// Bottom-right UV coordinate in the atlas/texture.
    pub uv_max: [f32; 2],
    /// RGBA color tint.
    pub color: [u8; 4],
    /// Bindless texture array index.
    pub texture_index: u32,
    /// Shader clip rectangle: [x, y, width, height] in logical pixels.
    /// Set to [0, 0, MAX, MAX] for no clipping.
    pub clip_rect: [f32; 4],
}

impl InstanceData {
    /// Sentinel clip rect value meaning "no clipping".
    pub const CLIP_NONE: [f32; 4] = [0.0, 0.0, f32::MAX, f32::MAX];

    /// Create an instance for a solid-color rectangle.
    pub fn rect(bounds: Rect2D, color: Color) -> Self {
        Self {
            position: [bounds.min.x(), bounds.min.y()],
            size: [bounds.width(), bounds.height()],
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
            color: color.to_bytes(),
            texture_index: 0,
            clip_rect: Self::CLIP_NONE,
        }
    }

    /// Create an instance for a textured rectangle.
    pub fn textured(bounds: Rect2D, uv: Rect2D, color: Color, texture_index: u32) -> Self {
        Self {
            position: [bounds.min.x(), bounds.min.y()],
            size: [bounds.width(), bounds.height()],
            uv_min: [uv.min.x(), uv.min.y()],
            uv_max: [uv.max.x(), uv.max.y()],
            color: color.to_bytes(),
            texture_index,
            clip_rect: Self::CLIP_NONE,
        }
    }
}

// Safety: InstanceData is POD and can be safely cast to bytes.
unsafe impl bytemuck::Pod for InstanceData {}
unsafe impl bytemuck::Zeroable for InstanceData {}

/// A single vertex in the UI draw list.
///
/// Used for complex geometry (circles, rounded rects, lines, gradients)
/// that cannot be represented as instanced quads.
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
/// texture and clipping rectangle. Commands can be either instanced
/// (for simple quads) or vertex-based (for complex geometry).
#[derive(Debug, Clone, Copy)]
pub struct DrawCmd {
    /// Starting index in the index buffer (vertex commands)
    /// or starting index in the instance buffer (instanced commands).
    pub offset: u32,
    /// Number of indices (vertex commands) or instances (instanced commands).
    pub count: u32,
    /// Clipping rectangle in pixels: [x, y, width, height].
    /// None = no clipping (full screen).
    pub clip_rect: Option<[f32; 4]>,
    /// Texture to sample from.
    /// Use `TextureId::NONE` for solid color rendering.
    pub texture: TextureId,
    /// Whether this is an instanced draw command.
    pub is_instanced: bool,
}

impl DrawCmd {
    /// Create a new instanced draw command.
    #[inline]
    pub const fn instanced(
        instance_start: u32,
        instance_count: u32,
        clip_rect: Option<[f32; 4]>,
        texture: TextureId,
    ) -> Self {
        Self {
            offset: instance_start,
            count: instance_count,
            clip_rect,
            texture,
            is_instanced: true,
        }
    }

    /// Create a new vertex-based draw command.
    #[inline]
    pub const fn vertex(
        index_offset: u32,
        index_count: u32,
        clip_rect: Option<[f32; 4]>,
        texture: TextureId,
    ) -> Self {
        Self {
            offset: index_offset,
            count: index_count,
            clip_rect,
            texture,
            is_instanced: false,
        }
    }
}

impl Default for DrawCmd {
    fn default() -> Self {
        Self::vertex(0, 0, None, TextureId::NONE)
    }
}
