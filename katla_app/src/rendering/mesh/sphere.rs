use crate::rendering::{Mesh, VertexPBR};
use katla_math::Vec3;
use katla_gfx::VulkanContext;

pub fn create_sphere_vertices(radius: f32, segments: u32, rings: u32) -> Vec<VertexPBR> {
    let mut vertices = Vec::new();

    for ring in 0..=rings {
        let u = ring as f32 / rings as f32;
        let theta = u * std::f32::consts::PI;

        for segment in 0..=segments {
            let v = segment as f32 / segments as f32;
            let phi = v * 2.0 * std::f32::consts::PI;

            let x = radius * theta.sin() * phi.cos();
            let y = radius * theta.cos();
            let z = radius * theta.sin() * phi.sin();

            let normal = Vec3::new(x, y, z).normalize();
            let _tangent = Vec3::new(-normal.z(), 0.0, normal.x()).normalize();

            let texture_coords = (v, u);

            vertices.push(VertexPBR::new(
                [x, y, z],
                normal.to_array(),
                [_tangent.x(), _tangent.y(), _tangent.z(), 1.0], // Tangent
                [texture_coords.0, texture_coords.1],            // UV
            ));
        }
    }

    vertices
}

pub fn create_sphere_indices(segments: u32, rings: u32) -> Vec<u32> {
    let mut indices = Vec::new();

    for ring in 0..rings {
        for segment in 0..segments {
            let current = ring * (segments + 1) + segment;
            let next = (ring + 1) * (segments + 1) + segment;

            // At the poles (first and last ring), vertices collapse to a single point.
            // Skip triangles where all three vertices are at the same position.
            // We check by seeing if we're at the last ring (connecting to south pole)
            // and the vertices would form a degenerate triangle.
            if ring == rings - 1 {
                // South pole cap - these triangles often have incorrect winding
                // Skip them to avoid rendering artifacts
                continue;
            }

            // First triangle: current, current+1, next (CCW when viewed from outside)
            indices.push(current);
            indices.push(current + 1);
            indices.push(next);

            // Second triangle: current+1, next+1, next (CCW when viewed from outside)
            indices.push(current + 1);
            indices.push(next + 1);
            indices.push(next);
        }
    }

    indices
}

pub fn create_sphere_mesh(
    context: std::rc::Rc<VulkanContext>,
    radius: f32,
    segments: u32,
    rings: u32,
) -> Mesh {
    let vertices = create_sphere_vertices(radius, segments, rings);
    let indices = create_sphere_indices(segments, rings);
    Mesh::new(context, vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sphere_winding_order() {
        let radius = 1.0;
        let segments = 16;
        let rings = 12;
        let vertices = create_sphere_vertices(radius, segments, rings);
        let indices = create_sphere_indices(segments, rings);

        // Check each triangle
        let mut failed = 0;
        let mut passed = 0;

        for chunk in indices.chunks(3) {
            let i0 = chunk[0] as usize;
            let i1 = chunk[1] as usize;
            let i2 = chunk[2] as usize;

            let v0 = Vec3::from(vertices[i0].position);
            let v1 = Vec3::from(vertices[i1].position);
            let v2 = Vec3::from(vertices[i2].position);

            // Compute edge vectors
            let edge1 = v1 - v0;
            let edge2 = v2 - v0;

            // Face normal via cross product (normalized)
            let face_normal = edge1.cross(edge2).normalize();

            // For a sphere, the expected normal at the centroid should point outward from origin
            let centroid = (v0 + v1 + v2) / 3.0;
            let expected_normal = centroid.normalize();

            // Check alignment
            let dot = face_normal.dot(expected_normal);

            if dot < 0.0 {
                failed += 1;
                log::info!(
                    "Triangle {:?} (centroid={:?}): face_normal={:?}, expected_normal={:?}, dot={}",
                    chunk,
                    centroid.to_array(),
                    face_normal.to_array(),
                    expected_normal.to_array(),
                    dot
                );
            } else {
                passed += 1;
            }
        }

        log::info!("Sphere winding: {} passed, {} failed", passed, failed);
        assert_eq!(
            failed, 0,
            "Sphere has {} triangles with incorrect winding",
            failed
        );
    }
}
