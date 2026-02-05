use crate::rendering::{Mesh, VertexPBR};
use katla_math::Vec3;
use katla_vulkan::VulkanContext;

pub fn create_cylinder_vertices(height: f32, radius: f32, segments: u32) -> Vec<VertexPBR> {
    let mut vertices = Vec::new();
    let half_height = height / 2.0;

    // Side vertices
    for segment in 0..=segments {
        let u = segment as f32 / segments as f32;
        let angle = u * 2.0 * std::f32::consts::PI;

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let lower_x = radius * cos_a;
        let lower_z = radius * sin_a;
        let upper_x = radius * cos_a;
        let upper_z = radius * sin_a;

        let normal = Vec3::new(cos_a, 0.0, sin_a).normalize();

        let lower_left = Vec3::new(lower_x, -half_height, lower_z);
        let lower_right = Vec3::new(lower_x, -half_height, lower_z);
        let upper_left = Vec3::new(upper_x, half_height, upper_z);
        let upper_right = Vec3::new(upper_x, half_height, upper_z);

        // UV coordinates: U goes around the cylinder (0-1), V goes along height (0-1)
        vertices.push(VertexPBR::new(
            lower_left.0,
            normal.0,
            [1.0, 0.0, 0.0, 1.0],  // Tangent
            [u, 0.0],              // UV: Bottom of cylinder
        ));

        vertices.push(VertexPBR::new(
            upper_left.0,
            normal.0,
            [1.0, 0.0, 0.0, 1.0],  // Tangent
            [u, 1.0],              // UV: Top of cylinder
        ));

        vertices.push(VertexPBR::new(
            upper_right.0,
            normal.0,
            [1.0, 0.0, 0.0, 1.0],  // Tangent
            [u, 1.0],              // UV: Top of cylinder
        ));

        vertices.push(VertexPBR::new(
            lower_right.0,
            normal.0,
            [1.0, 0.0, 0.0, 1.0],  // Tangent
            [u, 0.0],              // UV: Bottom of cylinder
        ));
    }

    // Bottom cap center vertex
    vertices.push(VertexPBR::new(
        [0.0, -half_height, 0.0],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],  // Tangent
        [0.5, 0.5],              // UV: Center of texture
    ));

    // Top cap center vertex
    vertices.push(VertexPBR::new(
        [0.0, half_height, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],  // Tangent
        [0.5, 0.5],              // UV: Center of texture
    ));

    vertices
}

pub fn create_cylinder_indices(segments: u32) -> Vec<u32> {
    let mut indices = Vec::new();

    // Side faces
    for segment in 0..segments {
        let current = segment * 4;
        let next = (segment + 1) * 4;

        // First triangle: lower_left, upper_left, lower_right (counter-clockwise)
        indices.push(current);
        indices.push(current + 1);
        indices.push(next);

        // Second triangle: upper_left, upper_right, lower_right (counter-clockwise)
        indices.push(current + 1);
        indices.push(next + 1);
        indices.push(next);
    }

    // Center vertex indices
    let bottom_center = (segments + 1) * 4;
    let top_center = (segments + 1) * 4 + 1;

    // Bottom cap triangles (wound counter-clockwise when viewed from below)
    for segment in 0..segments {
        let current = segment * 4;  // lower vertex of current segment
        let next = ((segment + 1) % (segments + 1)) * 4;  // lower vertex of next segment

        // Triangle: bottom_center -> current_lower -> next_lower (reversed for CCW from below)
        indices.push(bottom_center);
        indices.push(current);
        indices.push(next);
    }

    // Top cap triangles (wound counter-clockwise when viewed from above)
    for segment in 0..segments {
        let current = segment * 4 + 1;  // upper vertex of current segment
        let next = ((segment + 1) % (segments + 1)) * 4 + 1;  // upper vertex of next segment

        // Triangle: top_center -> next_upper -> current_upper (reversed for CCW from above)
        indices.push(top_center);
        indices.push(next);
        indices.push(current);
    }

    indices
}

pub fn create_cylinder_mesh(
    context: std::rc::Rc<VulkanContext>,
    height: f32,
    radius: f32,
    segments: u32,
) -> Mesh {
    let vertices = create_cylinder_vertices(height, radius, segments);
    let indices = create_cylinder_indices(segments);
    Mesh::new(context, vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cylinder_winding_order() {
        let height = 2.0;
        let radius = 1.0;
        let segments = 16;
        let vertices = create_cylinder_vertices(height, radius, segments);
        let indices = create_cylinder_indices(segments);

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

            // For outward-facing triangles, face_normal and avg_normal should point in the same direction
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

        println!("Cylinder winding: {} passed, {} failed", passed, failed);
        assert_eq!(failed, 0, "Cylinder has {} triangles with incorrect winding", failed);
    }
}
