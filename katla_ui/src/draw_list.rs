//! Draw list for batched UI rendering.
//!
//! The draw list collects all UI primitives (rectangles, text, images)
//! into batches that can be efficiently rendered by the GPU.

use crate::types::{DrawCmd, TextureId, Vertex};
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

    /// Add a gradient rectangle with per-corner colors.
    ///
    /// Colors are: top-left, top-right, bottom-right, bottom-left.
    /// GPU vertex interpolation handles the gradient automatically.
    pub fn add_gradient_rect(
        &mut self,
        bounds: Rect2D,
        tl: Color,
        tr: Color,
        br: Color,
        bl: Color,
    ) {
        self.set_texture(TextureId::NONE);

        let vertex_offset = self.vertices.len() as u32;
        let min = bounds.min;
        let max = bounds.max;

        // Same vertex order as add_rect: TL, BL, BR, TR (CCW for Y-down)
        self.vertices.push(Vertex::position_only(
            Vec2::new(min.x(), min.y()),
            tl.to_bytes(),
        ));
        self.vertices.push(Vertex::position_only(
            Vec2::new(min.x(), max.y()),
            bl.to_bytes(),
        ));
        self.vertices.push(Vertex::position_only(
            Vec2::new(max.x(), max.y()),
            br.to_bytes(),
        ));
        self.vertices.push(Vertex::position_only(
            Vec2::new(max.x(), min.y()),
            tr.to_bytes(),
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

        // Flush to prevent gradient vertices from being batched with solid-color rects
        self.flush_batch();
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
        self.vertices.push(Vertex::new(
            Vec2::new(bounds.min.x(), bounds.min.y()),
            Vec2::new(uv.min.x(), uv.min.y()),
            color_arr,
        ));
        self.vertices.push(Vertex::new(
            Vec2::new(bounds.min.x(), bounds.max.y()),
            Vec2::new(uv.min.x(), uv.max.y()),
            color_arr,
        ));
        self.vertices.push(Vertex::new(
            Vec2::new(bounds.max.x(), bounds.max.y()),
            Vec2::new(uv.max.x(), uv.max.y()),
            color_arr,
        ));
        self.vertices.push(Vertex::new(
            Vec2::new(bounds.max.x(), bounds.min.y()),
            Vec2::new(uv.max.x(), uv.min.y()),
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

    pub fn add_circle_auto(&mut self, center: Vec2, radius: f32, color: Color) {
        let segments = (radius * std::f32::consts::PI * 2.0 / 4.0).ceil().max(8.0) as u32;
        self.add_circle(center, radius, color, segments);
    }

    pub fn add_rounded_rect(&mut self, bounds: Rect2D, color: Color, radius: f32) {
        let r = radius.min(bounds.width() * 0.5).min(bounds.height() * 0.5);
        if r < 0.5 {
            self.add_rect(bounds, color);
            return;
        }

        let segments_per_corner = ((r * std::f32::consts::PI * 0.5 / 4.0).ceil() as u32).max(2);
        let points = generate_rounded_rect_points(bounds.min, bounds.max, r, segments_per_corner);
        self.add_convex_poly(&points, color);
    }

    /// Add a rounded rectangle border stroke.
    ///
    /// Draws a thick outline along the rounded path, producing smooth corners
    /// instead of 4 sharp rectangles.
    pub fn add_rounded_rect_stroke(
        &mut self,
        bounds: Rect2D,
        color: Color,
        radius: f32,
        thickness: f32,
    ) {
        let r = radius.min(bounds.width() * 0.5).min(bounds.height() * 0.5);
        if r < 0.5 {
            self.add_rect_stroke(bounds, color, thickness);
            return;
        }

        let segments_per_corner = ((r * std::f32::consts::PI * 0.5 / 4.0).ceil() as u32).max(2);
        let half_t = thickness * 0.5;

        let outer_min = Vec2::new(bounds.min.x() - half_t, bounds.min.y() - half_t);
        let outer_max = Vec2::new(bounds.max.x() + half_t, bounds.max.y() + half_t);
        let outer_r = r + half_t;

        let inner_min = Vec2::new(bounds.min.x() + half_t, bounds.min.y() + half_t);
        let inner_max = Vec2::new(bounds.max.x() - half_t, bounds.max.y() - half_t);
        let inner_r = (r - half_t).max(0.0);

        let outer_points =
            generate_rounded_rect_points(outer_min, outer_max, outer_r, segments_per_corner);
        let inner_points =
            generate_rounded_rect_points(inner_min, inner_max, inner_r, segments_per_corner);

        let n = outer_points.len();
        debug_assert_eq!(inner_points.len(), n);

        self.set_texture(TextureId::NONE);

        let color_arr = color.to_bytes();
        let vertex_offset = self.vertices.len() as u32;

        for &p in &outer_points {
            self.vertices.push(Vertex::position_only(p, color_arr));
        }
        for &p in &inner_points {
            self.vertices.push(Vertex::position_only(p, color_arr));
        }

        for i in 0..n {
            let j = (i + 1) % n;
            let oi = vertex_offset + i as u32;
            let oj = vertex_offset + j as u32;
            let ii = vertex_offset + n as u32 + i as u32;
            let ij = vertex_offset + n as u32 + j as u32;
            self.indices.extend_from_slice(&[oi, oj, ij, oi, ij, ii]);
        }
    }

    /// Add a sharp rectangle border stroke (4 rectangles).
    fn add_rect_stroke(&mut self, bounds: Rect2D, color: Color, thickness: f32) {
        let min = bounds.min;
        let max = bounds.max;
        self.add_rect(
            Rect2D::from_origin_size(min, Vec2::new(bounds.width(), thickness)),
            color,
        );
        self.add_rect(
            Rect2D::from_origin_size(
                Vec2::new(min.x(), max.y() - thickness),
                Vec2::new(bounds.width(), thickness),
            ),
            color,
        );
        self.add_rect(
            Rect2D::from_origin_size(min, Vec2::new(thickness, bounds.height())),
            color,
        );
        self.add_rect(
            Rect2D::from_origin_size(
                Vec2::new(max.x() - thickness, min.y()),
                Vec2::new(thickness, bounds.height()),
            ),
            color,
        );
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
        self.commands.clear();
        self.commands
            .extend(self.pending_batches.iter().map(|batch| {
                let clip_rect = if batch.clip_rect.min.x() < f32::MAX / 2.0 {
                    Some(batch.clip_rect.to_clip_array())
                } else {
                    None
                };
                DrawCmd::new(
                    batch.index_start,
                    batch.index_count,
                    clip_rect,
                    batch.texture,
                )
            }));
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

/// Generate the outline points of a rounded rectangle.
///
/// Points are in counter-clockwise order starting from the top-left corner.
/// Each corner arc uses `segments_per_corner` subdivisions.
fn generate_rounded_rect_points(
    min: Vec2,
    max: Vec2,
    r: f32,
    segments_per_corner: u32,
) -> Vec<Vec2> {
    let mut points = Vec::with_capacity((segments_per_corner as usize + 1) * 4);

    // Top-left corner
    let cx = min.x() + r;
    let cy = min.y() + r;
    for i in 0..=segments_per_corner {
        let angle = std::f32::consts::PI
            + (i as f32 / segments_per_corner as f32) * std::f32::consts::FRAC_PI_2;
        points.push(Vec2::new(cx + r * angle.cos(), cy + r * angle.sin()));
    }

    // Top-right corner
    let cx = max.x() - r;
    let cy = min.y() + r;
    for i in 0..=segments_per_corner {
        let angle = std::f32::consts::FRAC_PI_2 * 3.0
            + (i as f32 / segments_per_corner as f32) * std::f32::consts::FRAC_PI_2;
        points.push(Vec2::new(cx + r * angle.cos(), cy + r * angle.sin()));
    }

    // Bottom-right corner
    let cx = max.x() - r;
    let cy = max.y() - r;
    for i in 0..=segments_per_corner {
        let angle = (i as f32 / segments_per_corner as f32) * std::f32::consts::FRAC_PI_2;
        points.push(Vec2::new(cx + r * angle.cos(), cy + r * angle.sin()));
    }

    // Bottom-left corner
    let cx = min.x() + r;
    let cy = max.y() - r;
    for i in 0..=segments_per_corner {
        let angle = std::f32::consts::FRAC_PI_2
            + (i as f32 / segments_per_corner as f32) * std::f32::consts::FRAC_PI_2;
        points.push(Vec2::new(cx + r * angle.cos(), cy + r * angle.sin()));
    }

    points
}

// Safety: Vertex is POD and can be safely cast to bytes
unsafe impl bytemuck::Pod for Vertex {}
unsafe impl bytemuck::Zeroable for Vertex {}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::Vec2;

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
