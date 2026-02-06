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
        let view = accessor.view().expect("Index accessor must have a buffer view");
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
                let vert0 = Vec3(position);
                let norm0 = vert0.normalize();
                VertexPBR {
                    position,
                    normal: norm0.0,
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
        let positions = vec
![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
;
        let normals = vec
![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]]
;
        let tex_coords = vec
![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]
;

        let (vertices, sphere) = build_vertex_data(positions, normals, tex_coords);

        assert_eq!(vertices.len()
, 3);
        assert_eq!(vertices[0].position, [0.0, 0.0, 0.0]);
        assert_eq!(vertices[0].normal, [0.0, 0.0, 1.0]);
        assert_eq!(vertices[0].tex_coord0, [0.0, 0.0]);
        // Sphere should have a non-zero radius
        assert!(sphere.radius > 0.0);
    }

    #[test]
    fn test_build_vertex_data_positions_only() {
        let positions = vec
![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
;
        let normals = vec![];
        let tex_coords = vec![];

        let (vertices, sphere) = build_vertex_data(positions, normals, tex_coords);

        assert_eq!(vertices.len()
, 3);
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
