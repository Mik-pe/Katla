//! Cone primitive generation.

use crate::vertex::VertexPBR;

/// Generates a cone mesh with base at y=0 and apex at y=height.
///
/// # Arguments
/// * `height` - Height of the cone along Y axis
/// * `base_radius` - Radius of the base circle
/// * `segments` - Number of radial divisions around the circumference
///
/// # Returns
/// A tuple of (vertices, indices) ready for mesh creation.
///
/// # Winding Order
/// Uses counter-clockwise (CCW) winding for front faces.
pub fn generate_cone(height: f32, base_radius: f32, segments: u32) -> (Vec<VertexPBR>, Vec<u32>) {
    assert!(segments >= 3, "segments must be at least 3");

    let mut vertices = Vec::with_capacity(segments as usize * 3 + 2);
    let mut indices = Vec::with_capacity(segments as usize * 6);

    // Apex vertex
    let apex_idx = 0u32;
    vertices.push(VertexPBR::new(
        [0.0, height, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.5, 1.0],
    ));

    // Base ring vertices (side faces)
    let base_ring_start = vertices.len() as u32;
    for segment in 0..=segments {
        let u = segment as f32 / segments as f32;
        let angle = u * 2.0 * std::f32::consts::PI;

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let x = base_radius * cos_a;
        let z = base_radius * sin_a;

        // Side normal: points outward and upward
        let slope_len = (base_radius * base_radius + height * height).sqrt();
        let normal = [
            height * cos_a / slope_len,
            base_radius / slope_len,
            height * sin_a / slope_len,
        ];

        vertices.push(VertexPBR::new(
            [x, 0.0, z],
            normal,
            [1.0, 0.0, 0.0, 1.0],
            [u, 0.0],
        ));
    }

    // Side face indices
    for segment in 0..segments {
        let current = base_ring_start + segment;
        let next = base_ring_start + segment + 1;
        // CCW winding: apex -> current -> next
        indices.extend_from_slice(&[apex_idx, next, current]);
    }

    // Bottom cap center vertex
    let bottom_center_idx = vertices.len() as u32;
    vertices.push(VertexPBR::new(
        [0.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.5, 0.5],
    ));

    // Bottom cap ring vertices
    let cap_ring_start = vertices.len() as u32;
    for segment in 0..=segments {
        let u = segment as f32 / segments as f32;
        let angle = u * 2.0 * std::f32::consts::PI;

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let x = base_radius * cos_a;
        let z = base_radius * sin_a;

        let uv = [0.5 + cos_a * 0.5, 0.5 + sin_a * 0.5];

        vertices.push(VertexPBR::new(
            [x, 0.0, z],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            uv,
        ));
    }

    // Bottom cap indices (CCW when viewed from outside/below)
    for segment in 0..segments {
        let current = cap_ring_start + segment;
        let next = cap_ring_start + segment + 1;
        indices.extend_from_slice(&[current, next, bottom_center_idx]);
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cone_vertex_count() {
        let (vertices, _) = generate_cone(1.0, 0.5, 16);
        // 1 apex + (16+1) side ring + 1 bottom center + (16+1) bottom ring = 36
        assert_eq!(vertices.len(), 36);
    }

    #[test]
    fn test_cone_index_count() {
        let (_, indices) = generate_cone(1.0, 0.5, 16);
        // Side: 16 * 3 = 48, Bottom cap: 16 * 3 = 48, Total: 96
        assert_eq!(indices.len(), 96);
    }

    #[test]
    fn test_cone_bounds() {
        let (vertices, _) = generate_cone(2.0, 1.0, 16);
        for v in &vertices {
            assert!(v.position[1] >= 0.0 && v.position[1] <= 2.0);
            assert!(v.position[0].abs() <= 1.0 + 1e-5);
            assert!(v.position[2].abs() <= 1.0 + 1e-5);
        }
    }

    #[test]
    fn test_cone_normals_normalized() {
        let (vertices, _) = generate_cone(2.0, 1.0, 16);
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
    #[should_panic]
    fn test_cone_invalid_segments() {
        generate_cone(1.0, 0.5, 2);
    }
}
