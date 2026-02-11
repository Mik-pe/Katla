use crate::rendering::{Mesh, VertexPBR};
use katla_vulkan::VulkanContext;

pub fn create_plane_vertices(width: f32, height: f32, segments: u32) -> Vec<VertexPBR> {
    let mut vertices = Vec::new();

    let half_width = width / 2.0;
    let half_height = height / 2.0;

    // Create a grid of vertices
    for row in 0..=segments {
        let v = row as f32 / segments as f32;
        let y = v * height - half_height;

        for col in 0..=segments {
            let u = col as f32 / segments as f32;
            let x = u * width - half_width;

            vertices.push(VertexPBR::new(
                [x, y, 0.0],
                [0.0, 0.0, 1.0],      // Normal pointing up
                [1.0, 0.0, 0.0, 1.0], // Tangent
                [u, v],               // UV coordinates spanning [0,1]
            ));
        }
    }

    vertices
}

pub fn create_plane_indices(segments: u32) -> Vec<u32> {
    let mut indices = Vec::new();

    for row in 0..segments {
        for col in 0..segments {
            let current = row * (segments + 1) + col;
            let next_row = (row + 1) * (segments + 1) + col;

            // First triangle: current, current+1, next_row (CCW when viewed from above)
            indices.push(current);
            indices.push(current + 1);
            indices.push(next_row);

            // Second triangle: current+1, next_row+1, next_row (CCW when viewed from above)
            indices.push(current + 1);
            indices.push(next_row + 1);
            indices.push(next_row);
        }
    }

    indices
}

pub fn create_plane_mesh(
    context: std::rc::Rc<VulkanContext>,
    width: f32,
    height: f32,
    segments: u32,
) -> Mesh {
    let vertices = create_plane_vertices(width, height, segments);
    let indices = create_plane_indices(segments);
    Mesh::new(context, vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::Vec3;

    #[test]
    fn test_plane_winding_order() {
        let width = 2.0;
        let height = 2.0;
        let segments = 2;
        let vertices = create_plane_vertices(width, height, segments);
        let indices = create_plane_indices(segments);

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

            // Average vertex normal
            let n0 = Vec3::from(vertices[i0].normal);
            let n1 = Vec3::from(vertices[i1].normal);
            let n2 = Vec3::from(vertices[i2].normal);
            let avg_normal = (n0 + n1 + n2).normalize();

            // For upward-facing triangles, both should point in +Z direction
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

        println!("Plane winding: {} passed, {} failed", passed, failed);
        assert_eq!(
            failed, 0,
            "Plane has {} triangles with incorrect winding",
            failed
        );
    }
}
