//! Draw list for batched UI rendering with GPU instancing.
//!
//! The draw list collects all UI primitives into batches that can be efficiently
//! rendered by the GPU. Simple quads (rectangles, textured rects) are accumulated
//! as per-instance data and rendered with a shared unit quad. Complex geometry
//! (circles, rounded rects, lines, gradients) uses traditional vertex/index emission.

use crate::types::{DrawCmd, InstanceData, TextureId, Vertex};
use katla_math::{Color, Rect2D, Vec2};

/// A list of draw commands and vertex/instance data for UI rendering.
///
/// This is the output of `UiContext::end()` and contains
/// everything needed to render the UI.
#[derive(Debug, Clone, Default)]
pub struct DrawList {
    /// Per-instance data for instanced quad rendering.
    instances: Vec<InstanceData>,
    /// Vertex data for complex geometry.
    vertices: Vec<Vertex>,
    /// Index data for complex geometry.
    indices: Vec<u32>,
    /// Finalized draw commands.
    commands: Vec<DrawCmd>,
    /// Current clip rectangle for new commands.
    current_clip: Rect2D,
    /// Current texture for batching.
    current_texture: TextureId,
    /// Current Z-index for render order.
    current_z: u32,
    /// Scratch buffer for circle/polygon tessellation.
    scratch_points: Vec<Vec2>,
    /// Pending instance batches before finalization.
    pending_instance_batches: Vec<PendingInstanceBatch>,
    /// Pending vertex batches before finalization.
    pending_vertex_batches: Vec<PendingVertexBatch>,
}

/// Internal instance batch being accumulated before finalization.
#[derive(Debug, Clone)]
struct PendingInstanceBatch {
    texture: TextureId,
    clip_rect: Rect2D,
    z_index: u32,
    instance_start: u32,
    instance_count: u32,
}

