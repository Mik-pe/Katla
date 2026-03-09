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

        // Two triangles in CCW order (0,1,2) and (0,2,3) for proper outward normals
        indices.extend_from_slice(&[lower_left, upper_left, upper_right]);
        indices.extend_from_slice(&[lower_left, upper_right, lower_right]);
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
    // current -> next -> center gives CCW winding when viewed from outside
    for segment in 0..segments {
        let current = bottom_ring_start + segment;
        let next = bottom_ring_start + segment + 1;
        indices.extend_from_slice(&[current, next, bottom_center_idx]);
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
    // current -> center -> next gives CCW winding when viewed from outside
    for segment in 0..segments {
        let current = top_ring_start + segment;
        let next = top_ring_start + segment + 1;
        indices.extend_from_slice(&[current, top_center_idx, next]);
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
    fn test_cylinder_triangle_winding() {
        let height = 2.0;
        let radius = 1.0;
        let segments = 16;
        let (vertices, indices) = generate_cylinder(height, radius, segments);

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
            if len < 1e-10 {
                continue; // Skip degenerate triangles (cap centers)
            }
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
                dot > 0.95,
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
    #[should_panic]
    fn test_cylinder_invalid_segments() {
        generate_cylinder(2.0, 1.0, 2);
    }
}
