//! Immediate-mode 3D debug drawing API.
//!
//! Provides line/shape drawing for debugging and visualization.
//! All drawings are cleared each frame (immediate mode semantics).
//!
//! # Example
//! ```ignore
//! // Draw a line from player to target
//! app.debug_draw.line(player_pos, target_pos, Color::RED);
//!
//! // Draw a wireframe box around a collider
//! app.debug_draw.box_wireframe(center, half_extents, Color::GREEN);
//!
//! // Draw a sphere at impact point
//! app.debug_draw.sphere_wireframe(impact_point, 0.5, Color::YELLOW, 16);
//! ```

use katla_gfx::{IndexBuffer, IndexType, MaterialHandle, MeshHandle, VertexBuffer, VulkanContext};
use katla_math::{Color, Mat4, Vec3};
use std::rc::Rc;

/// Maximum number of debug lines per frame (pre-allocated buffer size)
const MAX_DEBUG_LINES: usize = 65536;
/// Maximum number of debug vertices (2 per line)
const MAX_DEBUG_VERTICES: usize = MAX_DEBUG_LINES * 2;
/// Maximum number of debug indices (2 per line for line list topology)
const MAX_DEBUG_INDICES: usize = MAX_DEBUG_LINES * 2;

/// Vertex format for debug drawing.
///
/// Matches GizmoVertex format: position (RGB32f) + color (RGB32f).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugVertex {
    /// Position in world space.
    pub position: [f32; 3],
    /// Color (RGB, 0.0-1.0).
    pub color: [f32; 3],
}

impl DebugVertex {
    /// Create a new debug vertex.
    #[inline]
    pub fn new(position: Vec3, color: Color) -> Self {
        Self {
            position: [position.x(), position.y(), position.z()],
            color: [color.r, color.g, color.b],
        }
    }
}

/// Immediate-mode 3D debug drawing context.
///
/// Accumulates debug primitives each frame and renders them as lines.
/// All drawings are cleared at the start of each frame.
pub struct DebugDraw {
    /// Accumulated vertices for this frame.
    vertices: Vec<DebugVertex>,
    /// Accumulated indices for this frame.
    indices: Vec<u32>,
    /// Material handle for debug lines.
    material_handle: Option<MaterialHandle>,
}