/// Internal vertex batch being accumulated before finalization.
#[derive(Debug, Clone)]
struct PendingVertexBatch {
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
            instances: Vec::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
            commands: Vec::new(),
            current_clip: Rect2D::from_size(Vec2::new(f32::MAX, f32::MAX)),
            current_texture: TextureId::NONE,
            current_z: 0,
            scratch_points: Vec::new(),
            pending_instance_batches: Vec::new(),
            pending_vertex_batches: Vec::new(),
        }
    }

    /// Clear the draw list for a new frame.
    pub fn clear(&mut self) {
        self.instances.clear();
        self.vertices.clear();
        self.indices.clear();
        self.commands.clear();
        self.current_clip = Rect2D::from_size(Vec2::new(f32::MAX, f32::MAX));
        self.current_texture = TextureId::NONE;
        self.current_z = 0;
        self.scratch_points.clear();
        self.pending_instance_batches.clear();
        self.pending_vertex_batches.clear();
    }

    /// Set the current Z-index for subsequent draw commands.
    pub fn set_z_index(&mut self, z: u32) {
        if self.current_z != z {
            self.flush_instance_batch();
            self.flush_vertex_batch();
            self.current_z = z;
        }
    }

    /// Get the current Z-index.
    pub fn z_index(&self) -> u32 {
        self.current_z
    }

    /// Set the current clip rectangle.
    pub fn set_clip(&mut self, clip: Rect2D) {
        if self.current_clip != clip {
            self.flush_instance_batch();
            self.flush_vertex_batch();
            self.current_clip = clip;
        }
    }

    /// Set the current texture.
    pub fn set_texture(&mut self, texture: TextureId) {
        if self.current_texture != texture {
            self.flush_instance_batch();
            self.flush_vertex_batch();
            self.current_texture = texture;
        }
    }

    /// Add a solid-color rectangle (instanced).
    pub fn add_rect(&mut self, bounds: Rect2D, color: Color) {
        self.set_texture(TextureId::NONE);
        let mut instance = InstanceData::rect(bounds, color);
        instance.clip_rect = self.clip_to_array();
        self.instances.push(instance);
    }

    /// Add a gradient rectangle with per-corner colors (vertex-based).
    ///
    /// Colors are: top-left, top-right, bottom-right, bottom-left.
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

        self.indices.extend_from_slice(&[
            vertex_offset,
            vertex_offset + 1,
            vertex_offset + 2,
            vertex_offset,
            vertex_offset + 2,
            vertex_offset + 3,
        ]);

        self.flush_vertex_batch();
    }

    /// Add a textured rectangle (instanced).
    pub fn add_textured_rect(
        &mut self,
        bounds: Rect2D,
        uv: Rect2D,
        color: Color,
        texture: TextureId,
    ) {
        self.set_texture(texture);
        let mut instance = InstanceData::textured(bounds, uv, color, 0);
        instance.clip_rect = self.clip_to_array();
        self.instances.push(instance);
    }

    /// Add an image with custom UV coordinates and explicit texture.
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

    /// Add a convex polygon (vertex-based).
    pub fn add_convex_poly(&mut self, points: &[Vec2], color: Color) {
        if points.len() < 3 {
            return;
        }

        self.set_texture(TextureId::NONE);

        let vertex_offset = self.vertices.len() as u32;
        let color_arr = color.to_bytes();

        for &point in points {
            self.vertices.push(Vertex::position_only(point, color_arr));
        }

        for i in 1..(points.len() as u32 - 1) {
            self.indices.extend_from_slice(&[
                vertex_offset,
                vertex_offset + i,
                vertex_offset + i + 1,
            ]);
        }
    }

    /// Add a line with thickness (vertex-based).
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

    /// Add a filled circle (vertex-based, auto segments).
    pub fn add_circle(&mut self, center: Vec2, radius: f32, color: Color, segments: u32) {
        if segments < 3 {
            return;
        }

        self.scratch_points.clear();
        self.scratch_points.reserve(segments as usize);

        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            self.scratch_points.push(Vec2::new(
                center.x() + radius * angle.cos(),
                center.y() + radius * angle.sin(),
            ));
        }

        let points = std::mem::take(&mut self.scratch_points);
        self.add_convex_poly(&points, color);
        self.scratch_points = points;
    }

    pub fn add_circle_auto(&mut self, center: Vec2, radius: f32, color: Color) {
        let segments = (radius * std::f32::consts::PI * 2.0 / 2.0).ceil().max(8.0) as u32;
        self.add_circle_aa(center, radius, color, segments);
    }

    /// Add a filled circle with anti-aliased edges (vertex-based).
    pub fn add_circle_aa(&mut self, center: Vec2, radius: f32, color: Color, segments: u32) {
        if segments < 3 {
            return;
        }

        self.set_texture(TextureId::NONE);

        let vertex_offset = self.vertices.len() as u32;
        let color_full = color.to_bytes();
        let color_fade = [color_full[0], color_full[1], color_full[2], 0u8];

        let seg = segments as usize;

        for i in 0..seg {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            self.vertices.push(Vertex::position_only(
                Vec2::new(
                    center.x() + radius * angle.cos(),
                    center.y() + radius * angle.sin(),
                ),
                color_full,
            ));
        }

        const AA_FRINGE: f32 = 1.0;
        let outer_radius = radius + AA_FRINGE;
        for i in 0..seg {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            self.vertices.push(Vertex::position_only(
                Vec2::new(
                    center.x() + outer_radius * angle.cos(),
                    center.y() + outer_radius * angle.sin(),
                ),
                color_fade,
            ));
        }

        for i in 1..(seg as u32 - 1) {
            self.indices.extend_from_slice(&[
                vertex_offset,
                vertex_offset + i,
                vertex_offset + i + 1,
            ]);
        }

        let outer_start = vertex_offset + seg as u32;
        for i in 0..seg {
            let j = (i + 1) % seg;
            let ii = vertex_offset + i as u32;
            let ij = vertex_offset + j as u32;
            let oi = outer_start + i as u32;
            let oj = outer_start + j as u32;
            self.indices.extend_from_slice(&[ii, ij, oj, ii, oj, oi]);
        }
    }

    pub fn add_rounded_rect(&mut self, bounds: Rect2D, color: Color, radius: f32) {
        let r = radius.min(bounds.width() * 0.5).min(bounds.height() * 0.5);
        if r < 0.5 {
            self.add_rect(bounds, color);
            return;
        }

        let segments_per_corner = ((r * std::f32::consts::PI * 0.5 / 1.5).ceil() as u32).max(2);
        generate_rounded_rect_points_into(
            bounds.min,
            bounds.max,
            r,
            segments_per_corner,
            &mut self.scratch_points,
        );
        let points = std::mem::take(&mut self.scratch_points);
        self.add_convex_poly(&points, color);
        self.scratch_points = points;
    }

    /// Add a filled rounded rectangle with anti-aliased edges (vertex-based).
    pub fn add_rounded_rect_aa(&mut self, bounds: Rect2D, color: Color, radius: f32) {
        let r = radius.min(bounds.width() * 0.5).min(bounds.height() * 0.5);
        if r < 0.5 {
            self.add_rect(bounds, color);
            return;
        }

        let segments_per_corner = ((r * std::f32::consts::PI * 0.5 / 1.5).ceil() as u32).max(2);

        generate_rounded_rect_points_into(
            bounds.min,
            bounds.max,
            r,
            segments_per_corner,
            &mut self.scratch_points,
        );
        let inner_points = std::mem::take(&mut self.scratch_points);
        let n = inner_points.len();

        self.set_texture(TextureId::NONE);

        let vertex_offset = self.vertices.len() as u32;
        let color_full = color.to_bytes();
        let color_fade = [color_full[0], color_full[1], color_full[2], 0u8];

        let mut outer_points = Vec::with_capacity(n);
        for i in 0..n {
            let prev = inner_points[(i + n - 1) % n];
            let curr = inner_points[i];
            let next = inner_points[(i + 1) % n];

            let dx_in = curr.x() - prev.x();
            let dy_in = curr.y() - prev.y();
            let dx_out = next.x() - curr.x();
            let dy_out = next.y() - curr.y();

            let len_in = (dx_in * dx_in + dy_in * dy_in).sqrt().max(0.0001);
            let len_out = (dx_out * dx_out + dy_out * dy_out).sqrt().max(0.0001);

            let nx = (-dy_in / len_in + -dy_out / len_out) * 0.5;
            let ny = (dx_in / len_in + dx_out / len_out) * 0.5;
            let nlen = (nx * nx + ny * ny).sqrt().max(0.0001);

            const AA_FRINGE: f32 = 1.0;
            let offset_x = nx / nlen * AA_FRINGE;
            let offset_y = ny / nlen * AA_FRINGE;

            outer_points.push(Vec2::new(curr.x() + offset_x, curr.y() + offset_y));
        }

        for &p in &inner_points {
            self.vertices.push(Vertex::position_only(p, color_full));
        }
        for &p in &outer_points {
            self.vertices.push(Vertex::position_only(p, color_fade));
        }

        for i in 1..(n as u32 - 1) {
            self.indices.extend_from_slice(&[
                vertex_offset,
                vertex_offset + i,
                vertex_offset + i + 1,
            ]);
        }

        let outer_start = vertex_offset + n as u32;
        for i in 0..n {
            let j = (i + 1) % n;
            let ii = vertex_offset + i as u32;
            let ij = vertex_offset + j as u32;
            let oi = outer_start + i as u32;
            let oj = outer_start + j as u32;
            self.indices.extend_from_slice(&[ii, ij, oj, ii, oj, oi]);
        }

        self.scratch_points = inner_points;
    }

    /// Add a rounded rectangle border stroke (vertex-based).
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

        let segments_per_corner = ((r * std::f32::consts::PI * 0.5 / 1.5).ceil() as u32).max(2);
        let half_t = thickness * 0.5;

        let outer_min = Vec2::new(bounds.min.x() - half_t, bounds.min.y() - half_t);
        let outer_max = Vec2::new(bounds.max.x() + half_t, bounds.max.y() + half_t);
        let outer_r = r + half_t;

        let inner_min = Vec2::new(bounds.min.x() + half_t, bounds.min.y() + half_t);
        let inner_max = Vec2::new(bounds.max.x() - half_t, bounds.max.y() - half_t);
        let inner_r = (r - half_t).max(0.0);

        generate_rounded_rect_points_into(
            outer_min,
            outer_max,
            outer_r,
            segments_per_corner,
            &mut self.scratch_points,
        );
        let outer_points = std::mem::take(&mut self.scratch_points);
        generate_rounded_rect_points_into(
            inner_min,
            inner_max,
            inner_r,
            segments_per_corner,
            &mut self.scratch_points,
        );
        let inner_points = std::mem::take(&mut self.scratch_points);

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

        self.scratch_points = inner_points;
    }

    /// Add a rounded rectangle border stroke with anti-aliased edges (vertex-based).
    pub fn add_rounded_rect_stroke_aa(
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

        let segments_per_corner = ((r * std::f32::consts::PI * 0.5 / 1.5).ceil() as u32).max(2);
        let half_t = thickness * 0.5;

        let outer_aa_min = Vec2::new(bounds.min.x() - half_t - 1.0, bounds.min.y() - half_t - 1.0);
        let outer_aa_max = Vec2::new(bounds.max.x() + half_t + 1.0, bounds.max.y() + half_t + 1.0);
        let outer_aa_r = r + half_t + 1.0;

        let outer_min = Vec2::new(bounds.min.x() - half_t, bounds.min.y() - half_t);
        let outer_max = Vec2::new(bounds.max.x() + half_t, bounds.max.y() + half_t);
        let outer_r = r + half_t;

        let inner_min = Vec2::new(bounds.min.x() + half_t, bounds.min.y() + half_t);
        let inner_max = Vec2::new(bounds.max.x() - half_t, bounds.max.y() - half_t);
        let inner_r = (r - half_t).max(0.0);

        let inner_aa_min = Vec2::new(bounds.min.x() + half_t + 1.0, bounds.min.y() + half_t + 1.0);
        let inner_aa_max = Vec2::new(bounds.max.x() - half_t - 1.0, bounds.max.y() - half_t - 1.0);
        let inner_aa_r = (r - half_t - 1.0).max(0.0);

        generate_rounded_rect_points_into(
            outer_aa_min,
            outer_aa_max,
            outer_aa_r,
            segments_per_corner,
            &mut self.scratch_points,
        );
        let outer_aa_points = std::mem::take(&mut self.scratch_points);

        generate_rounded_rect_points_into(
            outer_min,
            outer_max,
            outer_r,
            segments_per_corner,
            &mut self.scratch_points,
        );
        let outer_points = std::mem::take(&mut self.scratch_points);

        generate_rounded_rect_points_into(
            inner_min,
            inner_max,
            inner_r,
            segments_per_corner,
            &mut self.scratch_points,
        );
        let inner_points = std::mem::take(&mut self.scratch_points);

        let has_inner_aa = inner_aa_r > 0.0
            && inner_aa_min.x() < inner_aa_max.x()
            && inner_aa_min.y() < inner_aa_max.y();
        let inner_aa_points = if has_inner_aa {
            generate_rounded_rect_points_into(
                inner_aa_min,
                inner_aa_max,
                inner_aa_r,
                segments_per_corner,
                &mut self.scratch_points,
            );
            std::mem::take(&mut self.scratch_points)
        } else {
            inner_points.clone()
        };

        let n = outer_points.len();
        debug_assert_eq!(outer_aa_points.len(), n);
        debug_assert_eq!(inner_points.len(), n);

        self.set_texture(TextureId::NONE);

        let color_full = color.to_bytes();
        let color_fade = [color_full[0], color_full[1], color_full[2], 0u8];
        let vertex_offset = self.vertices.len() as u32;

        for &p in &outer_aa_points {
            self.vertices.push(Vertex::position_only(p, color_fade));
        }
        for &p in &outer_points {
            self.vertices.push(Vertex::position_only(p, color_full));
        }
        for &p in &inner_points {
            self.vertices.push(Vertex::position_only(p, color_full));
        }
        for &p in &inner_aa_points {
            self.vertices.push(Vertex::position_only(p, color_fade));
        }

        let outer_aa_base = vertex_offset;
        let outer_base = vertex_offset + n as u32;
        let inner_base = vertex_offset + 2 * n as u32;
        let inner_aa_base = vertex_offset + 3 * n as u32;

        for i in 0..n {
            let j = (i + 1) % n;
            let oai = outer_aa_base + i as u32;
            let oaj = outer_aa_base + j as u32;
            let oi = outer_base + i as u32;
            let oj = outer_base + j as u32;
            let ii = inner_base + i as u32;
            let ij = inner_base + j as u32;
            let iai = inner_aa_base + i as u32;
            let iaj = inner_aa_base + j as u32;

            self.indices.extend_from_slice(&[oai, oaj, oj, oai, oj, oi]);
            self.indices.extend_from_slice(&[oi, oj, ij, oi, ij, ii]);
            self.indices.extend_from_slice(&[ii, ij, iaj, ii, iaj, iai]);
        }

        self.scratch_points = inner_points;
    }

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

    /// Flush pending instance data into a batch.
    fn flush_instance_batch(&mut self) {
        let total = self.instances.len() as u32;
        let prev_end = self
            .pending_instance_batches
            .last()
            .map(|b| b.instance_start + b.instance_count)
            .unwrap_or(0);
        let count = total - prev_end;
        if count > 0 {
            self.pending_instance_batches.push(PendingInstanceBatch {
                texture: self.current_texture,
                clip_rect: self.current_clip,
                z_index: self.current_z,
                instance_start: prev_end,
                instance_count: count,
            });
        }
    }

    /// Flush pending vertex data into a batch.
    fn flush_vertex_batch(&mut self) {
        let total = self.indices.len() as u32;
        let prev_end = self
            .pending_vertex_batches
            .last()
            .map(|b| b.index_start + b.index_count)
            .unwrap_or(0);
        let count = total - prev_end;
        if count > 0 {
            self.pending_vertex_batches.push(PendingVertexBatch {
                texture: self.current_texture,
                clip_rect: self.current_clip,
                z_index: self.current_z,
                index_start: prev_end,
                index_count: count,
            });
        }
    }

    /// Finalize the draw list, sorting by Z-index and converting to GPU commands.
    pub fn finalize(&mut self) {
        self.flush_instance_batch();
        self.flush_vertex_batch();

        // Combine all batches into a unified list for z-sorting
        let mut all_batches: Vec<SortedBatch> = Vec::new();

        for batch in &self.pending_instance_batches {
            all_batches.push(SortedBatch {
                z_index: batch.z_index,
                texture: batch.texture,
                clip_rect: batch.clip_rect,
                is_instanced: true,
                offset: batch.instance_start,
                count: batch.instance_count,
            });
        }

        for batch in &self.pending_vertex_batches {
            all_batches.push(SortedBatch {
                z_index: batch.z_index,
                texture: batch.texture,
                clip_rect: batch.clip_rect,
                is_instanced: false,
                offset: batch.index_start,
                count: batch.index_count,
            });
        }

        // Sort by z-index (stable sort preserves order within same z)
        all_batches.sort_by_key(|b| b.z_index);

        // Convert to GPU commands
        self.commands.clear();
        self.commands.extend(all_batches.iter().map(|batch| {
            let clip_rect = if batch.clip_rect.min.x() < f32::MAX / 2.0 {
                Some(batch.clip_rect.to_clip_array())
            } else {
                None
            };
            if batch.is_instanced {
                DrawCmd::instanced(batch.offset, batch.count, clip_rect, batch.texture)
            } else {
                DrawCmd::vertex(batch.offset, batch.count, clip_rect, batch.texture)
            }
        }));
    }

    /// Check if the draw list is empty.
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty() && self.indices.is_empty()
    }

    /// Get the total number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Get the total number of indices.
    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    /// Get the total number of instances.
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Get the number of draw commands.
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    /// Get instance data as bytes for GPU upload.
    pub fn instance_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.instances)
    }

    /// Get vertex data as bytes for GPU upload.
    pub fn vertex_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.vertices)
    }

    /// Get index data as bytes for GPU upload.
    pub fn index_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.indices)
    }

    /// Get the instances slice.
    pub fn instances(&self) -> &[InstanceData] {
        &self.instances
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

    /// Convert the current clip rect to a [f32; 4] array for instance data.
    fn clip_to_array(&self) -> [f32; 4] {
        if self.current_clip.min.x() < f32::MAX / 2.0 {
            self.current_clip.to_clip_array()
        } else {
            InstanceData::CLIP_NONE
        }
    }
}

