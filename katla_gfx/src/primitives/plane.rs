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
    // Tangent along +X, bitangent along +Z (handedness = +1 so that
    // cross(N, T) = (0, 0, -1) and B = (0, 0, -1)... wait.
    //
    // Actually: B = cross(N, T) * handedness.
    // cross((0,1,0), (1,0,0)) = (0, 0, -1).
    // For handedness = -1: B = (0, 0, 1) which matches V direction (+Z).
    let vertices = vec![
        // Bottom-left
        VertexPBR::new(
            [-hw, 0.0, -hh],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, -1.0],
            [0.0, 0.0],
        ),
        // Bottom-right
        VertexPBR::new(
            [hw, 0.0, -hh],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, -1.0],
            [1.0, 0.0],
        ),
        // Top-right
        VertexPBR::new(
            [hw, 0.0, hh],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, -1.0],
            [1.0, 1.0],
        ),
        // Top-left
        VertexPBR::new(
            [-hw, 0.0, hh],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, -1.0],
            [0.0, 1.0],
        ),
    ];

    // Two triangles (CCW winding when viewed from above)
    let indices = vec![
        0, 2, 1, // First triangle
        0, 3, 2, // Second triangle
    ];

    (vertices, indices)
}

/// Generates a tessellated plane on the XY plane centered at the origin.
///
/// The plane extends from -width/2 to +width/2 on X and -height/2 to +height/2 on Y.
/// Normal points in +Z direction (facing the camera in a standard setup).
///
/// # Arguments
/// * `width` - Width along the X axis
/// * `height` - Height along the Y axis
/// * `segments` - Number of subdivisions in each direction (minimum 1)
///
/// # Returns
/// A tuple of (vertices, indices) ready for mesh creation.
///
/// # Winding Order
/// Uses counter-clockwise (CCW) winding for front faces (viewed from +Z).
pub fn generate_plane_xy(width: f32, height: f32, segments: u32) -> (Vec<VertexPBR>, Vec<u32>) {
    let segments = segments.max(1);
    let half_width = width * 0.5;
    let half_height = height * 0.5;

    let mut vertices = Vec::with_capacity(((segments + 1) * (segments + 1)) as usize);

    // Create a grid of vertices
    for row in 0..=segments {
        let v = row as f32 / segments as f32;
        let y = v * height - half_height;

        for col in 0..=segments {
            let u = col as f32 / segments as f32;
            let x = u * width - half_width;

            vertices.push(VertexPBR::new(
                [x, y, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
                [u, v],
            ));
        }
    }

    let mut indices = Vec::with_capacity((segments * segments * 6) as usize);

    // Generate indices for each quad (2 triangles per quad)
    for row in 0..segments {
        for col in 0..segments {
            let current = row * (segments + 1) + col;
            let next_row = (row + 1) * (segments + 1) + col;

            // First triangle (CCW when viewed from +Z)
            indices.push(current);
            indices.push(current + 1);
            indices.push(next_row);

            // Second triangle (CCW when viewed from +Z)
            indices.push(current + 1);
            indices.push(next_row + 1);
            indices.push(next_row);
        }
    }

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

    #[test]
    fn test_plane_triangle_winding() {
        let (vertices, indices) = generate_plane(1.0, 1.0);

        // Check each triangle's winding by computing the geometric normal
        // from CCW vertex order and comparing with the stored normal
        for tri in indices.chunks(3) {
            let v0 = &vertices[tri[0] as usize];
            let v1 = &vertices[tri[1] as usize];
            let v2 = &vertices[tri[2] as usize];

            // Compute edge vectors
            let e1 = [
                v1.position[0] - v0.position[0],
                v1.position[1] - v0.position[1],
                v1.position[2] - v0.position[2],
            ];
            let e2 = [
                v2.position[0] - v0.position[0],
                v2.position[1] - v0.position[1],
                v2.position[2] - v0.position[2],
            ];

            // Cross product gives geometric normal (should match stored normal for CCW)
            let geo_normal = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];

            // Normalize
            let len =
                (geo_normal[0].powi(2) + geo_normal[1].powi(2) + geo_normal[2].powi(2)).sqrt();
            let geo_normal = [
                geo_normal[0] / len,
                geo_normal[1] / len,
                geo_normal[2] / len,
            ];

            // Use v0's stored normal
            let stored_normal = v0.normal;

            // Dot product should be close to 1.0 (normals point same direction)
            let dot = stored_normal[0] * geo_normal[0]
                + stored_normal[1] * geo_normal[1]
                + stored_normal[2] * geo_normal[2];

            assert!(
                dot > 0.99,
                "Triangle winding error: geometric normal {:?} doesn't match stored normal {:?} (dot={}) \
                for triangle with vertices: {:?}, {:?}, {:?}",
                geo_normal,
                stored_normal,
                dot,
                v0.position,
                v1.position,
                v2.position
            );
        }
    }

    // === XY Plane Tests ===

    #[test]
    fn test_plane_xy_vertex_count() {
        let (vertices, _) = generate_plane_xy(1.0, 1.0, 1);
        assert_eq!(vertices.len(), 4); // 2x2 grid

        let (vertices, _) = generate_plane_xy(1.0, 1.0, 4);
        assert_eq!(vertices.len(), 25); // 5x5 grid
    }

    #[test]
    fn test_plane_xy_index_count() {
        let (_, indices) = generate_plane_xy(1.0, 1.0, 1);
        assert_eq!(indices.len(), 6); // 1 quad * 2 triangles * 3 indices

        let (_, indices) = generate_plane_xy(1.0, 1.0, 4);
        assert_eq!(indices.len(), 96); // 16 quads * 2 triangles * 3 indices
    }

    #[test]
    fn test_plane_xy_bounds() {
        let (vertices, _) = generate_plane_xy(2.0, 4.0, 3);
        for v in &vertices {
            assert!(v.position[0].abs() <= 1.0); // width/2
            assert!(v.position[1].abs() <= 2.0); // height/2
            assert!(v.position[2].abs() < 1e-6); // z = 0
        }
    }

    #[test]
    fn test_plane_xy_normals_forward() {
        let (vertices, _) = generate_plane_xy(1.0, 1.0, 4);
        for v in &vertices {
            assert_eq!(v.normal, [0.0, 0.0, 1.0]);
        }
    }

    #[test]
    fn test_plane_xy_tangent() {
        let (vertices, _) = generate_plane_xy(1.0, 1.0, 4);
        for v in &vertices {
            assert_eq!(v.tangent, [1.0, 0.0, 0.0, 1.0]);
        }
    }

    #[test]
    fn test_plane_xy_uv_range() {
        let (vertices, _) = generate_plane_xy(1.0, 1.0, 4);
        for v in &vertices {
            assert!(v.tex_coord0[0] >= 0.0 && v.tex_coord0[0] <= 1.0);
            assert!(v.tex_coord0[1] >= 0.0 && v.tex_coord0[1] <= 1.0);
        }
    }

    #[test]
    fn test_plane_xy_triangle_winding() {
        let (vertices, indices) = generate_plane_xy(2.0, 2.0, 2);

        // Check each triangle's winding by computing the geometric normal
        // from CCW vertex order and comparing with the stored normal
        for tri in indices.chunks(3) {
            let v0 = &vertices[tri[0] as usize];
            let v1 = &vertices[tri[1] as usize];
            let v2 = &vertices[tri[2] as usize];

            // Compute edge vectors
            let e1 = [
                v1.position[0] - v0.position[0],
                v1.position[1] - v0.position[1],
                v1.position[2] - v0.position[2],
            ];
            let e2 = [
                v2.position[0] - v0.position[0],
                v2.position[1] - v0.position[1],
                v2.position[2] - v0.position[2],
            ];

            // Cross product gives geometric normal (should match stored normal for CCW)
            let geo_normal = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];

            // Normalize
            let len =
                (geo_normal[0].powi(2) + geo_normal[1].powi(2) + geo_normal[2].powi(2)).sqrt();
            let geo_normal = [
                geo_normal[0] / len,
                geo_normal[1] / len,
                geo_normal[2] / len,
            ];

            // Use v0's stored normal
            let stored_normal = v0.normal;

            // Dot product should be close to 1.0 (normals point same direction)
            let dot = stored_normal[0] * geo_normal[0]
                + stored_normal[1] * geo_normal[1]
                + stored_normal[2] * geo_normal[2];

            assert!(
                dot > 0.99,
                "Triangle winding error: geometric normal {:?} doesn't match stored normal {:?} (dot={}) \
                for triangle with vertices: {:?}, {:?}, {:?}",
                geo_normal,
                stored_normal,
                dot,
                v0.position,
                v1.position,
                v2.position
            );
        }
    }

    #[test]
    fn test_plane_xy_segments_minimum() {
        // segments=0 should be treated as segments=1
        let (vertices, indices) = generate_plane_xy(1.0, 1.0, 0);
        assert_eq!(vertices.len(), 4);
        assert_eq!(indices.len(), 6);
    }
}
