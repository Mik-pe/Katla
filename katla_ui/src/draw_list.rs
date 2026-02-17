//! Draw list for batched UI rendering.
//!
//! The draw list collects all UI primitives (rectangles, text, images)
//! into batches that can be efficiently rendered by the GPU.

use katla_math::{Color, Rect2D, Vec2};

/// Identifier for a texture in the UI system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextureId(pub u64);

impl TextureId {
    /// No texture (solid color rendering).
    pub const NONE: TextureId = TextureId(0);

    /// Default font atlas texture.
    pub const FONT_ATLAS: TextureId = TextureId(1);

    /// Game viewport texture (rendered scene).
    pub const VIEWPORT: TextureId = TextureId(2);

    /// Reserved texture IDs start here.
    pub const CUSTOM_START: u64 = 100;

    /// Create a custom texture ID.
    pub fn custom(id: u64) -> Self {
        TextureId(Self::CUSTOM_START + id)
    }
}

/// A single vertex in the UI draw list.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiVertex {
    /// Position in screen coordinates (pixels).
    pub position: Vec2,
    /// Texture coordinates (0.0 - 1.0). Use (0, 0) for solid colors.
    pub uv: Vec2,
    /// Vertex color (multiplied with texture if present).
    pub color: Color,
}

impl UiVertex {
    /// Create a new vertex.
    #[inline]
    pub fn new(position: Vec2, uv: Vec2, color: Color) -> Self {
        Self {
            position,
            uv,
            color,
        }
    }

    /// Create a position-only vertex (solid color, uses white pixel).
    /// UV points to center of white pixel (0,0) to avoid linear filtering artifacts.
    #[inline]
    pub fn position_only(position: Vec2, color: Color) -> Self {
        // White pixel is at (0,0) in 512x512 atlas, center is at (0.5/512, 0.5/512)
        const WHITE_PIXEL_UV: f32 = 0.5 / 512.0;
        Self {
            position,
            uv: Vec2::new(WHITE_PIXEL_UV, WHITE_PIXEL_UV),
            color,
        }
    }
}

/// A draw command in the list.
///
/// Each command represents a batch of primitives that share
/// the same texture and clipping rectangle.
#[derive(Debug, Clone)]
pub struct DrawCommand {
    /// Texture to use for this batch (None = solid color).
    pub texture: TextureId,
    /// Clipping rectangle for scissor test.
    pub clip_rect: Rect2D,
    /// Number of indices to draw for this command.
    pub index_count: u32,
    /// Starting index in the index buffer.
    pub index_offset: u32,
    /// Z-index for render order (higher = rendered on top).
    pub z_index: u32,
}

impl DrawCommand {
    /// Create a new draw command.
    pub fn new(
        texture: TextureId,
        clip_rect: Rect2D,
        index_count: u32,
        index_offset: u32,
        z_index: u32,
    ) -> Self {
        Self {
            texture,
            clip_rect,
            index_count,
            index_offset,
            z_index,
        }
    }
}

/// A list of draw commands and vertex data for UI rendering.
///
/// This is the output of `UiContext::end()` and contains
/// everything needed to render the UI.
#[derive(Debug, Clone, Default)]
pub struct DrawList {
    /// All vertices in the draw list.
    pub vertices: Vec<UiVertex>,
    /// All indices in the draw list.
    pub indices: Vec<u32>,
    /// Draw commands (batches).
    pub commands: Vec<DrawCommand>,
    /// Current clip rectangle for new commands.
    current_clip: Rect2D,
    /// Current texture for batching.
    current_texture: TextureId,
    /// Current Z-index for render order.
    current_z: u32,
}

