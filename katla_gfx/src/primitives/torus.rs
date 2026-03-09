//! Torus primitive generation.

use crate::vertex::VertexPBR;

/// Generates a torus (donut) centered at the origin, lying on the XZ plane.
///
/// The torus is created by revolving a circle (minor radius) around the Y axis
/// at a distance (major radius) from the center.
///
/// # Arguments
/// * `major_radius` - Distance from center of torus to center of tube
/// * `minor_radius` - Radius of the tube
/// * `segments` - Number of divisions around the torus (major circle)
/// * `rings` - Number of divisions around the tube (minor circle)
///
/// # Returns
/// A tuple of (vertices, indices) ready for mesh creation.
///
/// # Winding Order
/// Uses counter-clockwise (CCW) winding for front faces.
///
/// # Panics
/// Panics if `segments` or `rings` is less than 3.
pub fn generate_torus(
    major_radius: f32,
    minor_radius: f32,
    segments: u32,
    rings: u32,
) -> (Vec<VertexPBR>, Vec<u32>) {
    assert!(segments >= 3, "segments must be at least 3");
    assert!(rings >= 3, "rings must be at least 3");

    let mut vertices = Vec::with_capacity(((rings + 1) * (segments + 1)) as usize);
    let mut indices = Vec::with_capacity((rings * segments * 6) as usize);

    // Generate vertices
    // u: angle around the torus (major circle) - 0 to 2π
    // v: angle around the tube (minor circle) - 0 to 2π
    for segment in 0..=segments {
        let u = 2.0 * std::f32::consts::PI * segment as f32 / segments as f32;
        let cos_u = u.cos();
        let sin_u = u.sin();

        for ring in 0..=rings {
            let v = 2.0 * std::f32::consts::PI * ring as f32 / rings as f32;
            let cos_v = v.cos();
            let sin_v = v.sin();

            // Position on torus surface
            // x = (R + r*cos(v)) * cos(u)
            // y = r * sin(v)
            // z = (R + r*cos(v)) * sin(u)
            let r_cos_v = minor_radius * cos_v;
            let r_sin_v = minor_radius * sin_v;
            let x = (major_radius + r_cos_v) * cos_u;
            let y = r_sin_v;
            let z = (major_radius + r_cos_v) * sin_u;

            // Normal points outward from tube center
            // nx = cos(v) * cos(u)
            // ny = sin(v)
            // nz = cos(v) * sin(u)
            let nx = cos_v * cos_u;
            let ny = sin_v;
            let nz = cos_v * sin_u;

            // Tangent: derivative with respect to u (around the torus)
            // tx = -sin(u)
            // ty = 0
            // tz = cos(u)
            let tx = -sin_u;
            let ty = 0.0;
            let tz = cos_u;

            // UV coordinates
            let u_coord = segment as f32 / segments as f32;
            let v_coord = ring as f32 / rings as f32;

            vertices.push(VertexPBR::new(
                [x, y, z],
                [nx, ny, nz],
                [tx, ty, tz, 1.0],
                [u_coord, v_coord],
            ));
        }
    }

    // Generate indices for triangles
    for segment in 0..segments {
        for ring in 0..rings {
            let current = segment * (rings + 1) + ring;
            let next = current + rings + 1;

            // Two triangles per quad (0,1,2) and (0,2,3) for proper CCW winding
            let v0 = current;
            let v1 = current + 1;
            let v2 = next + 1;
            let v3 = next;

            indices.extend_from_slice(&[v0, v1, v2]);
            indices.extend_from_slice(&[v0, v2, v3]);
        }
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_torus_vertex_count() {
        // (segments + 1) * (rings + 1)
        let (vertices, _) = generate_torus(1.0, 0.3, 16, 16);
        assert_eq!(vertices.len(), 17 * 17);
    }

    #[test]
    fn test_torus_index_count() {
        // segments * rings * 6 (2 triangles * 3 indices per quad)
        let (_, indices) = generate_torus(1.0, 0.3, 16, 16);
        assert_eq!(indices.len(), 16 * 16 * 6);
    }

    #[test]
    fn test_torus_normals_normalized() {
        let (vertices, _) = generate_torus(1.0, 0.3, 16, 16);
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
    fn test_torus_tangents_normalized() {
        let (vertices, _) = generate_torus(1.0, 0.3, 16, 16);
        for v in &vertices {
            let len = (v.tangent[0].powi(2) + v.tangent[1].powi(2) + v.tangent[2].powi(2)).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-5,
                "Tangent not normalized: {:?}",
                v.tangent
            );
        }
    }

    #[test]
    fn test_torus_uv_range() {
        let (vertices, _) = generate_torus(1.0, 0.3, 8, 8);
        for v in &vertices {
            assert!(v.tex_coord0[0] >= 0.0 && v.tex_coord0[0] <= 1.0);
            assert!(v.tex_coord0[1] >= 0.0 && v.tex_coord0[1] <= 1.0);
        }
    }

    #[test]
    fn test_torus_distance_from_center() {
        let major_radius = 1.0;
        let minor_radius = 0.3;
        let (vertices, _) = generate_torus(major_radius, minor_radius, 16, 16);

        for v in &vertices {
            // Distance from Y axis (in XZ plane)
            let dist_xz = (v.position[0].powi(2) + v.position[2].powi(2)).sqrt();
            // This distance should be between (major - minor) and (major + minor)
            assert!(dist_xz >= major_radius - minor_radius - 1e-5);
            assert!(dist_xz <= major_radius + minor_radius + 1e-5);

            // Y should be between -minor and +minor
            assert!(v.position[1].abs() <= minor_radius + 1e-5);
        }
    }

    #[test]
    #[should_panic]
    fn test_torus_invalid_segments() {
        generate_torus(1.0, 0.3, 2, 8);
    }

    #[test]
    #[should_panic]
    fn test_torus_invalid_rings() {
        generate_torus(1.0, 0.3, 8, 2);
    }

    #[test]
    fn test_torus_triangle_winding() {
        let major_radius = 1.0;
        let minor_radius = 0.3;
        let segments = 16;
        let rings = 16;
        let (vertices, indices) = generate_torus(major_radius, minor_radius, segments, rings);

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
                continue; // Skip degenerate triangles
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
}
