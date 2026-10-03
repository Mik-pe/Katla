//! Decode triangle primitives without combining indices or material assignments.

use gltf::{Node, Primitive, buffer::Data};
use katla_gfx::{VertexPBR, VertexPBRSkinned};
use katla_math::{AABB, Mat3, Mat4, Vec3};

use super::{GLTFModel, GltfPrimitive, GltfVertices, gltf_material::GltfMaterialInfo};

pub(super) fn decode(
    buffers: &[Data],
    node: &Node<'_>,
    primitive_index: usize,
    primitive: &Primitive<'_>,
    transform: Mat4,
) -> Result<GltfPrimitive, String> {
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| &data.0[..]));
    let positions: Vec<_> = reader
        .read_positions()
        .ok_or("POSITION is required")?
        .collect();
    let count = positions.len();
    if count == 0 || count > u32::MAX as usize {
        return Err("Vertex count must fit a nonempty u32 index buffer".into());
    }
    let source_indices: Vec<u32> = reader
        .read_indices()
        .map(|indices| indices.into_u32().collect())
        .unwrap_or_else(|| (0..count as u32).collect());
    if source_indices.iter().any(|index| *index as usize >= count) {
        return Err("Index refers to a vertex outside POSITION".into());
    }
    let mut indices = triangulate(primitive.mode(), &source_indices)?;
    u32::try_from(indices.len()).map_err(|_| "Triangle index count exceeds u32")?;
    let normals: Option<Vec<_>> = reader.read_normals().map(Iterator::collect);
    let tangents: Option<Vec<_>> = reader.read_tangents().map(Iterator::collect);
    let uv: Option<Vec<_>> = reader
        .read_tex_coords(0)
        .map(|values| values.into_f32().collect());
    let uv1: Option<Vec<_>> = reader
        .read_tex_coords(1)
        .map(|values| values.into_f32().collect());
    if let Some(values) = &uv1 {
        check_attribute("TEXCOORD_1", values, count)?;
    }
    check_attribute("POSITION", &positions, count)?;
    if let Some(values) = &normals {
        check_attribute("NORMAL", values, count)?;
    }
    if let Some(values) = &tangents {
        check_attribute("TANGENT", values, count)?;
    }
    if let Some(values) = &uv {
        check_attribute("TEXCOORD_0", values, count)?;
    }
    let skin_index = node.skin().map(|skin| skin.index());
    let joints: Option<Vec<_>> = reader
        .read_joints(0)
        .map(|values| values.into_u16().collect());
    let weights: Option<Vec<_>> = reader
        .read_weights(0)
        .map(|values| values.into_f32().collect());
    if let Some(skin) = node.skin() {
        let joints = joints
            .as_ref()
            .ok_or("Skinned primitive requires JOINTS_0")?;
        let weights = weights
            .as_ref()
            .ok_or("Skinned primitive requires WEIGHTS_0")?;
        if joints.len() != count {
            return Err("JOINTS_0 count differs from POSITION".into());
        }
        check_attribute("WEIGHTS_0", weights, count)?;
        let joint_count = skin.joints().count();
        if joints
            .iter()
            .flatten()
            .any(|index| usize::from(*index) >= joint_count)
        {
            return Err("JOINTS_0 refers outside this node's skin".into());
        }
        if weights.iter().any(|values| {
            values.iter().any(|value| *value < 0.0) || values.iter().sum::<f32>() <= 0.0
        }) {
            return Err("Skin weights must be nonnegative with a positive sum".into());
        }
    }
    let expand = normals.is_none() || (tangents.is_none() && uv.is_some());
    let source_vertices: Vec<usize> = if expand {
        indices.iter().map(|index| *index as usize).collect()
    } else {
        (0..count).collect()
    };
    let mut vertices: Vec<_> = source_vertices
        .iter()
        .map(|&index| {
            let normal = normals.as_ref().map_or(Vec3::Y_AXIS, |values| {
                normalized(values[index], Vec3::Y_AXIS)
            });
            VertexPBR {
                position: positions[index],
                normal: normal.to_array(),
                tangent: tangents
                    .as_ref()
                    .map_or_else(|| fallback_tangent(normal), |values| values[index]),
                tex_coord0: uv.as_ref().map_or([0.0; 2], |values| values[index]),
                tex_coord1: uv1.as_ref().map_or([0.0; 2], |values| values[index]),
            }
        })
        .collect();
    if expand {
        indices = (0..vertices.len() as u32).collect();
    }
    if normals.is_none() {
        for triangle in vertices.as_chunks_mut::<3>().0 {
            let a = Vec3::from(triangle[0].position);
            let b = Vec3::from(triangle[1].position);
            let c = Vec3::from(triangle[2].position);
            let normal = normalized((b - a).cross(c - a).to_array(), Vec3::Y_AXIS);
            for vertex in triangle {
                vertex.normal = normal.to_array();
                if tangents.is_none() {
                    vertex.tangent = fallback_tangent(normal);
                }
            }
        }
    }
    if tangents.is_none() && uv.is_some() {
        bevy_mikktspace::generate_tangents(&mut TangentGeometry(&mut vertices))
            .map_err(|error| format!("Tangent generation failed: {error}"))?;
    }
    let vertices = if skin_index.is_some() {
        let joints = joints.ok_or("Skinned primitive requires JOINTS_0")?;
        let weights = weights.ok_or("Skinned primitive requires WEIGHTS_0")?;
        GltfVertices::Skinned(
            vertices
                .iter()
                .zip(source_vertices)
                .map(|(vertex, index)| {
                    let sum = weights[index].iter().sum::<f32>();
                    VertexPBRSkinned {
                        position: vertex.position,
                        normal: vertex.normal,
                        tangent: vertex.tangent,
                        tex_coord0: vertex.tex_coord0,
                        tex_coord1: vertex.tex_coord1,
                        joint_indices: joints[index],
                        joint_weights: weights[index].map(|weight| weight / sum),
                    }
                })
                .collect(),
        )
    } else {
        GLTFModel::transform_vertex_data(&mut vertices, &transform);
        let matrix = Mat3::from(transform);
        if matrix[0].dot(matrix[1].cross(matrix[2])) < 0.0 {
            for triangle in indices.as_chunks_mut::<3>().0 {
                triangle.swap(1, 2);
            }
        }
        GltfVertices::Static(vertices)
    };
    let positions = vertices.positions();
    check_attribute("Transformed POSITION", &positions, positions.len())?;
    let mut min = Vec3::from(positions[0]);
    let mut max = min;
    for position in positions {
        let p = Vec3::from(position);
        min = Vec3::new(min.x().min(p.x()), min.y().min(p.y()), min.z().min(p.z()));
        max = Vec3::new(max.x().max(p.x()), max.y().max(p.y()), max.z().max(p.z()));
    }
    Ok(GltfPrimitive {
        node_index: node.index(),
        primitive_index,
        name: format!(
            "{} / primitive {primitive_index}",
            node.name().unwrap_or("Mesh")
        ),
        material: GltfMaterialInfo::from_gltf(&primitive.material()),
        vertices,
        indices,
        bounds: AABB::from_min_max(min, max),
        skin_index,
    })
}

