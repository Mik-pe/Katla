//! Draw list for batched UI rendering.
//!
//! The draw list collects all UI primitives (rectangles, text, images)
//! into batches that can be efficiently rendered by the GPU.

use crate::types::{ClipRect, DrawCmd, TextureId, Vertex};
use katla_math::{Color, Rect2D, Vec2};

/// A list of draw commands and vertex data for UI rendering.
///
/// This is the output of `UiContext::end()` and contains
/// everything needed to render the UI.
#[derive(Debug, Clone, Default)]
pub struct DrawList {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    commands: Vec<DrawCmd>,
    /// Current clip rectangle for new commands.
    current_clip: Rect2D,
    /// Current texture for batching.
    current_texture: TextureId,
    /// Current Z-index for render order (used for sorting before finalization).
    current_z: u32,
    /// Scratch buffer for circle/polygon tessellation (avoids per-frame allocation).
    scratch_points: Vec<Vec2>,
    /// Pending batches keyed by (texture, clip, z) for sorting before finalization.
    pending_batches: Vec<PendingBatch>,
}

/// Internal batch being accumulated before finalization.
#[derive(Debug, Clone)]
struct PendingBatch {
    texture: TextureId,
    clip_rect: Rect2D,
    z_index: u32,
    index_start: u32,
    index_count: u32,
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
            scratch_points: Vec::new(),
            pending_batches: Vec::new(),
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
        self.scratch_points.clear();
        self.pending_batches.clear();
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
        let color_arr = color.to_bytes();

