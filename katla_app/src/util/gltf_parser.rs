//! GLTF buffer parsing utilities.
//!
//! This module provides utilities for parsing GLTF buffer data into vertex and index formats.

use byteorder::{ByteOrder, LittleEndian};
use gltf::buffer::Data as BufferData;
use katla_math::{Sphere, Vec3};

use crate::rendering::VertexPBR;

/// Represents parsed mesh data from a GLTF primitive.
#[derive(Clone)]
pub struct ParsedMesh {
    /// Vertex data in PBR format
    pub vertices: Vec<VertexPBR>,
    /// Index data (raw bytes)
    pub indices: Vec<u8>,
    /// Stride of index data (1, 2, or 4 bytes per index)
    pub index_stride: u8,
    /// Bounding sphere of the mesh
    pub bounds: Sphere,
}

/// GLTF attribute parser using accessor iterators.
pub struct AttributeParser<'a> {
    buffers: &'a [BufferData],
}

impl<'a> AttributeParser<'a> {
    pub fn new(buffers: &'a [BufferData]) -> Self {
        Self { buffers }
    }

    /// Parse position data from an accessor.
    pub fn parse_positions(&self, accessor: gltf::Accessor<'a>) -> Vec<[f32; 3]> {
        accessor
            .view()
            .and_then(|view| self.parse_vec3_accessor(accessor, view))
            .unwrap_or_default()
    }

    /// Parse normal data from an accessor.
    pub fn parse_normals(&self, accessor: gltf::Accessor<'a>) -> Vec<[f32; 3]> {
        accessor
            .view()
            .and_then(|view| self.parse_vec3_accessor(accessor, view))
            .unwrap_or_default()
    }

    /// Parse tex coord data from an accessor.
    pub fn parse_tex_coords(&self, accessor: gltf::Accessor<'a>) -> Vec<[f32; 2]> {
        accessor
            .view()
            .and_then(|view| self.parse_vec2_accessor(accessor, view))
            .unwrap_or_default()
    }

    /// Parse index data from an accessor.
    pub fn parse_indices(&self, accessor: gltf::Accessor<'a>) -> (Vec<u8>, u8) {
        let view = accessor
            .view()
            .expect("Index accessor must have a buffer view");
        let buf_index = view.buffer().index();
        let ind_offset = view.offset() + accessor.offset();
        let ind_size = view.length();
        let ind_buf = &self.buffers[buf_index];
        let index_data = ind_buf[ind_offset..ind_offset + ind_size].to_vec();
        let index_stride = accessor.size() as u8;
        (index_data, index_stride)
    }

    /// Helper to parse Vec3 data from an accessor with its view.
    fn parse_vec3_accessor(
        &self,
        accessor: gltf::Accessor<'a>,
        view: gltf::buffer::View<'a>,
    ) -> Option<Vec<[f32; 3]>> {
        let buf_index = view.buffer().index();
        let buf_stride = view.stride();
        let attr_buf = &self.buffers[buf_index];

        // Calculate indices
        let start_index = accessor.offset() + view.offset();
        let stride = buf_stride.unwrap_or(accessor.size());
        let total_size = accessor.size() * accessor.count();
        let end_index = start_index + total_size;

        let attr_arr = &attr_buf[start_index..end_index];

        // Parse based on data type
        if accessor.data_type() == gltf::accessor::DataType::F32 {
            Some(
                attr_arr
                    .chunks(stride)
                    .map(|bytes| {
                        [
                            LittleEndian::read_f32(&bytes[0..4]),
                            LittleEndian::read_f32(&bytes[4..8]),
                            LittleEndian::read_f32(&bytes[8..12]),
                        ]
                    })
                    .collect(),
            )
        } else {
            // Unsupported data type
            None
        }
    }