fn check_attribute<const N: usize>(
    name: &str,
    values: &[[f32; N]],
    count: usize,
) -> Result<(), String> {
    if values.len() != count {
        return Err(format!("{name} count differs from POSITION"));
    }
    if values.iter().flatten().any(|value| !value.is_finite()) {
        return Err(format!("{name} contains a nonfinite value"));
    }
    Ok(())
}

fn triangulate(mode: gltf::mesh::Mode, indices: &[u32]) -> Result<Vec<u32>, String> {
    use gltf::mesh::Mode;
    if indices.len() < 3 {
        return Err("Triangle primitive requires at least three indices".into());
    }
    match mode {
        Mode::Triangles => {
            if !indices.len().is_multiple_of(3) {
                return Err("TRIANGLES index count must be a multiple of three".into());
            }
            Ok(indices.to_vec())
        }
        Mode::TriangleStrip => Ok(indices
            .windows(3)
            .enumerate()
            .flat_map(|(i, t)| {
                if i % 2 == 0 {
                    [t[0], t[1], t[2]]
                } else {
                    [t[1], t[0], t[2]]
                }
            })
            .collect()),
        Mode::TriangleFan => Ok(indices[1..]
            .windows(2)
            .flat_map(|pair| [indices[0], pair[0], pair[1]])
            .collect()),
        _ => Err(format!(
            "Unsupported glTF primitive topology {mode:?}; triangle geometry is required"
        )),
    }
}

fn normalized(value: [f32; 3], fallback: Vec3) -> Vec3 {
    let value = Vec3::from(value);
    let length_sq = value.dot(value);
    if length_sq > 1e-12 {
        value / length_sq.sqrt()
    } else {
        fallback
    }
}

fn fallback_tangent(normal: Vec3) -> [f32; 4] {
    let axis = if normal.x().abs() > 0.9 {
        Vec3::Z_AXIS
    } else {
        Vec3::X_AXIS
    };
    let tangent = normalized((axis - normal * axis.dot(normal)).to_array(), Vec3::X_AXIS);
    [tangent.x(), tangent.y(), tangent.z(), 1.0]
}

struct TangentGeometry<'a>(&'a mut [VertexPBR]);

impl bevy_mikktspace::Geometry for TangentGeometry<'_> {
    fn num_faces(&self) -> usize {
        self.0.len() / 3
    }
    fn num_vertices_of_face(&self, _: usize) -> usize {
        3
    }
    fn position(&self, face: usize, vertex: usize) -> [f32; 3] {
        self.0[face * 3 + vertex].position
    }
    fn normal(&self, face: usize, vertex: usize) -> [f32; 3] {
        self.0[face * 3 + vertex].normal
    }
    fn tex_coord(&self, face: usize, vertex: usize) -> [f32; 2] {
        self.0[face * 3 + vertex].tex_coord0
    }
    fn set_tangent(
        &mut self,
        tangent: Option<bevy_mikktspace::TangentSpace>,
        face: usize,
        vertex: usize,
    ) {
        if let Some(tangent) = tangent {
            self.0[face * 3 + vertex].tangent = tangent.tangent_encoded();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triangle_strip_and_fan_preserve_winding() {
        assert_eq!(
            triangulate(gltf::mesh::Mode::TriangleStrip, &[0, 1, 2, 3, 4]).unwrap(),
            [0, 1, 2, 2, 1, 3, 2, 3, 4]
        );
        assert_eq!(
            triangulate(gltf::mesh::Mode::TriangleFan, &[0, 1, 2, 3, 4]).unwrap(),
            [0, 1, 2, 0, 2, 3, 0, 3, 4]
        );
    }
}
