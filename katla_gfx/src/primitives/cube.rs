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

    // Two triangles in CCW order (0,1,2) and (0,2,3)
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

    #[test]
    fn test_cube_normals_point_outward() {
        let (vertices, _) = generate_cube([1.0, 1.0, 1.0]);

        // For each face, verify the normal points outward by checking
        // that normal dot view_direction is positive when viewed from outside
        let test_cases = [
            (
                [0.0, 0.0, 1.0],
                [
                    [-0.5, -0.5, 0.5],
                    [0.5, -0.5, 0.5],
                    [0.5, 0.5, 0.5],
                    [-0.5, 0.5, 0.5],
                ],
            ), // Front (+Z)
            (
                [0.0, 0.0, -1.0],
                [
                    [0.5, -0.5, -0.5],
                    [-0.5, -0.5, -0.5],
                    [-0.5, 0.5, -0.5],
                    [0.5, 0.5, -0.5],
                ],
            ), // Back (-Z)
            (
                [0.0, 1.0, 0.0],
                [
                    [-0.5, 0.5, 0.5],
                    [0.5, 0.5, 0.5],
                    [0.5, 0.5, -0.5],
                    [-0.5, 0.5, -0.5],
                ],
            ), // Top (+Y)
            (
                [0.0, -1.0, 0.0],
                [
                    [-0.5, -0.5, -0.5],
                    [0.5, -0.5, -0.5],
                    [0.5, -0.5, 0.5],
                    [-0.5, -0.5, 0.5],
                ],
            ), // Bottom (-Y)
            (
                [1.0, 0.0, 0.0],
                [
                    [0.5, -0.5, 0.5],
                    [0.5, -0.5, -0.5],
                    [0.5, 0.5, -0.5],
                    [0.5, 0.5, 0.5],
                ],
            ), // Right (+X)
            (
                [-1.0, 0.0, 0.0],
                [
                    [-0.5, -0.5, -0.5],
                    [-0.5, -0.5, 0.5],
                    [-0.5, 0.5, 0.5],
                    [-0.5, 0.5, -0.5],
                ],
            ), // Left (-X)
        ];

        let mut vertex_idx = 0;
        for (view_dir, corners) in test_cases {
            for _corner in corners {
                let v = &vertices[vertex_idx];
                let normal_dot_view = v.normal[0] * view_dir[0]
                    + v.normal[1] * view_dir[1]
                    + v.normal[2] * view_dir[2];

                // Normal should point in same direction as view (positive dot product)
                assert!(
                    normal_dot_view > 0.5,
                    "Normal {:?} at position {:?} doesn't point outward for face {:?}",
                    v.normal,
                    v.position,
                    view_dir
                );
                vertex_idx += 1;
            }
        }
    }

    #[test]
    fn test_cube_triangle_winding() {
        let (vertices, indices) = generate_cube([1.0, 1.0, 1.0]);

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
            let geo_normal = if len > 1e-10 {
                [
                    geo_normal[0] / len,
                    geo_normal[1] / len,
                    geo_normal[2] / len,
                ]
            } else {
                geo_normal
            };

            // Use v0's stored normal (all vertices on a face have the same normal)
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
}