    /// Helper to parse Vec2 data from an accessor with its view.
    fn parse_vec2_accessor(
        &self,
        accessor: gltf::Accessor<'a>,
        view: gltf::buffer::View<'a>,
    ) -> Option<Vec<[f32; 2]>> {
        let buf_index = view.buffer().index();
        let buf_stride = view.stride();
        let attr_buf = &self.buffers[buf_index];

        // Calculate indices
        let start_index = accessor.offset() + view.offset();
        let stride = buf_stride.unwrap_or(accessor.size());
        let total_size = accessor.size() * accessor.count();
        let end_index = start_index + total_size;

        let attr_arr = &attr_buf[start_index..end_index];

        // Parse based on data type
        if accessor.data_type() == gltf::accessor::DataType::F32 {
            Some(
                attr_arr
                    .chunks(stride)
                    .map(|bytes| {
                        [
                            LittleEndian::read_f32(&bytes[0..4]),
                            LittleEndian::read_f32(&bytes[4..8]),
                        ]
                    })
                    .collect(),
            )
        } else {
            // Unsupported data type
            None
        }
    }
}

/// Generate smooth vertex normals from triangle data.
///
/// This calculates normals by averaging the face normals of all triangles
/// that share each vertex. This produces smooth shading compared to flat shading.
pub fn generate_smooth_normals(
    positions: &[[f32; 3]],
    indices: &[u8],
    index_stride: u8,
) -> Vec<[f32; 3]> {
    use std::collections::HashMap;

    let mut normals: Vec<Vec3> = vec![Vec3::new(0.0, 0.0, 0.0); positions.len()];
    let mut counts: Vec<usize> = vec![0; positions.len()];

    // Handle empty or invalid index data
    if indices.is_empty() || index_stride == 0 {
        // No index data - compute flat normals from sequential triplets
        for i in (0..positions.len()).step_by(3) {
            if i + 2 >= positions.len() {
                break;
            }

            let v0 = Vec3::from(positions[i]);
            let v1 = Vec3::from(positions[i + 1]);
            let v2 = Vec3::from(positions[i + 2]);

            // Calculate face normal using cross product
            let edge1 = Vec3::new(v1.x() - v0.x(), v1.y() - v0.y(), v1.z() - v0.z());
            let edge2 = Vec3::new(v2.x() - v0.x(), v2.y() - v0.y(), v2.z() - v0.z());
            let face_normal = Vec3::new(
                edge1.y() * edge2.z() - edge1.z() * edge2.y(),
                edge1.z() * edge2.x() - edge1.x() * edge2.z(),
                edge1.x() * edge2.y() - edge1.y() * edge2.x(),
            )
            .normalize();

            normals[i] = face_normal;
            normals[i + 1] = face_normal;
            normals[i + 2] = face_normal;
        }

        return normals.iter().map(|n| n.to_array()).collect();
    }

    // Parse indices based on stride
    let get_index = |data: &[u8], stride: u8, i: usize| -> usize {
        match stride {
            1 => data[i] as usize,
            2 => {
                let arr = [data[i * 2], data[i * 2 + 1]];
                u16::from_le_bytes(arr) as usize
            }
            4 => {
                let arr = [
                    data[i * 4],
                    data[i * 4 + 1],
                    data[i * 4 + 2],
                    data[i * 4 + 3],
                ];
                u32::from_le_bytes(arr) as usize
            }
            _ => 0,
        }
    };

    let index_count = indices.len() / index_stride as usize;

    // Calculate face normals and accumulate per vertex
    for i in (0..index_count).step_by(3) {
        if i + 2 >= index_count {
            break;
        }

        let i0 = get_index(indices, index_stride, i);
        let i1 = get_index(indices, index_stride, i + 1);
        let i2 = get_index(indices, index_stride, i + 2);

        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            continue;
        }

        let v0 = Vec3::from(positions[i0]);
        let v1 = Vec3::from(positions[i1]);
        let v2 = Vec3::from(positions[i2]);

        // Calculate face normal using cross product
        let edge1 = Vec3::new(v1.x() - v0.x(), v1.y() - v0.y(), v1.z() - v0.z());
        let edge2 = Vec3::new(v2.x() - v0.x(), v2.y() - v0.y(), v2.z() - v0.z());
        let face_normal = Vec3::new(
            edge1.y() * edge2.z() - edge1.z() * edge2.y(),
            edge1.z() * edge2.x() - edge1.x() * edge2.z(),
            edge1.x() * edge2.y() - edge1.y() * edge2.x(),
        );

        // Accumulate face normal to each vertex
        normals[i0] = normals[i0] + face_normal;
        normals[i1] = normals[i1] + face_normal;
        normals[i2] = normals[i2] + face_normal;

        counts[i0] += 1;
        counts[i1] += 1;
        counts[i2] += 1;
    }

    // Normalize accumulated normals
    for i in 0..normals.len() {
        if counts[i] > 0 {
            normals[i] = normals[i] * (1.0 / counts[i] as f32);
            normals[i] = normals[i].normalize();
        } else {
            // No triangles contribute - use up as default
            normals[i] = Vec3::new(0.0, 1.0, 0.0);
        }
    }

    // Convert back to arrays
    normals.iter().map(|n| n.to_array()).collect()
}

