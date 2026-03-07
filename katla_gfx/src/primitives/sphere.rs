//! UV Sphere primitive generation.

use crate::vertex::VertexPBR;

/// Generates a UV sphere centered at the origin.
///
/// The sphere is generated with horizontal rings (latitude) and vertical segments (longitude).
/// Poles are at +Y and -Y.
///
/// # Arguments
/// * `radius` - Radius of the sphere
/// * `segments` - Number of vertical divisions (longitude lines)
/// * `rings` - Number of horizontal divisions (latitude lines)
///
/// # Returns
/// A tuple of (vertices, indices) ready for mesh creation.
///
/// # Winding Order
/// Uses counter-clockwise (CCW) winding for front faces.
///
/// # Panics
/// Panics if `segments` or `rings` is less than 3.
pub fn generate_sphere(radius: f32, segments: u32, rings: u32) -> (Vec<VertexPBR>, Vec<u32>) {
    assert!(segments >= 3, "segments must be at least 3");
    assert!(rings >= 3, "rings must be at least 3");

    let mut vertices = Vec::with_capacity(((rings - 1) * (segments + 1) + 2) as usize);
    let mut indices = Vec::with_capacity((rings * segments * 6) as usize);

    // Generate vertices for middle rings (not poles)
    // Start from ring 1, skip ring 0 (top pole) - we'll add a single vertex for it
    for ring in 1..rings {
        let theta = std::f32::consts::PI * ring as f32 / rings as f32;
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();

        for segment in 0..=segments {
            let phi = 2.0 * std::f32::consts::PI * segment as f32 / segments as f32;
            let sin_phi = phi.sin();
            let cos_phi = phi.cos();

            // Position on unit sphere, scaled by radius
            let x = cos_phi * sin_theta;
            let y = cos_theta;
            let z = sin_phi * sin_theta;

            // Normal is same as position for unit sphere
            let normal = [x, y, z];

            // Position scaled by radius
            let position = [x * radius, y * radius, z * radius];

            // Tangent: derivative with respect to phi (longitude)
            // For a sphere: tangent points along the longitude line
            let tx = -sin_phi * sin_theta;
            let ty = 0.0;
            let tz = cos_phi * sin_theta;
            let tangent = [tx, ty, tz, 1.0];

            // UV coordinates
            let u = segment as f32 / segments as f32;
            let v = ring as f32 / rings as f32;

            vertices.push(VertexPBR::new(position, normal, tangent, [u, v]));
        }
    }

    // Add top pole vertex (after all middle ring vertices)
    let top_pole_idx = vertices.len() as u32;
    vertices.push(VertexPBR::new(
        [0.0, radius, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.5, 0.0],
    ));

    // Add bottom pole vertex
    let bottom_pole_idx = vertices.len() as u32;
    vertices.push(VertexPBR::new(
        [0.0, -radius, 0.0],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.5, 1.0],
    ));

    // Generate indices for side triangles (between middle rings)
    // Each ring has (segments + 1) vertices, starting from index 0
    // We have (rings - 1) middle rings, and we connect adjacent rings
    for ring in 0..(rings - 2) {
        for segment in 0..segments {
            let ring_base = (ring * (segments + 1)) as u32;
            let next_ring_base = ((ring + 1) * (segments + 1)) as u32;

            let current = ring_base + segment as u32;
            let next = next_ring_base + segment as u32;

            // Two triangles per quad
            let v0 = current;
            let v1 = current + 1;
            let v2 = next + 1;
            let v3 = next;

            indices.extend_from_slice(&[v0, v2, v1]);
            indices.extend_from_slice(&[v0, v3, v2]);
        }
    }

    // Top cap: connect top pole to first ring
    // For face normal to point opposite to vertex normal (inward, not outward),
    // Order: [pole, current, next]
    for segment in 0..segments {
        let v0 = top_pole_idx; // pole vertex
        let v1 = segment; // current vertex in ring
        let v2 = segment + 1; // next vertex in ring

        indices.extend_from_slice(&[v0, v1, v2]);
    }

    // Bottom cap: connect last ring to bottom pole
    // For face normal to point opposite to vertex normal (inward, not outward),
    // Order: [pole, next, current]
    let last_ring_base = ((rings - 2) * (segments + 1)) as u32;
    for segment in 0..segments {
        let v0 = bottom_pole_idx; // bottom pole vertex
        let v1 = last_ring_base + segment as u32 + 1; // next vertex in ring
        let v2 = last_ring_base + segment as u32; // current vertex in ring

        indices.extend_from_slice(&[v0, v1, v2]);
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sphere_vertex_count() {
        // 2 poles + (rings - 1) * (segments + 1) middle rings
        let (vertices, _) = generate_sphere(1.0, 16, 16);
        assert_eq!(vertices.len(), 2 + 15 * 17);
    }

    #[test]
    fn test_sphere_index_count() {
        // Top cap: segments * 3, Bottom cap: segments * 3, Side: (rings - 2) * segments * 6
        let (_, indices) = generate_sphere(1.0, 16, 16);
        assert_eq!(indices.len(), 16 * 3 + 16 * 3 + 14 * 16 * 6);
    }

    #[test]
    fn test_sphere_radius() {
        let (vertices, _) = generate_sphere(2.0, 8, 8);
        for v in &vertices {
            let dist =
                (v.position[0].powi(2) + v.position[1].powi(2) + v.position[2].powi(2)).sqrt();
            assert!((dist - 2.0).abs() < 1e-5);
        }
    }

    #[test]
    fn test_sphere_normals_normalized() {
        let (vertices, _) = generate_sphere(1.0, 8, 8);
        for v in &vertices {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((len - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn test_sphere_uv_range() {
        let (vertices, _) = generate_sphere(1.0, 8, 8);
        for v in &vertices {
            assert!(v.tex_coord0[0] >= 0.0 && v.tex_coord0[0] <= 1.0);
            assert!(v.tex_coord0[1] >= 0.0 && v.tex_coord0[1] <= 1.0);
        }
    }

    #[test]
    #[should_panic]
    fn test_sphere_invalid_segments() {
        generate_sphere(1.0, 2, 8);
    }

    #[test]
    #[should_panic]
    fn test_sphere_invalid_rings() {
        generate_sphere(1.0, 8, 2);
    }

    #[test]
    fn test_sphere_normals_point_outward() {
        let radius = 1.0;
        let (vertices, _) = generate_sphere(radius, 16, 16);

        for v in &vertices {
            // For a sphere centered at origin, normals should equal normalized position
            let pos_len =
                (v.position[0].powi(2) + v.position[1].powi(2) + v.position[2].powi(2)).sqrt();
            let normal_len =
                (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();

            assert!(pos_len > 1e-10, "Position at origin");
            assert!(normal_len > 1e-10, "Zero normal");

            // Normalize position
            let norm_pos = [
                v.position[0] / pos_len,
                v.position[1] / pos_len,
                v.position[2] / pos_len,
            ];

            // Normalize normal
            let norm_normal = [
                v.normal[0] / normal_len,
                v.normal[1] / normal_len,
                v.normal[2] / normal_len,
            ];

            // They should match (normals point outward from center)
            let dot = norm_pos[0] * norm_normal[0]
                + norm_pos[1] * norm_normal[1]
                + norm_pos[2] * norm_normal[2];

            assert!(
                dot > 0.99,
                "Normal doesn't point outward: pos={:?}, normal={:?}, dot={}",
                v.position,
                v.normal,
                dot
            );
        }
    }

    #[test]
    fn test_sphere_winding_order() {
        let radius = 1.0;
        let segments = 16;
        let rings = 16;
        let (vertices, indices) = generate_sphere(radius, segments, rings);

        let mut failed = 0;
        let mut passed = 0;

        for chunk in indices.chunks(3) {
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

            // Average vertex normal
            let n0 = vertices[i0].normal;
            let n1 = vertices[i1].normal;
            let n2 = vertices[i2].normal;
            let avg = [
                n0[0] + n1[0] + n2[0],
                n0[1] + n1[1] + n2[1],
                n0[2] + n1[2] + n2[2],
            ];
            let avg_len = (avg[0].powi(2) + avg[1].powi(2) + avg[2].powi(2)).sqrt();
            if avg_len < 1e-10 {
                continue;
            }
            let avg_normal = [avg[0] / avg_len, avg[1] / avg_len, avg[2] / avg_len];

            // For correct winding, face_normal should point opposite to vertex normal
            let dot = face_normal[0] * avg_normal[0]
                + face_normal[1] * avg_normal[1]
                + face_normal[2] * avg_normal[2];

            if dot > 0.0 {
                // CHANGED: positive dot means wrong winding now
                failed += 1;
            } else {
                passed += 1;
            }
        }

        assert_eq!(
            failed, 0,
            "Sphere has {} triangles with incorrect winding ({} passed)",
            failed, passed
        );
    }
}
