use crate::rendering::{Mesh, VertexPBR};
use katla_math::Vec3;
use katla_vulkan::VulkanContext;

pub fn create_cube_vertices(size: Vec3) -> Vec<VertexPBR> {
    let half_size = size / 2.0;
    let mut vertices = Vec::new();

    // Front face (Z+) - use full UV range [0,1]
    let n = Vec3::new(0.0, 0.0, 1.0);
    vertices.push(VertexPBR::new([-half_size.x(), -half_size.y(), half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [0.0, 0.0]));
    vertices.push(VertexPBR::new([-half_size.x(), half_size.y(), half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [0.0, 1.0]));
    vertices.push(VertexPBR::new([half_size.x(), half_size.y(), half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [1.0, 1.0]));
    vertices.push(VertexPBR::new([half_size.x(), -half_size.y(), half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [1.0, 0.0]));

    // Back face (Z-) - use full UV range [0,1]
    let n = Vec3::new(0.0, 0.0, -1.0);
    vertices.push(VertexPBR::new([half_size.x(), -half_size.y(), -half_size.z()], n.to_array(), [-1.0, 0.0, 0.0, 1.0], [0.0, 0.0]));
    vertices.push(VertexPBR::new([half_size.x(), half_size.y(), -half_size.z()], n.to_array(), [-1.0, 0.0, 0.0, 1.0], [0.0, 1.0]));
    vertices.push(VertexPBR::new([-half_size.x(), half_size.y(), -half_size.z()], n.to_array(), [-1.0, 0.0, 0.0, 1.0], [1.0, 1.0]));
    vertices.push(VertexPBR::new([-half_size.x(), -half_size.y(), -half_size.z()], n.to_array(), [-1.0, 0.0, 0.0, 1.0], [1.0, 0.0]));

    // Left face (X-) - use full UV range [0,1]
    let n = Vec3::new(-1.0, 0.0, 0.0);
    vertices.push(VertexPBR::new([-half_size.x(), -half_size.y(), -half_size.z()], n.to_array(), [0.0, 0.0, 1.0, 0.0], [0.0, 0.0]));
    vertices.push(VertexPBR::new([-half_size.x(), half_size.y(), -half_size.z()], n.to_array(), [0.0, 0.0, 1.0, 0.0], [0.0, 1.0]));
    vertices.push(VertexPBR::new([-half_size.x(), half_size.y(), half_size.z()], n.to_array(), [0.0, 0.0, 1.0, 0.0], [1.0, 1.0]));
    vertices.push(VertexPBR::new([-half_size.x(), -half_size.y(), half_size.z()], n.to_array(), [0.0, 0.0, 1.0, 0.0], [1.0, 0.0]));

    // Right face (X+) - use full UV range [0,1]
    let n = Vec3::new(1.0, 0.0, 0.0);
    vertices.push(VertexPBR::new([half_size.x(), -half_size.y(), half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [0.0, 0.0]));
    vertices.push(VertexPBR::new([half_size.x(), half_size.y(), half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [0.0, 1.0]));
    vertices.push(VertexPBR::new([half_size.x(), half_size.y(), -half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [1.0, 1.0]));
    vertices.push(VertexPBR::new([half_size.x(), -half_size.y(), -half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [1.0, 0.0]));

    // Top face (Y+) - use full UV range [0,1]
    let n = Vec3::new(0.0, 1.0, 0.0);
    vertices.push(VertexPBR::new([-half_size.x(), half_size.y(), -half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [0.0, 0.0]));
    vertices.push(VertexPBR::new([-half_size.x(), half_size.y(), half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [0.0, 1.0]));
    vertices.push(VertexPBR::new([half_size.x(), half_size.y(), half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [1.0, 1.0]));
    vertices.push(VertexPBR::new([half_size.x(), half_size.y(), -half_size.z()], n.to_array(), [1.0, 0.0, 0.0, 1.0], [1.0, 0.0]));

    // Bottom face (Y-) - use full UV range [0,1]
    let n = Vec3::new(0.0, -1.0, 0.0);
    vertices.push(VertexPBR::new([-half_size.x(), -half_size.y(), half_size.z()], n.to_array(), [1.0, 0.0, 0.0, -1.0], [0.0, 0.0]));
    vertices.push(VertexPBR::new([-half_size.x(), -half_size.y(), -half_size.z()], n.to_array(), [1.0, 0.0, 0.0, -1.0], [0.0, 1.0]));
    vertices.push(VertexPBR::new([half_size.x(), -half_size.y(), -half_size.z()], n.to_array(), [1.0, 0.0, 0.0, -1.0], [1.0, 1.0]));
    vertices.push(VertexPBR::new([half_size.x(), -half_size.y(), half_size.z()], n.to_array(), [1.0, 0.0, 0.0, -1.0], [1.0, 0.0]));

    vertices
}

pub fn create_cube_mesh(context: std::rc::Rc<VulkanContext>, size: Vec3) -> Mesh {
    let vertices = create_cube_vertices(size);
    // Each face has 4 vertices. For CCW winding when viewed from outside:
    // Front: 0,1,3, 1,2,3 | Back: 4,7,5, 7,6,5 | Left: 8,11,9, 11,10,9 | Right: 12,13,15, 13,14,15
    // Top: 16,17,19, 17,18,19 | Bottom: 20,23,21, 23,22,21
    let indices = vec![
        // Front face (Z+): [0,2,1], [0,3,2] gives +Z normal
        0, 2, 1, 0, 3, 2,
        // Back face (Z-): [4,6,5], [4,7,6] gives -Z normal
        4, 6, 5, 4, 7, 6,
        // Left face (X-): [8,10,9], [8,11,10] gives -X normal
        8, 10, 9, 8, 11, 10,
        // Right face (X+): [12,15,13], [13,15,14] gives +X normal
        12, 15, 13, 13, 15, 14,
        // Top face (Y+): [16,17,19], [17,18,19] gives +Y normal
        16, 17, 19, 17, 18, 19,
        // Bottom face (Y-): [20,21,23], [21,22,23] gives -Y normal
        20, 21, 23, 21, 22, 23,
    ];
    Mesh::new(context, vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cube_winding_order() {
        let size = Vec3::new(2.0, 2.0, 2.0);
        let vertices = create_cube_vertices(size);
        let indices = vec![
            // Front face (Z+)
            0, 2, 1, 0, 3, 2,
            // Back face (Z-)
            4, 6, 5, 4, 7, 6,
            // Left face (X-)
            8, 10, 9, 8, 11, 10,
            // Right face (X+)
            12, 15, 13, 13, 15, 14,
            // Top face (Y+)
            16, 17, 19, 17, 18, 19,
            // Bottom face (Y-)
            20, 21, 23, 21, 22, 23,
        ];

        // Check each triangle
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

            // For outward-facing triangles, face_normal and avg_normal should point in the same direction
            let dot = face_normal.dot(avg_normal);
            println!(
                "Triangle {:?}: face_normal={:?}, avg_normal={:?}, dot={}",
                chunk, face_normal.0, avg_normal.0, dot
            );

            // They should be roughly aligned (dot product > 0)
            assert!(
                dot > 0.1,
                "Triangle {:?} has incorrect winding: face_normal={:?}, avg_normal={:?}, dot={}",
                chunk, face_normal.0, avg_normal.0, dot
            );
        }
    }
}