        // Four corners in counter-clockwise order for screen space (Y-down)
        // Note: texture_index will be set during batch conversion based on TextureId
        self.vertices.push(Vertex::position_only(
            Vec2::new(bounds.min.x(), bounds.min.y()),
            color_arr,
        ));
        self.vertices.push(Vertex::position_only(
            Vec2::new(bounds.min.x(), bounds.max.y()),
            color_arr,
        ));
        self.vertices.push(Vertex::position_only(
            Vec2::new(bounds.max.x(), bounds.max.y()),
            color_arr,
        ));
        self.vertices.push(Vertex::position_only(
            Vec2::new(bounds.max.x(), bounds.min.y()),
            color_arr,
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
        let color_arr = color.to_bytes();

        // Four corners with UVs in counter-clockwise order for screen space (Y-down)
        // Note: texture_index will be set during batch conversion based on TextureId
        self.vertices.push(Vertex::new(
            Vec2::new(bounds.min.x(), bounds.min.y()),
            Vec2::new(uv.min.x(), uv.min.y()),
            color_arr,
            0, // Will be set during batch conversion
        ));
        self.vertices.push(Vertex::new(
            Vec2::new(bounds.min.x(), bounds.max.y()),
            Vec2::new(uv.min.x(), uv.max.y()),
            color_arr,
            0, // Will be set during batch conversion
        ));
        self.vertices.push(Vertex::new(
            Vec2::new(bounds.max.x(), bounds.max.y()),
            Vec2::new(uv.max.x(), uv.max.y()),
            color_arr,
            0, // Will be set during batch conversion
        ));
        self.vertices.push(Vertex::new(
            Vec2::new(bounds.max.x(), bounds.min.y()),
            Vec2::new(uv.max.x(), uv.min.y()),
            color_arr,
            0, // Will be set during batch conversion
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

    /// Add an image with custom UV coordinates and explicit texture.
    ///
    /// # Arguments
    /// * `bounds` - Screen position and size
    /// * `uv_min` - Top-left UV coordinate (0-1 for atlas, any range for viewport)
    /// * `uv_max` - Bottom-right UV coordinate
    /// * `color` - Tint color (use Color::WHITE for no tint)
    /// * `texture` - Texture ID to sample from
    pub fn add_image(
        &mut self,
        bounds: Rect2D,
        uv_min: Vec2,
        uv_max: Vec2,
        color: Color,
        texture: TextureId,
    ) {
        let uv = Rect2D::new(uv_min, uv_max);
        self.add_textured_rect(bounds, uv, color, texture);
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
        let color_arr = color.to_bytes();

        // Add all vertices
        // Note: texture_index will be set during batch conversion based on TextureId
        for &point in points {
            self.vertices.push(Vertex::position_only(point, color_arr));
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

        let nx = dy / len * thickness * 0.5;
        let ny = -dx / len * thickness * 0.5;

        self.set_texture(TextureId::NONE);

        let vertex_offset = self.vertices.len() as u32;
        let color_arr = color.to_bytes();

        // Two triangles with same winding as add_rect
        self.vertices.push(Vertex::position_only(
            Vec2::new(start.x() + nx, start.y() + ny),
            color_arr,
        ));
        self.vertices.push(Vertex::position_only(
            Vec2::new(start.x() - nx, start.y() - ny),
            color_arr,
        ));
        self.vertices.push(Vertex::position_only(
            Vec2::new(end.x() + nx, end.y() + ny),
            color_arr,
        ));
        self.vertices.push(Vertex::position_only(
            Vec2::new(end.x() - nx, end.y() - ny),
            color_arr,
        ));

        self.indices.extend_from_slice(&[
            vertex_offset,
            vertex_offset + 1,
            vertex_offset + 2,
            vertex_offset + 1,
            vertex_offset + 3,
            vertex_offset + 2,
        ]);
    }

    /// Add a circle outline.
    pub fn add_circle(&mut self, center: Vec2, radius: f32, color: Color, segments: u32) {
        if segments < 3 {
            return;
        }

        // Use scratch buffer to avoid per-frame allocation
        self.scratch_points.clear();
        self.scratch_points.reserve(segments as usize);

        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            self.scratch_points.push(Vec2::new(
                center.x() + radius * angle.cos(),
                center.y() + radius * angle.sin(),
            ));
        }

        // Take ownership temporarily to avoid borrow conflict
        let points = std::mem::take(&mut self.scratch_points);
        self.add_convex_poly(&points, color);
        self.scratch_points = points;
    }

    /// Flush the current batch into pending batches.
    fn flush_batch(&mut self) {
        let index_count = self.indices.len() as u32;

        // Find where this batch starts
        let index_offset = self
            .pending_batches
            .last()
            .map(|b| b.index_start + b.index_count)
            .unwrap_or(0);

        // Only create a batch if there are new indices
        let batch_index_count = index_count - index_offset;
        if batch_index_count > 0 {
            self.pending_batches.push(PendingBatch {
                texture: self.current_texture,
                clip_rect: self.current_clip,
                z_index: self.current_z,
                index_start: index_offset,
                index_count: batch_index_count,
            });
        }
    }

    /// Finalize the draw list, sorting by Z-index and converting to GPU commands.
    pub fn finalize(&mut self) {
        self.flush_batch();

        // Sort batches by Z-index (stable sort preserves order within same Z)
        self.pending_batches.sort_by_key(|b| b.z_index);

        // Convert to GPU commands
        self.commands = self
            .pending_batches
            .iter()
            .map(|batch| {
                let clip_rect = if batch.clip_rect.min.x() < f32::MAX / 2.0 {
                    Some(ClipRect::from_rect(&batch.clip_rect).to_array())
                } else {
                    None
                };
                DrawCmd::new(
                    batch.index_start,
                    batch.index_count,
                    clip_rect,
                    batch.texture,
                )
            })
            .collect();
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

    /// Get vertex data as bytes for GPU upload.
    pub fn vertex_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.vertices)
    }

    /// Get index data as bytes for GPU upload.
    pub fn index_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.indices)
    }

    /// Get the vertices slice.
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    /// Get the indices slice.
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Get the draw commands slice.
    pub fn commands(&self) -> &[DrawCmd] {
        &self.commands
    }
}

// Safety: Vertex is POD and can be safely cast to bytes
unsafe impl bytemuck::Pod for Vertex {}
unsafe impl bytemuck::Zeroable for Vertex {}

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
    fn test_add_line_vertical() {
        let mut list = DrawList::new();

        list.add_line(
            Vec2::new(50.0, 10.0),
            Vec2::new(50.0, 30.0),
            Color::WHITE,
            2.0,
        );
        list.finalize();

        assert_eq!(list.vertex_count(), 4);
        assert_eq!(list.index_count(), 6);

        // Vertical line should produce a 2px wide, 20px tall quad centered at x=50
        let xs: Vec<f32> = list.vertices().iter().map(|v| v.pos.x()).collect();
        let ys: Vec<f32> = list.vertices().iter().map(|v| v.pos.y()).collect();
        assert_eq!(
            xs.iter()
                .cloned()
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap(),
            49.0
        );
        assert_eq!(
            xs.iter()
                .cloned()
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap(),
            51.0
        );
        assert_eq!(
            ys.iter()
                .cloned()
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap(),
            10.0
        );
        assert_eq!(
            ys.iter()
                .cloned()
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap(),
            30.0
        );
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