/// Unified batch for z-sorting.
struct SortedBatch {
    z_index: u32,
    texture: TextureId,
    clip_rect: Rect2D,
    is_instanced: bool,
    offset: u32,
    count: u32,
}

/// Generate the outline points of a rounded rectangle into an existing buffer.
///
/// Points are in counter-clockwise order starting from the top-left corner.
/// Each corner arc uses `segments_per_corner` subdivisions.
fn generate_rounded_rect_points_into(
    min: Vec2,
    max: Vec2,
    r: f32,
    segments_per_corner: u32,
    out: &mut Vec<Vec2>,
) {
    out.clear();
    out.reserve((segments_per_corner as usize + 1) * 4);

    // Top-left corner
    let cx = min.x() + r;
    let cy = min.y() + r;
    for i in 0..=segments_per_corner {
        let angle = std::f32::consts::PI
            + (i as f32 / segments_per_corner as f32) * std::f32::consts::FRAC_PI_2;
        out.push(Vec2::new(cx + r * angle.cos(), cy + r * angle.sin()));
    }

    // Top-right corner
    let cx = max.x() - r;
    let cy = min.y() + r;
    for i in 0..=segments_per_corner {
        let angle = std::f32::consts::FRAC_PI_2 * 3.0
            + (i as f32 / segments_per_corner as f32) * std::f32::consts::FRAC_PI_2;
        out.push(Vec2::new(cx + r * angle.cos(), cy + r * angle.sin()));
    }

    // Bottom-right corner
    let cx = max.x() - r;
    let cy = max.y() - r;
    for i in 0..=segments_per_corner {
        let angle = (i as f32 / segments_per_corner as f32) * std::f32::consts::FRAC_PI_2;
        out.push(Vec2::new(cx + r * angle.cos(), cy + r * angle.sin()));
    }

    // Bottom-left corner
    let cx = min.x() + r;
    let cy = max.y() - r;
    for i in 0..=segments_per_corner {
        let angle = std::f32::consts::FRAC_PI_2
            + (i as f32 / segments_per_corner as f32) * std::f32::consts::FRAC_PI_2;
        out.push(Vec2::new(cx + r * angle.cos(), cy + r * angle.sin()));
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
    fn test_add_rect() {
        let mut list = DrawList::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0));

        list.add_rect(bounds, Color::RED);
        list.finalize();

        assert_eq!(list.instance_count(), 1);
        assert_eq!(list.command_count(), 1);
        assert!(list.commands()[0].is_instanced);
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

        assert_eq!(list.instance_count(), 2);
        assert_eq!(list.command_count(), 1);
        let cmd = &list.commands()[0];
        assert!(cmd.is_instanced);
        assert_eq!(cmd.count, 2);
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

        assert_eq!(list.vertex_count(), 4);
        assert_eq!(list.index_count(), 6);
        assert_eq!(list.command_count(), 1);
        assert!(!list.commands()[0].is_instanced);
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
        assert!(!list.commands()[0].is_instanced);

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

    #[test]
    fn test_instance_data_position() {
        let mut list = DrawList::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(50.0, 75.0), Vec2::new(100.0, 40.0));
        list.add_rect(bounds, Color::RED);
        list.finalize();

        let inst = &list.instances()[0];
        assert_eq!(inst.position, [50.0, 75.0]);
        assert_eq!(inst.size, [100.0, 40.0]);
    }

    #[test]
    fn test_textured_rect_instance() {
        let mut list = DrawList::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(10.0, 20.0), Vec2::new(50.0, 60.0));
        let uv = Rect2D::from_origin_size(Vec2::new(0.1, 0.2), Vec2::new(0.5, 0.6));
        list.add_textured_rect(bounds, uv, Color::WHITE, TextureId::FONT_ATLAS);
        list.finalize();

        let inst = &list.instances()[0];
        assert_eq!(inst.position, [10.0, 20.0]);
        assert_eq!(inst.size, [50.0, 60.0]);
        assert_eq!(inst.uv_min, [0.1, 0.2]);
        assert_eq!(inst.uv_max, [0.6, 0.8]);
    }

    #[test]
    fn test_instance_count_100_same_batch() {
        let mut list = DrawList::new();
        for i in 0..100 {
            list.add_rect(
                Rect2D::from_origin_size(Vec2::new((i as f32) * 10.0, 0.0), Vec2::new(10.0, 10.0)),
                Color::RED,
            );
        }
        list.finalize();

        assert_eq!(list.instance_count(), 100);
        assert_eq!(list.command_count(), 1);
        assert!(list.commands()[0].is_instanced);
        assert_eq!(list.commands()[0].count, 100);
    }

    #[test]
    fn test_texture_change_causes_batch_break() {
        let mut list = DrawList::new();

        for _ in 0..10 {
            list.add_textured_rect(
                Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)),
                Rect2D::from_origin_size(Vec2::ZERO, Vec2::new(1.0, 1.0)),
                Color::WHITE,
                TextureId::FONT_ATLAS,
            );
        }
        for _ in 0..10 {
            list.add_rect(
                Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)),
                Color::RED,
            );
        }
        list.finalize();

        assert_eq!(list.command_count(), 2);
    }

    #[test]
    fn test_clip_change_causes_batch_break() {
        let mut list = DrawList::new();

        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::RED,
        );
        list.set_clip(Rect2D::from_origin_size(
            Vec2::new(0.0, 0.0),
            Vec2::new(50.0, 50.0),
        ));
        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::RED,
        );
        list.finalize();

        assert_eq!(list.command_count(), 2);
    }

    #[test]
    fn test_empty_draw_list_no_crash() {
        let mut list = DrawList::new();
        list.finalize();
        assert!(list.is_empty());
        assert_eq!(list.command_count(), 0);
    }

    #[test]
    fn test_10000_instances() {
        let mut list = DrawList::new();
        for i in 0..10_000 {
            list.add_rect(
                Rect2D::from_origin_size(
                    Vec2::new((i % 100) as f32 * 10.0, (i / 100) as f32 * 10.0),
                    Vec2::new(8.0, 8.0),
                ),
                Color::RED,
            );
        }
        list.finalize();

        assert_eq!(list.instance_count(), 10_000);
        assert_eq!(list.command_count(), 1);
    }

    #[test]
    fn test_z_ordering() {
        let mut list = DrawList::new();

        list.set_z_index(2);
        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::RED,
        );
        list.set_z_index(0);
        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::GREEN,
        );
        list.set_z_index(1);
        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::BLUE,
        );
        list.finalize();

        assert_eq!(list.command_count(), 3);
    }

    #[test]
    fn test_gradient_rect_uses_vertices() {
        let mut list = DrawList::new();
        list.add_gradient_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::RED,
            Color::GREEN,
            Color::BLUE,
            Color::WHITE,
        );
        list.finalize();

        assert_eq!(list.vertex_count(), 4);
        assert_eq!(list.index_count(), 6);
        assert_eq!(list.instance_count(), 0);
        assert!(!list.commands()[0].is_instanced);
    }

    #[test]
    fn test_mixed_instanced_and_vertex() {
        let mut list = DrawList::new();

        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::RED,
        );
        list.add_rounded_rect_aa(
            Rect2D::from_origin_size(Vec2::new(10.0, 10.0), Vec2::new(80.0, 30.0)),
            Color::GREEN,
            8.0,
        );
        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(200.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::BLUE,
        );
        list.finalize();

        assert!(list.instances().len() > 0);
        assert!(list.vertices().len() > 0);
    }

    #[test]
    fn test_unit_quad_geometry_correct() {
        let mut list = DrawList::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(10.0, 20.0), Vec2::new(30.0, 40.0));
        list.add_rect(bounds, Color::RED);
        list.finalize();

        let inst = &list.instances()[0];
        assert_eq!(inst.position, [10.0, 20.0]);
        assert_eq!(inst.size, [30.0, 40.0]);
        assert_eq!(inst.uv_min, [0.0, 0.0]);
        assert_eq!(inst.uv_max, [1.0, 1.0]);
    }

    #[test]
    fn test_clip_rect_in_instance() {
        let mut list = DrawList::new();
        list.set_clip(Rect2D::from_origin_size(
            Vec2::new(5.0, 10.0),
            Vec2::new(100.0, 200.0),
        ));
        list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 300.0)),
            Color::RED,
        );
        list.finalize();

        let inst = &list.instances()[0];
        assert_eq!(inst.clip_rect, [5.0, 10.0, 100.0, 200.0]);
    }
}
