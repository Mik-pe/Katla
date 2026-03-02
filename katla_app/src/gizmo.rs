//! 3D Transform Gizmo for entity manipulation.
//!
//! Provides visual handles for translating, rotating, and scaling selected entities.

use katla_gfx::{VertexBinding, VertexFormat};
use katla_math::{Color, Vec3};

/// Gizmo axis identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoAxis {
    /// X axis (red)
    X,
    /// Y axis (green)
    Y,
    /// Z axis (blue)
    Z,
    /// XY plane
    XY,
    /// XZ plane
    XZ,
    /// YZ plane
    YZ,
}

/// Gizmo mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GizmoMode {
    /// Translate mode
    #[default]
    Translate,
    /// Rotate mode
    Rotate,
    /// Scale mode
    Scale,
}

/// Gizmo state for tracking interaction.
#[derive(Debug, Clone, Default)]
pub struct GizmoState {
    /// Currently hovered axis (if any)
    pub hovered_axis: Option<GizmoAxis>,
    /// Currently dragged axis (if any)
    pub dragged_axis: Option<GizmoAxis>,
    /// Starting position when drag began
    pub drag_start: Option<Vec3>,
    /// Current gizmo mode
    pub mode: GizmoMode,
    /// Gizmo size in world units
    pub size: f32,
}

impl GizmoState {
    /// Create a new gizmo state with default settings.
    pub fn new() -> Self {
        Self {
            hovered_axis: None,
            dragged_axis: None,
            drag_start: None,
            mode: GizmoMode::Translate,
            size: 0.5,
        }
    }

    /// Check if gizmo is currently being dragged.
    pub fn is_dragging(&self) -> bool {
        self.dragged_axis.is_some()
    }
}

/// Gizmo vertex for rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GizmoVertex {
    /// Position in local space
    pub position: [f32; 3],
    /// Color (RGB)
    pub color: [f32; 3],
}

impl GizmoVertex {
    pub fn new(position: Vec3, color: Color) -> Self {
        Self {
            position: [position.x(), position.y(), position.z()],
            color: [color.r, color.g, color.b],
        }
    }
}

/// Generate vertices for translation gizmo using triangle geometry.
///
/// Creates 3 colored arrows (X=red, Y=green, Z=blue) with cone heads.
pub fn generate_translate_gizmo(size: f32) -> (Vec<GizmoVertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let shaft_length = size;
    let shaft_radius = size * 0.02;
    let head_length = size * 0.2;
    let head_radius = size * 0.08;

    // Colors for each axis
    let x_color = Color::new(0.9, 0.2, 0.2, 1.0); // Red
    let y_color = Color::new(0.2, 0.9, 0.2, 1.0); // Green
    let z_color = Color::new(0.2, 0.2, 0.9, 1.0); // Blue

    // Generate axis arrows as cylinder shafts + cone heads
    generate_cylinder(
        &mut vertices,
        &mut indices,
        Vec3::new(1.0, 0.0, 0.0),
        x_color,
        shaft_length,
        shaft_radius,
        8,
    );
    generate_cone(
        &mut vertices,
        &mut indices,
        Vec3::new(1.0, 0.0, 0.0) * shaft_length,
        Vec3::new(1.0, 0.0, 0.0),
        x_color,
        head_length,
        head_radius,
        8,
    );

    generate_cylinder(
        &mut vertices,
        &mut indices,
        Vec3::new(0.0, 1.0, 0.0),
        y_color,
        shaft_length,
        shaft_radius,
        8,
    );
    generate_cone(
        &mut vertices,
        &mut indices,
        Vec3::new(0.0, 1.0, 0.0) * shaft_length,
        Vec3::new(0.0, 1.0, 0.0),
        y_color,
        head_length,
        head_radius,
        8,
    );

    generate_cylinder(
        &mut vertices,
        &mut indices,
        Vec3::new(0.0, 0.0, 1.0),
        z_color,
        shaft_length,
        shaft_radius,
        8,
    );
    generate_cone(
        &mut vertices,
        &mut indices,
        Vec3::new(0.0, 0.0, 1.0) * shaft_length,
        Vec3::new(0.0, 0.0, 1.0),
        z_color,
        head_length,
        head_radius,
        8,
    );

    (vertices, indices)
}

/// Generate a cylinder along a direction axis.
fn generate_cylinder(
    vertices: &mut Vec<GizmoVertex>,
    indices: &mut Vec<u32>,
    direction: Vec3,
    color: Color,
    length: f32,
    radius: f32,
    segments: u32,
) {
    // Calculate perpendicular vectors
    let up = if direction.y().abs() < 0.9 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };
    let right = direction.cross(up).normalize();
    let forward = right.cross(direction).normalize();

    let base_idx = vertices.len() as u32;

    // Generate vertices for bottom and top rings
    for ring in 0..2 {
        let y = if ring == 0 { 0.0 } else { length };
        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let offset = right * (angle.cos() * radius) + forward * (angle.sin() * radius);
            let pos = direction * y + offset;
            vertices.push(GizmoVertex::new(pos, color));
        }
    }

    // Generate indices for the cylinder sides
    for i in 0..segments {
        let next = (i + 1) % segments;
        // Two triangles per quad
        // Bottom ring indices: 0..segments
        // Top ring indices: segments..2*segments
        let b0 = base_idx + i;
        let b1 = base_idx + next;
        let t0 = base_idx + segments + i;
        let t1 = base_idx + segments + next;

        // Triangle 1: b0, b1, t0
        indices.push(b0);
        indices.push(b1);
        indices.push(t0);
        // Triangle 2: b1, t1, t0
        indices.push(b1);
        indices.push(t1);
        indices.push(t0);
    }
}

/// Generate a cone at a position pointing along a direction.
fn generate_cone(
    vertices: &mut Vec<GizmoVertex>,
    indices: &mut Vec<u32>,
    base_position: Vec3,
    direction: Vec3,
    color: Color,
    length: f32,
    radius: f32,
    segments: u32,
) {
    // Calculate perpendicular vectors
    let up = if direction.y().abs() < 0.9 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };
    let right = direction.cross(up).normalize();
    let forward = right.cross(direction).normalize();

    let base_idx = vertices.len() as u32;

    // Tip vertex
    let tip = base_position + direction * length;
    vertices.push(GizmoVertex::new(tip, color));

    // Base ring vertices
    for i in 0..segments {
        let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let offset = right * (angle.cos() * radius) + forward * (angle.sin() * radius);
        vertices.push(GizmoVertex::new(base_position + offset, color));
    }

    // Generate indices for cone triangles (tip to base)
    for i in 0..segments {
        let next = (i + 1) % segments;
        let tip_idx = base_idx;
        let curr_idx = base_idx + 1 + i;
        let next_idx = base_idx + 1 + next;

        indices.push(tip_idx);
        indices.push(curr_idx);
        indices.push(next_idx);
    }
}

/// Get the vertex binding for gizmo vertices.
pub fn gizmo_vertex_binding() -> VertexBinding {
    VertexBinding {
        formats: vec![
            VertexFormat::RGB32f, // position
            VertexFormat::RGB32f, // color
        ],
    }
}