impl DebugDraw {
    /// Create a new debug draw context.
    pub fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(MAX_DEBUG_VERTICES),
            indices: Vec::with_capacity(MAX_DEBUG_INDICES),
            material_handle: None,
        }
    }

    // =========================================================================
    // Core Drawing Methods
    // =========================================================================

    /// Draw a line from `from` to `to` with the specified color.
    pub fn line(&mut self, from: Vec3, to: Vec3, color: Color) {
        if self.vertices.len() + 2 > MAX_DEBUG_VERTICES {
            log::warn!("DebugDraw buffer full, dropping line");
            return;
        }

        let base_idx = self.vertices.len() as u32;
        self.vertices.push(DebugVertex::new(from, color));
        self.vertices.push(DebugVertex::new(to, color));
        self.indices.push(base_idx);
        self.indices.push(base_idx + 1);
    }

    /// Draw a ray from `origin` in direction `dir` with `length` and color.
    pub fn ray(&mut self, origin: Vec3, dir: Vec3, length: f32, color: Color) {
        let end = origin + dir.normalize() * length;
        self.line(origin, end, color);
    }

    /// Draw a line with different start and end colors (for gradients).
    pub fn line_gradient(&mut self, from: Vec3, to: Vec3, from_color: Color, to_color: Color) {
        if self.vertices.len() + 2 > MAX_DEBUG_VERTICES {
            log::warn!("DebugDraw buffer full, dropping line");
            return;
        }

        let base_idx = self.vertices.len() as u32;
        self.vertices.push(DebugVertex::new(from, from_color));
        self.vertices.push(DebugVertex::new(to, to_color));
        self.indices.push(base_idx);
        self.indices.push(base_idx + 1);
    }

    // =========================================================================
    // Wireframe Primitives
    // =========================================================================

    /// Draw a wireframe box (axis-aligned).
    ///
    /// Draws 12 lines connecting the 8 corners of the box.
    pub fn box_wireframe(&mut self, center: Vec3, half_extents: Vec3, color: Color) {
        let hx = half_extents.x();
        let hy = half_extents.y();
        let hz = half_extents.z();

        // 8 corners of the box
        let corners = [
            center + Vec3::new(-hx, -hy, -hz),
            center + Vec3::new(hx, -hy, -hz),
            center + Vec3::new(hx, hy, -hz),
            center + Vec3::new(-hx, hy, -hz),
            center + Vec3::new(-hx, -hy, hz),
            center + Vec3::new(hx, -hy, hz),
            center + Vec3::new(hx, hy, hz),
            center + Vec3::new(-hx, hy, hz),
        ];

        // Bottom face (0-1-2-3)
        self.line(corners[0], corners[1], color);
        self.line(corners[1], corners[2], color);
        self.line(corners[2], corners[3], color);
        self.line(corners[3], corners[0], color);

        // Top face (4-5-6-7)
        self.line(corners[4], corners[5], color);
        self.line(corners[5], corners[6], color);
        self.line(corners[6], corners[7], color);
        self.line(corners[7], corners[4], color);

        // Vertical edges
        self.line(corners[0], corners[4], color);
        self.line(corners[1], corners[5], color);
        self.line(corners[2], corners[6], color);
        self.line(corners[3], corners[7], color);
    }

    /// Draw a wireframe box from min and max corners.
    pub fn box_wireframe_from_bounds(&mut self, min: Vec3, max: Vec3, color: Color) {
        let center = (min + max) * 0.5;
        let half_extents = (max - min) * 0.5;
        self.box_wireframe(center, half_extents, color);
    }

    /// Draw a wireframe sphere using 3 circles (XY, XZ, YZ planes).
    ///
    /// `segments` controls the smoothness of the circles.
    pub fn sphere_wireframe(&mut self, center: Vec3, radius: f32, color: Color, segments: u32) {
        // XY plane (Z = 0)
        self.circle_wireframe(center, radius, color, segments, 0);
        // XZ plane (Y = 0)
        self.circle_wireframe(center, radius, color, segments, 1);
        // YZ plane (X = 0)
        self.circle_wireframe(center, radius, color, segments, 2);
    }

    /// Draw a wireframe circle in a specific plane.
    ///
    /// `plane`: 0 = XY, 1 = XZ, 2 = YZ
    fn circle_wireframe(
        &mut self,
        center: Vec3,
        radius: f32,
        color: Color,
        segments: u32,
        plane: u32,
    ) {
        for i in 0..segments {
            let angle1 = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let angle2 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;

            let (p1, p2) = match plane {
                0 => {
                    // XY plane
                    (
                        center + Vec3::new(angle1.cos() * radius, angle1.sin() * radius, 0.0),
                        center + Vec3::new(angle2.cos() * radius, angle2.sin() * radius, 0.0),
                    )
                }
                1 => {
                    // XZ plane
                    (
                        center + Vec3::new(angle1.cos() * radius, 0.0, angle1.sin() * radius),
                        center + Vec3::new(angle2.cos() * radius, 0.0, angle2.sin() * radius),
                    )
                }
                _ => {
                    // YZ plane
                    (
                        center + Vec3::new(0.0, angle1.cos() * radius, angle1.sin() * radius),
                        center + Vec3::new(0.0, angle2.cos() * radius, angle2.sin() * radius),
                    )
                }
            };

            self.line(p1, p2, color);
        }
    }

    /// Draw a wireframe frustum from a view-projection matrix.
    ///
    /// Extracts the 8 frustum corners and draws the 12 edges.
    pub fn frustum(&mut self, view_proj: Mat4, color: Color) {
        // Calculate inverse view-projection matrix
        let inv = view_proj.inverse();

        // NDC corners of the frustum
        let ndc_corners = [
            Vec3::new(-1.0, -1.0, 0.0), // Near bottom-left
            Vec3::new(1.0, -1.0, 0.0),  // Near bottom-right
            Vec3::new(1.0, 1.0, 0.0),   // Near top-right
            Vec3::new(-1.0, 1.0, 0.0),  // Near top-left
            Vec3::new(-1.0, -1.0, 1.0), // Far bottom-left
            Vec3::new(1.0, -1.0, 1.0),  // Far bottom-right
            Vec3::new(1.0, 1.0, 1.0),   // Far top-right
            Vec3::new(-1.0, 1.0, 1.0),  // Far top-left
        ];

        // Transform NDC corners to world space
        let world_corners: Vec<Vec3> = ndc_corners
            .iter()
            .map(|ndc| {
                let clip = inv.clone() * katla_math::Vec4::new(ndc.x(), ndc.y(), ndc.z(), 1.0);
                Vec3::new(
                    clip.x() / clip.w(),
                    clip.y() / clip.w(),
                    clip.z() / clip.w(),
                )
            })
            .collect();

        // Near plane
        self.line(world_corners[0], world_corners[1], color);
        self.line(world_corners[1], world_corners[2], color);
        self.line(world_corners[2], world_corners[3], color);
        self.line(world_corners[3], world_corners[0], color);

        // Far plane
        self.line(world_corners[4], world_corners[5], color);
        self.line(world_corners[5], world_corners[6], color);
        self.line(world_corners[6], world_corners[7], color);
        self.line(world_corners[7], world_corners[4], color);

        // Edges connecting near to far
        self.line(world_corners[0], world_corners[4], color);
        self.line(world_corners[1], world_corners[5], color);
        self.line(world_corners[2], world_corners[6], color);
        self.line(world_corners[3], world_corners[7], color);
    }

    /// Draw coordinate axes at a position (XYZ = RGB).
    pub fn axes(&mut self, origin: Vec3, length: f32) {
        self.line(origin, origin + Vec3::new(length, 0.0, 0.0), Color::RED);
        self.line(origin, origin + Vec3::new(0.0, length, 0.0), Color::GREEN);
        self.line(origin, origin + Vec3::new(0.0, 0.0, length), Color::BLUE);
    }

    /// Draw a wireframe grid on the XZ plane at Y=0.
    pub fn grid(&mut self, center: Vec3, size: f32, divisions: u32, color: Color) {
        let half = size * 0.5;
        let step = size / divisions as f32;

        // Lines parallel to X axis
        for i in 0..=divisions {
            let z = center.z() - half + (i as f32 * step);
            self.line(
                Vec3::new(center.x() - half, center.y(), z),
                Vec3::new(center.x() + half, center.y(), z),
                color,
            );
        }

        // Lines parallel to Z axis
        for i in 0..=divisions {
            let x = center.x() - half + (i as f32 * step);
            self.line(
                Vec3::new(x, center.y(), center.z() - half),
                Vec3::new(x, center.y(), center.z() + half),
                color,
            );
        }
    }

    // =========================================================================
    // Frame Lifecycle
    // =========================================================================

    /// Clear all accumulated primitives.
    ///
    /// Called automatically at the start of each frame.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    /// Check if there are any debug primitives to draw.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Get the number of lines queued for drawing.
    pub fn line_count(&self) -> usize {
        self.indices.len() / 2
    }

    // =========================================================================
    // GPU Buffer Creation
    // =========================================================================

    /// Create GPU buffers from accumulated primitives.
    ///
    /// Returns (vertex_buffer, index_buffer) or None if empty.
    pub fn create_buffers(
        &self,
        context: &Rc<VulkanContext>,
    ) -> Option<(VertexBuffer, IndexBuffer)> {
        if self.is_empty() {
            return None;
        }

        // Create vertex buffer
        let vertex_data = unsafe {
            std::slice::from_raw_parts(
                self.vertices.as_ptr() as *const u8,
                self.vertices.len() * std::mem::size_of::<DebugVertex>(),
            )
        };
        let vertex_count = self.vertices.len() as u32;
        let mut vertex_buffer =
            VertexBuffer::new(context.clone(), vertex_data.len() as u64, vertex_count);
        vertex_buffer.upload_data(vertex_data);

        // Create index buffer
        let index_data = unsafe {
            std::slice::from_raw_parts(
                self.indices.as_ptr() as *const u8,
                self.indices.len() * std::mem::size_of::<u32>(),
            )
        };
        let index_count = self.indices.len() as u32;
        let mut index_buffer = IndexBuffer::new(
            context.clone(),
            index_data.len() as u64,
            IndexType::Uint32,
            index_count,
        );
        index_buffer.upload_data(index_data);

        Some((vertex_buffer, index_buffer))
    }

    // =========================================================================
    // Renderer Integration
    // =========================================================================

    /// Get the material handle for rendering.
    pub fn material_handle(&self) -> Option<MaterialHandle> {
        self.material_handle
    }

    /// Set the material handle (called during initialization).
    pub fn set_material_handle(&mut self, handle: MaterialHandle) {
        self.material_handle = Some(handle);
    }
}

impl Default for DebugDraw {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the vertex binding for debug vertices.
pub fn debug_vertex_binding() -> katla_gfx::VertexBinding {
    katla_gfx::VertexBinding {
        formats: vec![
            katla_gfx::VertexFormat::RGB32f, // position
            katla_gfx::VertexFormat::RGB32f, // color
        ],
    }
}