/// Build vertex data from position, normal, and tex coord arrays.
pub fn build_vertex_data(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    tex_coords: Vec<[f32; 2]>,
) -> (Vec<VertexPBR>, Sphere) {
    use itertools::izip;

    let has_pos = !positions.is_empty();
    let has_norm = !normals.is_empty();
    let has_tex_coords = !tex_coords.is_empty();

    let sphere = if has_pos {
        Sphere::create_from_verts(&positions)
    } else {
        Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0)
    };

    let vertex_data = if has_pos && has_norm && has_tex_coords {
        izip!(positions, normals, tex_coords)
            .map(|(position, normal, tex_coord)| VertexPBR {
                position,
                normal,
                tangent: [0.0, 0.0, 0.0, 0.0],
                tex_coord0: tex_coord,
            })
            .collect()
    } else if has_pos && has_norm {
        positions
            .into_iter()
            .zip(normals)
            .map(|(position, normal)| VertexPBR {
                position,
                normal,
                tangent: [0.0, 0.0, 0.0, 0.0],
                tex_coord0: [0.0, 0.0],
            })
            .collect()
    } else if has_pos && has_tex_coords {
        positions
            .into_iter()
            .zip(tex_coords)
            .map(|(position, tex_coord0)| VertexPBR {
                position,
                normal: [0.0, 0.0, 0.0],
                tangent: [0.0, 0.0, 0.0, 0.0],
                tex_coord0,
            })
            .collect()
    } else if has_pos {
        // NOTE: When normals are missing, we currently use position as a fallback.
        // For proper rendering, smooth normals should be auto-generated from triangle data.
        positions
            .into_iter()
            .map(|position| {
                let vert0 = Vec3::from(position);
                let norm0 = vert0.normalize();
                VertexPBR {
                    position,
                    normal: norm0.to_array(),
                    tangent: [0.0, 0.0, 0.0, 0.0],
                    tex_coord0: [0.0, 0.0],
                }
            })
            .collect()
    } else {
        vec![]
    };

    (vertex_data, sphere)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_vertex_data_complete() {
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
        let tex_coords = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];

        let (vertices, sphere) = build_vertex_data(positions, normals, tex_coords);

        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[0].position, [0.0, 0.0, 0.0]);
        assert_eq!(vertices[0].normal, [0.0, 0.0, 1.0]);
        assert_eq!(vertices[0].tex_coord0, [0.0, 0.0]);
        // Sphere should have a non-zero radius
        assert!(sphere.radius > 0.0);
    }

    #[test]
    fn test_build_vertex_data_positions_only() {
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = vec![];
        let tex_coords = vec![];

        let (vertices, sphere) = build_vertex_data(positions, normals, tex_coords);

        assert_eq!(vertices.len(), 3);
        // Normals should be normalized positions
        assert_eq!(vertices[0].position, [0.0, 0.0, 0.0]);
        assert!(sphere.radius > 0.0);
    }

    #[test]
    fn test_build_vertex_data_empty() {
        let positions = vec![];
        let normals = vec![];
        let tex_coords = vec![];

        let (vertices, sphere) = build_vertex_data(positions, normals, tex_coords);

        assert_eq!(vertices.len(), 0);
        assert_eq!(sphere.radius, 0.0);
    }
}
