//! Plane primitive generation.

use crate::vertex::VertexPBR;

/// Generates a flat plane on the XZ plane centered at the origin.
///
/// The plane extends from -width/2 to +width/2 on X and -height/2 to +height/2 on Z.
/// Normal points in +Y direction.
///
/// # Arguments
/// * `width` - Width along the X axis
/// * `height` - Height along the Z axis
///
/// # Returns
/// A tuple of (vertices, indices) ready for mesh creation.
///
/// # Winding Order
/// Uses counter-clockwise (CCW) winding for front faces (viewed from +Y).
pub fn generate_plane(width: f32, height: f32) -> (Vec<VertexPBR>, Vec<u32>) {
    let hw = width * 0.5;
    let hh = height * 0.5;

    // 4 corners of the plane
    let vertices = vec![
        // Bottom-left
        VertexPBR::new(
            [-hw, 0.0, -hh],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
        // Bottom-right
        VertexPBR::new(
            [hw, 0.0, -hh],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        // Top-right
        VertexPBR::new(
            [hw, 0.0, hh],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        // Top-left
        VertexPBR::new(
            [-hw, 0.0, hh],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
    ];

    // Two triangles (CCW winding when viewed from above)
    let indices = vec![
        0, 1, 2, // First triangle
        0, 2, 3, // Second triangle
    ];

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plane_vertex_count() {
        let (vertices, _) = generate_plane(1.0, 1.0);
        assert_eq!(vertices.len(), 4);
    }

    #[test]
    fn test_plane_index_count() {
        let (_, indices) = generate_plane(1.0, 1.0);
        assert_eq!(indices.len(), 6); // 2 triangles * 3 indices
    }

    #[test]
    fn test_plane_bounds() {
        let (vertices, _) = generate_plane(2.0, 4.0);
        for v in &vertices {
            assert!(v.position[0].abs() <= 1.0); // width/2
            assert!(v.position[1].abs() < 1e-6); // y = 0
            assert!(v.position[2].abs() <= 2.0); // height/2
        }
    }

    #[test]
    fn test_plane_normals_up() {
        let (vertices, _) = generate_plane(1.0, 1.0);
        for v in &vertices {
            assert_eq!(v.normal, [0.0, 1.0, 0.0]);
        }
    }

    #[test]
    fn test_plane_uv_range() {
        let (vertices, _) = generate_plane(1.0, 1.0);
        for v in &vertices {
            assert!(v.tex_coord0[0] >= 0.0 && v.tex_coord0[0] <= 1.0);
            assert!(v.tex_coord0[1] >= 0.0 && v.tex_coord0[1] <= 1.0);
        }
    }
}