impl DrawList {
    /// Create a new, empty draw list.
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            commands: Vec::new(),
            current_clip: Rect2D::from_size(Vec2::new(f32::MAX, f32::MAX)),
            current_texture: TextureId::NONE,
            current_z: 0,
        }
    }

    /// Clear the draw list for a new frame.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.commands.clear();
        self.current_clip = Rect2D::from_size(Vec2::new(f32::MAX, f32::MAX));
        self.current_texture = TextureId::NONE;
        self.current_z = 0;
    }

    /// Set the current Z-index for subsequent draw commands.
    ///
    /// Higher Z values are rendered on top of lower Z values.
    pub fn set_z_index(&mut self, z: u32) {
        if self.current_z != z {
            self.flush_batch();
            self.current_z = z;
        }
    }

    /// Get the current Z-index.
    pub fn z_index(&self) -> u32 {
        self.current_z
    }

    /// Set the current clip rectangle.
    ///
    /// This will flush the current batch if the clip has changed.
    pub fn set_clip(&mut self, clip: Rect2D) {
        if self.current_clip != clip {
            self.flush_batch();
            self.current_clip = clip;
        }
    }

    /// Set the current texture.
    ///
    /// This will flush the current batch if the texture has changed.
    pub fn set_texture(&mut self, texture: TextureId) {
        if self.current_texture != texture {
            self.flush_batch();
            self.current_texture = texture;
        }
    }

    /// Add a solid-color rectangle.
    ///
    /// This adds 4 vertices and 6 indices for a quad.
    pub fn add_rect(&mut self, bounds: Rect2D, color: Color) {
        self.set_texture(TextureId::NONE);

        let vertex_offset = self.vertices.len() as u32;

        // Four corners
        self.vertices.push(UiVertex::position_only(
            Vec2::new(bounds.min.x(), bounds.min.y()),
            color,
        ));
        self.vertices.push(UiVertex::position_only(
            Vec2::new(bounds.max.x(), bounds.min.y()),
            color,
        ));
        self.vertices.push(UiVertex::position_only(
            Vec2::new(bounds.max.x(), bounds.max.y()),
            color,
        ));
        self.vertices.push(UiVertex::position_only(
            Vec2::new(bounds.min.x(), bounds.max.y()),
            color,
        ));

        // Two triangles
        self.indices.extend_from_slice(&[
            vertex_offset,
            vertex_offset + 1,
            vertex_offset + 2,
            vertex_offset,
            vertex_offset + 2,
            vertex_offset + 3,
        ]);
    }

    /// Add a textured rectangle (quad).
    ///
    /// The UV coordinates specify which part of the texture to use.
    pub fn add_textured_rect(
        &mut self,
        bounds: Rect2D,
        uv: Rect2D,
        color: Color,
        texture: TextureId,
    ) {
        self.set_texture(texture);

        let vertex_offset = self.vertices.len() as u32;

        // Four corners with UVs
        self.vertices.push(UiVertex::new(
            Vec2::new(bounds.min.x(), bounds.min.y()),
            Vec2::new(uv.min.x(), uv.min.y()),
            color,
        ));
        self.vertices.push(UiVertex::new(
            Vec2::new(bounds.max.x(), bounds.min.y()),
            Vec2::new(uv.max.x(), uv.min.y()),
            color,
        ));
        self.vertices.push(UiVertex::new(
            Vec2::new(bounds.max.x(), bounds.max.y()),
            Vec2::new(uv.max.x(), uv.max.y()),
            color,
        ));
        self.vertices.push(UiVertex::new(
            Vec2::new(bounds.min.x(), bounds.max.y()),
            Vec2::new(uv.min.x(), uv.max.y()),
            color,
        ));

        // Two triangles
        self.indices.extend_from_slice(&[
            vertex_offset,
            vertex_offset + 1,
            vertex_offset + 2,
            vertex_offset,
            vertex_offset + 2,
            vertex_offset + 3,
        ]);
    }

    /// Add an image with custom UV coordinates.
    ///
    /// UV encoding for texture selection:
    /// - uv_min.x < 1.0: Sample from font atlas
    /// - uv_min.x >= 1.0: Sample from viewport texture (1.0 is subtracted from x)
    pub fn add_image(&mut self, bounds: Rect2D, uv_min: Vec2, uv_max: Vec2, color: Color) {
        // Use VIEWPORT texture ID if UV indicates viewport (x >= 1.0)
        let texture = if uv_min.x() >= 1.0 {
            TextureId::VIEWPORT
        } else {
            TextureId::FONT_ATLAS
        };

        self.set_texture(texture);

        let vertex_offset = self.vertices.len() as u32;

        // Four corners with UVs
        self.vertices.push(UiVertex::new(
            Vec2::new(bounds.min.x(), bounds.min.y()),
            uv_min,
            color,
        ));
        self.vertices.push(UiVertex::new(
            Vec2::new(bounds.max.x(), bounds.min.y()),
            Vec2::new(uv_max.x(), uv_min.y()),
            color,
        ));
        self.vertices.push(UiVertex::new(
            Vec2::new(bounds.max.x(), bounds.max.y()),
            uv_max,
            color,
        ));
        self.vertices.push(UiVertex::new(
            Vec2::new(bounds.min.x(), bounds.max.y()),
            Vec2::new(uv_min.x(), uv_max.y()),
            color,
        ));

        // Two triangles
        self.indices.extend_from_slice(&[
            vertex_offset,
            vertex_offset + 1,
            vertex_offset + 2,
            vertex_offset,
            vertex_offset + 2,
            vertex_offset + 3,
        ]);
    }

    /// Add a convex polygon.
    ///
    /// Vertices should be in counter-clockwise order.
    pub fn add_convex_poly(&mut self, points: &[Vec2], color: Color) {
        if points.len() < 3 {
            return;
        }

        self.set_texture(TextureId::NONE);

        let vertex_offset = self.vertices.len() as u32;

        // Add all vertices
        for &point in points {
            self.vertices.push(UiVertex::position_only(point, color));
        }

        // Triangulate using fan method
        for i in 1..(points.len() as u32 - 1) {
            self.indices.extend_from_slice(&[
                vertex_offset,
                vertex_offset + i,
                vertex_offset + i + 1,
            ]);
        }
    }

    /// Add a line with thickness.
    pub fn add_line(&mut self, start: Vec2, end: Vec2, color: Color, thickness: f32) {
        let dx = end.x() - start.x();
        let dy = end.y() - start.y();
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.0001 {
            return;
        }

        // For screen coordinates (Y-down), use (dy, -dx) for counter-clockwise winding
        let nx = dy / len * thickness * 0.5;
        let ny = -dx / len * thickness * 0.5;

        self.add_convex_poly(
            &[
                Vec2::new(start.x() + nx, start.y() + ny),
                Vec2::new(end.x() + nx, end.y() + ny),
                Vec2::new(end.x() - nx, end.y() - ny),
                Vec2::new(start.x() - nx, start.y() - ny),
            ],
            color,
        );
    }

    /// Add a circle outline.
    pub fn add_circle(&mut self, center: Vec2, radius: f32, color: Color, segments: u32) {
        if segments < 3 {
            return;
        }

        let points: Vec<Vec2> = (0..segments)
            .map(|i| {
                let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
                Vec2::new(
                    center.x() + radius * angle.cos(),
                    center.y() + radius * angle.sin(),
                )
            })
            .collect();

        self.add_convex_poly(&points, color);
    }

    /// Flush the current batch into a draw command.
    fn flush_batch(&mut self) {
        let index_count = self.indices.len() as u32;

        // Find where this batch starts
        let index_offset = self
            .commands
            .last()
            .map(|c| c.index_offset + c.index_count)
            .unwrap_or(0);

        // Only create a command if there are new indices
        let batch_index_count = index_count - index_offset;
        if batch_index_count > 0 {
            self.commands.push(DrawCommand::new(
                self.current_texture,
                self.current_clip,
                batch_index_count,
                index_offset,
                self.current_z,
            ));
        }
    }

    /// Finalize the draw list, flushing any pending batch and sorting by Z-index.
    pub fn finalize(&mut self) {
        self.flush_batch();

        // Sort commands by Z-index (stable sort preserves order within same Z)
        self.commands.sort_by_key(|c| c.z_index);
    }

    /// Check if the draw list is empty.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Get the total number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Get the total number of indices.
    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    /// Get the number of draw commands.
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::Vec2;

    #[test]
    fn test_empty_draw_list() {
        let list = DrawList::new();
        assert!(list.is_empty());
        assert_eq!(list.vertex_count(), 0);
        assert_eq!(list.index_count(), 0);
    }

    #[test]
    fn test_add_rect() {
        let mut list = DrawList::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0));

        list.add_rect(bounds, Color::RED);
        list.finalize();

        assert_eq!(list.vertex_count(), 4);
        assert_eq!(list.index_count(), 6);
        assert_eq!(list.command_count(), 1);
    }

    #[test]
    fn test_add_two_rects_same_batch() {
        let mut list = DrawList::new();

        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::RED,
        );
        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(100.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::BLUE,
        );
        list.finalize();

        assert_eq!(list.vertex_count(), 8);
        assert_eq!(list.index_count(), 12);
        // Should be batched together (same texture = NONE)
        assert_eq!(list.command_count(), 1);
    }

    #[test]
    fn test_add_line() {
        let mut list = DrawList::new();

        list.add_line(
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Color::WHITE,
            2.0,
        );
        list.finalize();

        // Line is rendered as a quad (4 vertices, 6 indices)
        assert_eq!(list.vertex_count(), 4);
        assert_eq!(list.index_count(), 6);
    }

    #[test]
    fn test_clear() {
        let mut list = DrawList::new();

        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::RED,
        );
        list.finalize();

        assert!(!list.is_empty());

        list.clear();

        assert!(list.is_empty());
    }
}
