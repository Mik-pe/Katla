use crate::rendering::{Mesh, VertexPBR};
use katla_math::Vec3;
use katla_vulkan::VulkanContext;

pub fn create_torus_vertices(
    major_radius: f32,
    minor_radius: f32,
    segments: u32,
    rings: u32,
) -> Vec<VertexPBR> {
    let mut vertices = Vec::new();

    for ring in 0..=rings {
        let u = ring as f32 / rings as f32;
        let theta = u * 2.0 * std::f32::consts::PI;

        for segment in 0..=segments {
            let v = segment as f32 / segments as f32;
            let phi = v * 2.0 * std::f32::consts::PI;

            let x = (major_radius + minor_radius * theta.cos()) * phi.cos();
            let y = minor_radius * theta.sin();
            let z = (major_radius + minor_radius * theta.cos()) * phi.sin();

            let center_x = major_radius * phi.cos();
            let center_z = major_radius * phi.sin();

            let normal = Vec3::new(x - center_x, y, z - center_z).normalize();
            let _tangent = Vec3::new(-normal.z(), 0.0, normal.x()).normalize();

            let texture_coords = (v, u);

            vertices.push(VertexPBR::new(
                [x, y, z],
                normal.0,
                [_tangent.x(), _tangent.y(), _tangent.z(), 1.0],  // Tangent
                [texture_coords.0, texture_coords.1],              // UV
            ));
        }
    }

    vertices
}

pub fn create_torus_indices(segments: u32, rings: u32) -> Vec<u32> {
    let mut indices = Vec::new();

    for ring in 0..rings {
        for segment in 0..segments {
            let current = ring * (segments + 1) + segment;
            let next = (ring + 1) * (segments + 1) + segment;

            // First triangle: current, next, current+1 (CCW when viewed from outside)
            indices.push(current);
            indices.push(next);
            indices.push(current + 1);

            // Second triangle: current+1, next, next+1 (CCW when viewed from outside)
            indices.push(current + 1);
            indices.push(next);
            indices.push(next + 1);
        }
    }

    indices
}

pub fn create_torus_mesh(
    context: std::rc::Rc<VulkanContext>,
    major_radius: f32,
    minor_radius: f32,
    segments: u32,
    rings: u32,
) -> Mesh {
    let vertices = create_torus_vertices(major_radius, minor_radius, segments, rings);
    let indices = create_torus_indices(segments, rings);
    Mesh::new(context, vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_torus_winding_order() {
        let major_radius = 3.0;
        let minor_radius = 1.0;
        let segments = 16;
        let rings = 12;
        let vertices = create_torus_vertices(major_radius, minor_radius, segments, rings);
        let indices = create_torus_indices(segments, rings);

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

            // Average vertex normal (torus has inward normals on inner ring)
            let n0 = Vec3::from(vertices[i0].normal);
            let n1 = Vec3::from(vertices[i1].normal);
            let n2 = Vec3::from(vertices[i2].normal);
            let avg_normal = (n0 + n1 + n2).normalize();

            // Face normal should match vertex normals (whether inward or outward)
            let dot = face_normal.dot(avg_normal);

            if dot < 0.0 {
                failed += 1;
                println!(
                    "Triangle {:?}: face_normal={:?}, avg_normal={:?}, dot={}",
                    chunk, face_normal.0, avg_normal.0, dot
                );
            } else {
                passed += 1;
            }
        }

        println!("Torus winding: {} passed, {} failed", passed, failed);
        assert_eq!(failed, 0, "Torus has {} triangles with incorrect winding", failed);
    }
}
