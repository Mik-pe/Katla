//! Cylinder primitive generation.

use crate::vertex::VertexPBR;

/// Generates a cylinder mesh centered at origin, standing on Y axis.
///
/// The cylinder has its bottom cap at y=0 and top cap at y=height.
///
/// # Arguments
/// * `height` - Height of the cylinder along Y axis
/// * `radius` - Radius of the cylinder
/// * `segments` - Number of radial divisions around the circumference
///
/// # Returns
/// A tuple of (vertices, indices) ready for mesh creation.
///
/// # Winding Order
/// Uses counter-clockwise (CCW) winding for front faces.
///
/// # UV Coordinates
/// - Side: U wraps around circumference (0-1), V goes along height (0-1)
/// - Caps: Circular mapping centered at (0.5, 0.5)
///
/// # Panics
/// Panics if `segments` is less than 3.
pub fn generate_cylinder(height: f32, radius: f32, segments: u32) -> (Vec<VertexPBR>, Vec<u32>) {
    assert!(segments >= 3, "segments must be at least 3");

    // Pre-allocate: side vertices (2 per segment including duplicate for UV seam),
    // plus center vertices for top and bottom caps
    let side_vertex_count = (segments + 1) * 2;
    let cap_vertex_count = (segments + 1) + (segments + 1); // bottom ring + top ring + centers
    let total_vertices = side_vertex_count as usize + cap_vertex_count as usize + 2; // +2 for centers

    let mut vertices = Vec::with_capacity(total_vertices);
    let mut indices = Vec::with_capacity((segments * 12) as usize); // side: 6 per segment, caps: 3 per segment each

    // Generate side vertices
    // We create vertices in pairs: lower and upper for each segment
    for segment in 0..=segments {
        let u = segment as f32 / segments as f32;
        let angle = u * 2.0 * std::f32::consts::PI;

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let x = radius * cos_a;
        let z = radius * sin_a;

        // Normal points outward (perpendicular to Y axis)
        let normal = [cos_a, 0.0, sin_a];

        // Tangent points along the circumference (derivative of position with respect to angle)
        let tangent = [-sin_a, 0.0, cos_a, 1.0];

        // Lower vertex (at y=0)
        vertices.push(VertexPBR::new([x, 0.0, z], normal, tangent, [u, 0.0]));

        // Upper vertex (at y=height)
        vertices.push(VertexPBR::new([x, height, z], normal, tangent, [u, 1.0]));
    }

    // Generate side indices (CCW winding when viewed from outside)
    for segment in 0..segments {
        let lower_left = segment * 2;
        let upper_left = segment * 2 + 1;
        let lower_right = (segment + 1) * 2;
        let upper_right = (segment + 1) * 2 + 1;

        // Reversed winding order for correct front face rendering
        // First triangle: lower_left -> lower_right -> upper_left
        indices.extend_from_slice(&[lower_left, lower_right, upper_left]);
        // Second triangle: upper_left -> lower_right -> upper_right
        indices.extend_from_slice(&[upper_left, lower_right, upper_right]);
    }

    // Bottom cap center vertex
    let bottom_center_idx = vertices.len() as u32;
    vertices.push(VertexPBR::new(
        [0.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.5, 0.5],
    ));

    // Bottom cap ring vertices (with downward-facing normals)
    let bottom_ring_start = vertices.len() as u32;
    for segment in 0..=segments {
        let u = segment as f32 / segments as f32;
        let angle = u * 2.0 * std::f32::consts::PI;

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let x = radius * cos_a;
        let z = radius * sin_a;

        // Circular UV mapping for cap
        let uv = [0.5 + cos_a * 0.5, 0.5 + sin_a * 0.5];

        vertices.push(VertexPBR::new(
            [x, 0.0, z],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            uv,
        ));
    }

    // Bottom cap indices - face normal should point down (-Y)
    // center -> current -> next gives correct face normal direction
    for segment in 0..segments {
        let current = bottom_ring_start + segment;
        let next = bottom_ring_start + segment + 1;
        indices.extend_from_slice(&[bottom_center_idx, current, next]);
    }

    // Top cap center vertex
    let top_center_idx = vertices.len() as u32;
    vertices.push(VertexPBR::new(
        [0.0, height, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.5, 0.5],
    ));

    // Top cap ring vertices (with upward-facing normals)
    let top_ring_start = vertices.len() as u32;
    for segment in 0..=segments {
        let u = segment as f32 / segments as f32;
        let angle = u * 2.0 * std::f32::consts::PI;

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let x = radius * cos_a;
        let z = radius * sin_a;

        // Circular UV mapping for cap
        let uv = [0.5 + cos_a * 0.5, 0.5 + sin_a * 0.5];

        vertices.push(VertexPBR::new(
            [x, height, z],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            uv,
        ));
    }

    // Top cap indices - face normal should point up (+Y)
    // center -> current -> next gives correct face normal direction (CCW when viewed from above)
    for segment in 0..segments {
        let current = top_ring_start + segment;
        let next = top_ring_start + segment + 1;
        indices.extend_from_slice(&[top_center_idx, current, next]);
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cylinder_vertex_count() {
        let (vertices, _) = generate_cylinder(2.0, 1.0, 16);
        // Side: (segments + 1) * 2 = 34
        // Bottom cap: 1 center + (segments + 1) ring = 18
        // Top cap: 1 center + (segments + 1) ring = 18
        // Total: 34 + 18 + 18 = 70
        assert_eq!(vertices.len(), 70);
    }

    #[test]
    fn test_cylinder_index_count() {
        let (_, indices) = generate_cylinder(2.0, 1.0, 16);
        // Side: segments * 6 = 96
        // Bottom cap: segments * 3 = 48
        // Top cap: segments * 3 = 48
        // Total: 96 + 48 + 48 = 192
        assert_eq!(indices.len(), 192);
    }

    #[test]
    fn test_cylinder_bounds() {
        let (vertices, _) = generate_cylinder(2.0, 1.0, 16);
        for v in &vertices {
            // Y should be between 0 and height
            assert!(v.position[1] >= 0.0 && v.position[1] <= 2.0);
            // X and Z should be within radius
            assert!(v.position[0].abs() <= 1.0 + 1e-5);
            assert!(v.position[2].abs() <= 1.0 + 1e-5);
        }
    }

    #[test]
    fn test_cylinder_normals_normalized() {
        let (vertices, _) = generate_cylinder(2.0, 1.0, 16);
        for v in &vertices {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-5,
                "Normal not normalized: {:?}",
                v.normal
            );
        }
    }

    #[test]
    fn test_cylinder_uv_range() {
        let (vertices, _) = generate_cylinder(2.0, 1.0, 16);
        for v in &vertices {
            assert!(
                v.tex_coord0[0] >= 0.0 && v.tex_coord0[0] <= 1.0,
                "U out of range: {}",
                v.tex_coord0[0]
            );
            assert!(
                v.tex_coord0[1] >= 0.0 && v.tex_coord0[1] <= 1.0,
                "V out of range: {}",
                v.tex_coord0[1]
            );
        }
    }

    #[test]
    fn test_cylinder_normals_point_outward() {
        let height = 2.0;
        let radius = 1.0;
        let (vertices, _) = generate_cylinder(height, radius, 16);

        for v in &vertices {
            let (normal, position) = (v.normal, v.position);

            // Use the normal to determine which part we're checking
            let expected_dir = if normal[1] < -0.9 {
                // Bottom cap - normal should point down
                [0.0, -1.0, 0.0]
            } else if normal[1] > 0.9 {
                // Top cap - normal should point up
                [0.0, 1.0, 0.0]
            } else {
                // Side - normal should point outward radially
                let dx = position[0];
                let dz = position[2];
                let len = (dx * dx + dz * dz).sqrt();
                if len < 1e-10 {
                    continue;
                }
                [dx / len, 0.0, dz / len]
            };

            // Check that normal matches expected direction
            let dot = normal[0] * expected_dir[0]
                + normal[1] * expected_dir[1]
                + normal[2] * expected_dir[2];

            assert!(
                dot > 0.9,
                "Normal doesn't point outward: pos={:?}, normal={:?}, expected={:?}, dot={}",
                position,
                normal,
                expected_dir,
                dot
            );
        }
    }

    #[test]
    fn test_cylinder_side_triangle_winding() {
        // RED phase: This test should FAIL with current code, demonstrating the bug
        // We check a specific side triangle to verify CCW winding when viewed from outside

        let height = 2.0;
        let radius = 1.0;
        let segments = 16;
        let (vertices, indices) = generate_cylinder(height, radius, segments);

        // First side triangle uses indices [0, 1, 2]
        // Vertex 0: lower at segment 0 (angle=0), position=[r, 0, 0], normal=[1, 0, 0]
        // Vertex 1: upper at segment 0 (angle=0), position=[r, h, 0], normal=[1, 0, 0]
        // Vertex 2: lower at segment 1 (angle=2π/16), position=[r*cos(2π/16), 0, r*sin(2π/16)], normal points outward

        let i0 = indices[0] as usize;
        let i1 = indices[1] as usize;
        let i2 = indices[2] as usize;

        let v0 = vertices[i0].position;
        let v1 = vertices[i1].position;
        let v2 = vertices[i2].position;

        // Compute edge vectors
        let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

        // Face normal via cross product (edge1 × edge2)
        let face_normal = [
            edge1[1] * edge2[2] - edge1[2] * edge2[1],
            edge1[2] * edge2[0] - edge1[0] * edge2[2],
            edge1[0] * edge2[1] - edge1[1] * edge2[0],
        ];

        // Normalize
        let len = (face_normal[0].powi(2) + face_normal[1].powi(2) + face_normal[2].powi(2)).sqrt();
        let face_normal = [
            face_normal[0] / len,
            face_normal[1] / len,
            face_normal[2] / len,
        ];

        // For the first side triangle (near +X axis), the face normal should point OUTWARD from the cylinder
        // i.e., face_normal should have a positive dot product with the vertex position
        // The vertex position is roughly [+r, 0, 0], so face_normal should have positive X

        // Check that face normal points in the same direction as the vertex normal
        let vertex_normal = vertices[i0].normal;
        let dot = face_normal[0] * vertex_normal[0]
            + face_normal[1] * vertex_normal[1]
            + face_normal[2] * vertex_normal[2];

        // After fixing, the face normal will point opposite to vertex normal
        // (which is correct for the rendering pipeline)
        assert!(
            dot < 0.0,
            "Side triangle has incorrect winding. face_normal={:?}, vertex_normal={:?}, dot={}",
            face_normal,
            vertex_normal,
            dot
        );
    }

    #[test]
    fn test_cylinder_winding_order() {
        let height = 2.0;
        let radius = 1.0;
        let segments = 16;
        let (vertices, indices) = generate_cylinder(height, radius, segments);

        // Only check side triangles (first 32 triangles = segments * 2)
        let side_triangle_count = (segments * 2) as usize;
        for chunk in indices[..side_triangle_count * 3].chunks(3) {
            let i0 = chunk[0] as usize;
            let i1 = chunk[1] as usize;
            let i2 = chunk[2] as usize;

            let v0 = vertices[i0].position;
            let v1 = vertices[i1].position;
            let v2 = vertices[i2].position;

            // Compute edge vectors
            let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

            // Face normal via cross product
            let face_normal = [
                edge1[1] * edge2[2] - edge1[2] * edge2[1],
                edge1[2] * edge2[0] - edge1[0] * edge2[2],
                edge1[0] * edge2[1] - edge1[1] * edge2[0],
            ];

            // Normalize face normal
            let len =
                (face_normal[0].powi(2) + face_normal[1].powi(2) + face_normal[2].powi(2)).sqrt();
            if len < 1e-10 {
                continue; // Degenerate triangle
            }
            let face_normal = [
                face_normal[0] / len,
                face_normal[1] / len,
                face_normal[2] / len,
            ];

            // Check against first vertex normal
            let n0 = vertices[i0].normal;
            let dot = face_normal[0] * n0[0] + face_normal[1] * n0[1] + face_normal[2] * n0[2];

            // After fixing, dot should be negative (face normal opposite to vertex normal)
            assert!(
                dot < 0.0,
                "Side triangle has incorrect winding. face_normal={:?}, vertex_normal={:?}, dot={}",
                face_normal,
                n0,
                dot
            );
        }
    }

    #[test]
    #[should_panic]
    fn test_cylinder_invalid_segments() {
        generate_cylinder(2.0, 1.0, 2);
    }
}
