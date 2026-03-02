//! Cube primitive generation.

use crate::vertex::VertexPBR;

/// Generates a unit cube with the given size.
///
/// The cube is centered at the origin with faces aligned to axes.
/// Each face has its own vertices for proper normals and tangents.
///
/// # Arguments
/// * `size` - Dimensions [width, height, depth] of the cube
///
/// # Returns
/// A tuple of (vertices, indices) ready for mesh creation.
///
/// # Winding Order
/// Uses counter-clockwise (CCW) winding for front faces.
pub fn generate_cube(size: [f32; 3]) -> (Vec<VertexPBR>, Vec<u32>) {
    let hx = size[0] * 0.5;
    let hy = size[1] * 0.5;
    let hz = size[2] * 0.5;

    // 6 faces, 4 vertices per face = 24 vertices
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    // Face definitions: (normal, tangent, 4 corner positions, 4 UVs)
    // Each face is defined in CCW order when viewed from outside

    // Front face (+Z)
    let face_verts = [
        VertexPBR::new(
            [-hx, -hy, hz],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        VertexPBR::new(
            [hx, -hy, hz],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        VertexPBR::new(
            [hx, hy, hz],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        VertexPBR::new(
            [-hx, hy, hz],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    add_face(&mut vertices, &mut indices, &face_verts);

    // Back face (-Z)
    let face_verts = [
        VertexPBR::new(
            [hx, -hy, -hz],
            [0.0, 0.0, -1.0],
            [-1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        VertexPBR::new(
            [-hx, -hy, -hz],
            [0.0, 0.0, -1.0],
            [-1.0, 0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        VertexPBR::new(
            [-hx, hy, -hz],
            [0.0, 0.0, -1.0],
            [-1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        VertexPBR::new(
            [hx, hy, -hz],
            [0.0, 0.0, -1.0],
            [-1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    add_face(&mut vertices, &mut indices, &face_verts);

    // Top face (+Y)
    let face_verts = [
        VertexPBR::new(
            [-hx, hy, hz],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        VertexPBR::new(
            [hx, hy, hz],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        VertexPBR::new(
            [hx, hy, -hz],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        VertexPBR::new(
            [-hx, hy, -hz],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    add_face(&mut vertices, &mut indices, &face_verts);

    // Bottom face (-Y)
    let face_verts = [
        VertexPBR::new(
            [-hx, -hy, -hz],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        VertexPBR::new(
            [hx, -hy, -hz],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        VertexPBR::new(
            [hx, -hy, hz],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        VertexPBR::new(
            [-hx, -hy, hz],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    add_face(&mut vertices, &mut indices, &face_verts);

    // Right face (+X)
    let face_verts = [
        VertexPBR::new(
            [hx, -hy, hz],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0, 1.0],
            [0.0, 1.0],
        ),
        VertexPBR::new(
            [hx, -hy, -hz],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0, 1.0],
            [1.0, 1.0],
        ),
        VertexPBR::new(
            [hx, hy, -hz],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0, 1.0],
            [1.0, 0.0],
        ),
        VertexPBR::new(
            [hx, hy, hz],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    add_face(&mut vertices, &mut indices, &face_verts);

    // Left face (-X)
    let face_verts = [
        VertexPBR::new(
            [-hx, -hy, -hz],
            [-1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 1.0],
        ),
        VertexPBR::new(
            [-hx, -hy, hz],
            [-1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0],
        ),
        VertexPBR::new(
            [-hx, hy, hz],
            [-1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 0.0],
        ),
        VertexPBR::new(
            [-hx, hy, -hz],
            [-1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    add_face(&mut vertices, &mut indices, &face_verts);

    (vertices, indices)
}

/// Adds a quad face with 2 triangles (CCW winding).
fn add_face(vertices: &mut Vec<VertexPBR>, indices: &mut Vec<u32>, face: &[VertexPBR; 4]) {
    let base = vertices.len() as u32;
    vertices.extend_from_slice(face);

    // Two triangles in CCW order
    indices.extend_from_slice(&[base, base + 1, base + 2]);
    indices.extend_from_slice(&[base, base + 2, base + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cube_vertex_count() {
        let (vertices, _) = generate_cube([1.0, 1.0, 1.0]);
        assert_eq!(vertices.len(), 24); // 6 faces * 4 vertices
    }

    #[test]
    fn test_cube_index_count() {
        let (_, indices) = generate_cube([1.0, 1.0, 1.0]);
        assert_eq!(indices.len(), 36); // 6 faces * 2 triangles * 3 indices
    }

    #[test]
    fn test_cube_bounds() {
        let (vertices, _) = generate_cube([2.0, 2.0, 2.0]);
        for v in &vertices {
            assert!(v.position[0].abs() <= 1.0);
            assert!(v.position[1].abs() <= 1.0);
            assert!(v.position[2].abs() <= 1.0);
        }
    }

    #[test]
    fn test_cube_normals_normalized() {
        let (vertices, _) = generate_cube([1.0, 1.0, 1.0]);
        for v in &vertices {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((len - 1.0).abs() < 1e-6);
        }
    }
}
