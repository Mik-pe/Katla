//! UV Sphere primitive generation.

use crate::vertex::VertexPBR;

/// Generates a UV sphere centered at the origin.
///
/// The sphere is generated with horizontal rings (latitude) and vertical segments (longitude).
/// Poles are at +Y and -Y.
///
/// To avoid normal pinching at the poles, each pole uses (segments + 1) separate vertices
/// (one per segment), each with a normal pointing toward its segment direction.
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

    // Capacity: middle rings + top pole vertices + bottom pole vertices
    let middle_ring_count = (rings - 1) * (segments + 1);
    let pole_vertex_count = (segments + 1) * 2;
    let mut vertices = Vec::with_capacity((middle_ring_count + pole_vertex_count) as usize);
    let mut indices = Vec::with_capacity((rings * segments * 6) as usize);

    // Generate vertices for middle rings (not poles)
    // Start from ring 1, skip ring 0 (top pole area)
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

    // Generate top pole vertices (one per segment for UV continuity)
    // Normal is the normalized position
    let top_pole_start = vertices.len() as u32;

    for segment in 0..=segments {
        let phi = 2.0 * std::f32::consts::PI * segment as f32 / segments as f32;

        // Position at pole top
        let position = [0.0, radius, 0.0];

        // Normal = normalize(position)
        let len =
            (position[0] * position[0] + position[1] * position[1] + position[2] * position[2])
                .sqrt();
        let normal = [position[0] / len, position[1] / len, position[2] / len];

        // Tangent points along the longitude direction
        let tangent = [-phi.sin(), 0.0, phi.cos(), 1.0];

        // UV coordinates
        let u = segment as f32 / segments as f32;

        vertices.push(VertexPBR::new(position, normal, tangent, [u, 0.0]));
    }

    // Generate bottom pole vertices (one per segment for UV continuity)
    // Normal is the normalized position
    let bottom_pole_start = vertices.len() as u32;

    for segment in 0..=segments {
        let phi = 2.0 * std::f32::consts::PI * segment as f32 / segments as f32;

        // Position at pole bottom
        let position = [0.0, -radius, 0.0];

        // Normal = normalize(position)
        let len =
            (position[0] * position[0] + position[1] * position[1] + position[2] * position[2])
                .sqrt();
        let normal = [position[0] / len, position[1] / len, position[2] / len];

        // Tangent points along the longitude direction
        let tangent = [-phi.sin(), 0.0, phi.cos(), 1.0];

        // UV coordinates
        let u = segment as f32 / segments as f32;

        vertices.push(VertexPBR::new(position, normal, tangent, [u, 1.0]));
    }

    // Generate indices for side triangles (between middle rings)
    // Each ring has (segments + 1) vertices, starting from index 0
    // We have (rings - 1) middle rings, and we connect adjacent rings
    for ring in 0..(rings - 2) {
        for segment in 0..segments {
            let ring_base = (ring * (segments + 1)) as u32;
            let next_ring_base = ((ring + 1) * (segments + 1)) as u32;

            let current = ring_base + segment as u32;
            let next = next_ring_base + segment as u32;

            // Two triangles per quad (0,1,2) and (0,2,3)
            let v0 = current;
            let v1 = current + 1;
            let v2 = next + 1;
            let v3 = next;

            indices.extend_from_slice(&[v0, v1, v2]);
            indices.extend_from_slice(&[v0, v2, v3]);
        }
    }

    // Top cap: connect top pole vertices to first ring
    // Each triangle uses the pole vertex whose normal matches the ring segment
    for segment in 0..segments {
        let pole_vertex = top_pole_start + segment + 1;
        let ring_vertex = segment;
        let next_ring_vertex = segment + 1;

        indices.extend_from_slice(&[ring_vertex, pole_vertex, next_ring_vertex]);
    }

    // Bottom cap: connect last ring to bottom pole vertices
    // Each pole vertex connects to its corresponding ring vertex
    let last_ring_base = ((rings - 2) * (segments + 1)) as u32;
    for segment in 0..segments {
        let ring_vertex = last_ring_base + segment;
        let next_ring_vertex = last_ring_base + segment + 1;
        let pole_vertex = bottom_pole_start + segment;

        indices.extend_from_slice(&[next_ring_vertex, pole_vertex, ring_vertex]);
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sphere_vertex_count() {
        // Middle rings: (rings - 1) * (segments + 1) = 15 * 17 = 255
        // Top pole: segments + 1 = 17
        // Bottom pole: segments + 1 = 17
        // Total: 255 + 17 + 17 = 289
        let (vertices, _) = generate_sphere(1.0, 16, 16);
        assert_eq!(vertices.len(), 289);
    }

    #[test]
    fn test_sphere_index_count() {
        // Side triangles: (rings - 2) * segments * 6 = 14 * 16 * 6 = 1344
        // Top cap: segments * 3 = 16 * 3 = 48
        // Bottom cap: segments * 3 = 16 * 3 = 48
        // Total: 1344 + 48 + 48 = 1440
        let (_, indices) = generate_sphere(1.0, 16, 16);
        assert_eq!(indices.len(), 1440);
    }

    #[test]
    fn test_sphere_radius() {
        let (vertices, _) = generate_sphere(2.0, 8, 8);
        for v in &vertices {
            let dist =
                (v.position[0].powi(2) + v.position[1].powi(2) + v.position[2].powi(2)).sqrt();
            // Pole vertices are at exactly radius, others should be close
            assert!((dist - 2.0).abs() < 0.01);
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
    fn test_sphere_triangle_winding() {
        let radius = 1.0;
        let segments = 16;
        let rings = 16;
        let (vertices, indices) = generate_sphere(radius, segments, rings);

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
                continue; // Skip degenerate triangles (poles)
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
    fn test_sphere_pole_tangent_validity() {
        // Tangents should be normalized and non-degenerate at poles
        let radius = 1.0;
        let segments = 16;
        let rings = 16;
        let (vertices, _) = generate_sphere(radius, segments, rings);

        let middle_ring_count = (rings - 1) * (segments + 1);
        let top_pole_start = middle_ring_count as usize;
        let bottom_pole_start = top_pole_start + (segments + 1) as usize;

        // Check top pole tangents
        for i in 0..=segments {
            let v = &vertices[top_pole_start + i as usize];
            let tx = v.tangent[0];
            let ty = v.tangent[1];
            let tz = v.tangent[2];
            let len = (tx * tx + ty * ty + tz * tz).sqrt();
            // Tangents should be normalized (or close to it)
            assert!(
                (len - 1.0).abs() < 0.01,
                "Top pole vertex {} has non-unit tangent: len={}",
                i,
                len
            );
        }

        // Check bottom pole tangents
        for i in 0..=segments {
            let v = &vertices[bottom_pole_start + i as usize];
            let tx = v.tangent[0];
            let ty = v.tangent[1];
            let tz = v.tangent[2];
            let len = (tx * tx + ty * ty + tz * tz).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "Bottom pole vertex {} has non-unit tangent: len={}",
                i,
                len
            );
        }
    }

    #[test]
    fn test_sphere_normal_is_normalized_position() {
        // All normals should be the normalized position for a sphere centered at origin
        let radius = 2.0;
        let segments = 16;
        let rings = 16;
        let (vertices, _) = generate_sphere(radius, segments, rings);

        for v in &vertices {
            // Compute expected normal = normalize(position)
            let pos = v.position;
            let len = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
            let expected = [pos[0] / len, pos[1] / len, pos[2] / len];

            // Check normal matches
            let dx = (v.normal[0] - expected[0]).abs();
            let dy = (v.normal[1] - expected[1]).abs();
            let dz = (v.normal[2] - expected[2]).abs();

            assert!(
                dx < 1e-5 && dy < 1e-5 && dz < 1e-5,
                "Normal {:?} doesn't match normalized position {:?}",
                v.normal,
                expected
            );
        }
    }
}
