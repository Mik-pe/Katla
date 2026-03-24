//! GLTF buffer parsing utilities.
//!
//! This module provides utilities for parsing GLTF buffer data into vertex and index formats.

use byteorder::{ByteOrder, LittleEndian};
use gltf::buffer::Data as BufferData;
use katla_math::{Mat4, Quat, Sphere, Vec3, Vec4};

use katla_gfx::{VertexPBR, VertexPBRSkinned};

/// GLTF attribute parser using accessor iterators.
pub struct AttributeParser<'a> {
    buffers: &'a [BufferData],
}

impl<'a> AttributeParser<'a> {
    pub fn new(buffers: &'a [BufferData]) -> Self {
        Self { buffers }
    }

    /// Parse scalar f32 values from an accessor.
    /// Used for animation keyframe times and other scalar data.
    pub fn parse_scalars(&self, accessor: gltf::Accessor<'a>) -> Vec<f32> {
        accessor
            .view()
            .and_then(|view| self.parse_scalar_accessor(accessor, view))
            .unwrap_or_default()
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

    /// Parse joint indices from an accessor (JOINTS_0).
    ///
    /// GLTF stores joint indices as VEC4 of u8 or u16.
    /// We normalize to u16 for shader compatibility.
    pub fn parse_joint_indices(&self, accessor: gltf::Accessor<'a>) -> Vec<[u16; 4]> {
        accessor
            .view()
            .and_then(|view| self.parse_joints_accessor(accessor, view))
            .unwrap_or_default()
    }

    /// Parse joint weights from an accessor (WEIGHTS_0).
    pub fn parse_joint_weights(&self, accessor: gltf::Accessor<'a>) -> Vec<[f32; 4]> {
        accessor
            .view()
            .and_then(|view| self.parse_weights_accessor(accessor, view))
            .unwrap_or_default()
    }

    /// Parse tangent data from an accessor.
    /// GLTF stores tangents as VEC4 of F32 (xyz + handedness w).
    pub fn parse_tangents(&self, accessor: gltf::Accessor<'a>) -> Vec<[f32; 4]> {
        accessor
            .view()
            .and_then(|view| self.parse_vec4_accessor(accessor, view))
            .unwrap_or_default()
    }

    /// Parse index data from an accessor.
    ///
    /// Returns None if the accessor has no buffer view (invalid GLTF).
    pub fn parse_indices(&self, accessor: gltf::Accessor<'a>) -> Option<(Vec<u8>, u8)> {
        let view = accessor.view()?;
        let buf_index = view.buffer().index();
        let ind_offset = view.offset() + accessor.offset();
        let ind_size = view.length();
        let ind_buf = &self.buffers[buf_index];
        let index_data = ind_buf[ind_offset..ind_offset + ind_size].to_vec();
        let index_stride = accessor.size() as u8;
        Some((index_data, index_stride))
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

    /// Helper to parse Vec4 data from an accessor with its view.
    /// Used for tangents (xyz + handedness w).
    fn parse_vec4_accessor(
        &self,
        accessor: gltf::Accessor<'a>,
        view: gltf::buffer::View<'a>,
    ) -> Option<Vec<[f32; 4]>> {
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
                            LittleEndian::read_f32(&bytes[12..16]),
                        ]
                    })
                    .collect(),
            )
        } else {
            // Unsupported data type
            None
        }
    }

    /// Helper to parse joint indices from an accessor with its view.
    ///
    /// GLTF stores joints as VEC4 of u8 (most common) or u16.
    fn parse_joints_accessor(
        &self,
        accessor: gltf::Accessor<'a>,
        view: gltf::buffer::View<'a>,
    ) -> Option<Vec<[u16; 4]>> {
        let buf_index = view.buffer().index();
        let buf_stride = view.stride();
        let attr_buf = &self.buffers[buf_index];

        let start_index = accessor.offset() + view.offset();
        let stride = buf_stride.unwrap_or(accessor.size());
        let total_size = accessor.size() * accessor.count();
        let end_index = start_index + total_size;

        let attr_arr = &attr_buf[start_index..end_index];

        // GLTF uses U8 or U16 for joint indices
        match accessor.data_type() {
            gltf::accessor::DataType::U8 => Some(
                attr_arr
                    .chunks(stride)
                    .map(|bytes| {
                        [
                            bytes[0] as u16,
                            bytes[1] as u16,
                            bytes[2] as u16,
                            bytes[3] as u16,
                        ]
                    })
                    .collect(),
            ),
            gltf::accessor::DataType::U16 => Some(
                attr_arr
                    .chunks(stride)
                    .map(|bytes| {
                        [
                            LittleEndian::read_u16(&bytes[0..2]),
                            LittleEndian::read_u16(&bytes[2..4]),
                            LittleEndian::read_u16(&bytes[4..6]),
                            LittleEndian::read_u16(&bytes[6..8]),
                        ]
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Helper to parse joint weights from an accessor with its view.
    fn parse_weights_accessor(
        &self,
        accessor: gltf::Accessor<'a>,
        view: gltf::buffer::View<'a>,
    ) -> Option<Vec<[f32; 4]>> {
        let buf_index = view.buffer().index();
        let buf_stride = view.stride();
        let attr_buf = &self.buffers[buf_index];

        let start_index = accessor.offset() + view.offset();
        let stride = buf_stride.unwrap_or(accessor.size());
        let total_size = accessor.size() * accessor.count();
        let end_index = start_index + total_size;

        let attr_arr = &attr_buf[start_index..end_index];

        // GLTF uses F32 for weights
        if accessor.data_type() == gltf::accessor::DataType::F32 {
            Some(
                attr_arr
                    .chunks(stride)
                    .map(|bytes| {
                        [
                            LittleEndian::read_f32(&bytes[0..4]),
                            LittleEndian::read_f32(&bytes[4..8]),
                            LittleEndian::read_f32(&bytes[8..12]),
                            LittleEndian::read_f32(&bytes[12..16]),
                        ]
                    })
                    .collect(),
            )
        } else {
            None
        }
    }

    /// Helper to parse scalar f32 values from an accessor with its view.
    /// Used for animation keyframe times.
    fn parse_scalar_accessor(
        &self,
        accessor: gltf::Accessor<'a>,
        view: gltf::buffer::View<'a>,
    ) -> Option<Vec<f32>> {
        let buf_index = view.buffer().index();
        let buf_stride = view.stride();
        let attr_buf = &self.buffers[buf_index];

        let start_index = accessor.offset() + view.offset();
        let stride = buf_stride.unwrap_or(accessor.size());
        let total_size = accessor.size() * accessor.count();
        let end_index = start_index + total_size;

        let attr_arr = &attr_buf[start_index..end_index];

        if accessor.data_type() == gltf::accessor::DataType::F32 {
            Some(
                attr_arr
                    .chunks(stride)
                    .map(|bytes| LittleEndian::read_f32(&bytes[0..4]))
                    .collect(),
            )
        } else {
            None
        }
    }

    /// Parse 4x4 matrices from an accessor.
    /// Used for inverse bind matrices in skinning.
    pub fn parse_matrices(&self, accessor: gltf::Accessor<'a>) -> Vec<Mat4> {
        accessor
            .view()
            .and_then(|view| self.parse_mat4_accessor(accessor, view))
            .unwrap_or_default()
    }

    /// Helper to parse Mat4 data from an accessor with its view.
    fn parse_mat4_accessor(
        &self,
        accessor: gltf::Accessor<'a>,
        view: gltf::buffer::View<'a>,
    ) -> Option<Vec<Mat4>> {
        let buf_index = view.buffer().index();
        let buf_stride = view.stride();
        let attr_buf = &self.buffers[buf_index];

        let start_index = accessor.offset() + view.offset();
        let stride = buf_stride.unwrap_or(accessor.size());
        let total_size = accessor.size() * accessor.count();
        let end_index = start_index + total_size;

        let attr_arr = &attr_buf[start_index..end_index];

        // Mat4 is 16 x F32 = 64 bytes
        if accessor.data_type() == gltf::accessor::DataType::F32
            && accessor.dimensions() == gltf::accessor::Dimensions::Mat4
        {
            Some(
                attr_arr
                    .chunks(stride)
                    .map(|bytes| {
                        // GLTF stores matrices in column-major order as 16 consecutive floats.
                        // Our Mat4 is also column-major: Mat4.0[i] = i-th column as Vec4.
                        // So we read directly without transposing.
                        Mat4([
                            Vec4::new(
                                LittleEndian::read_f32(&bytes[0..4]),
                                LittleEndian::read_f32(&bytes[4..8]),
                                LittleEndian::read_f32(&bytes[8..12]),
                                LittleEndian::read_f32(&bytes[12..16]),
                            ),
                            Vec4::new(
                                LittleEndian::read_f32(&bytes[16..20]),
                                LittleEndian::read_f32(&bytes[20..24]),
                                LittleEndian::read_f32(&bytes[24..28]),
                                LittleEndian::read_f32(&bytes[28..32]),
                            ),
                            Vec4::new(
                                LittleEndian::read_f32(&bytes[32..36]),
                                LittleEndian::read_f32(&bytes[36..40]),
                                LittleEndian::read_f32(&bytes[40..44]),
                                LittleEndian::read_f32(&bytes[44..48]),
                            ),
                            Vec4::new(
                                LittleEndian::read_f32(&bytes[48..52]),
                                LittleEndian::read_f32(&bytes[52..56]),
                                LittleEndian::read_f32(&bytes[56..60]),
                                LittleEndian::read_f32(&bytes[60..64]),
                            ),
                        ])
                    })
                    .collect(),
            )
        } else {
            None
        }
    }

    /// Parse quaternion rotations from an accessor.
    /// Used for animation rotation keyframes.
    pub fn parse_quaternions(&self, accessor: gltf::Accessor<'a>) -> Vec<Quat> {
        accessor
            .view()
            .and_then(|view| self.parse_quat_accessor(accessor, view))
            .unwrap_or_default()
    }

    /// Helper to parse quaternion data from an accessor.
    fn parse_quat_accessor(
        &self,
        accessor: gltf::Accessor<'a>,
        view: gltf::buffer::View<'a>,
    ) -> Option<Vec<Quat>> {
        let vec4s = self.parse_vec4_accessor(accessor, view)?;
        // GLTF stores quaternions as [x, y, z, w], our Quat uses (x, y, z, w)
        Some(
            vec4s
                .into_iter()
                .map(|v| Quat::new(v[0], v[1], v[2], v[3]))
                .collect(),
        )
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
        normals[i0] += face_normal;
        normals[i1] += face_normal;
        normals[i2] += face_normal;

        counts[i0] += 1;
        counts[i1] += 1;
        counts[i2] += 1;
    }

    // Normalize accumulated normals
    for i in 0..normals.len() {
        if counts[i] > 0 {
            normals[i] *= 1.0 / counts[i] as f32;
            normals[i] = normals[i].normalize();
        } else {
            // No triangles contribute - use up as default
            normals[i] = Vec3::new(0.0, 1.0, 0.0);
        }
    }

    // Convert back to arrays
    normals.iter().map(|n| n.to_array()).collect()
}

/// Build vertex data from position, normal, tangent, and tex coord arrays.
pub fn build_vertex_data(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    tangents: Vec<[f32; 4]>,
    tex_coords: Vec<[f32; 2]>,
) -> (Vec<VertexPBR>, Sphere) {
    use itertools::izip;

    let has_pos = !positions.is_empty();
    let has_norm = !normals.is_empty();
    let has_tangents = !tangents.is_empty();
    let has_tex_coords = !tex_coords.is_empty();

    let sphere = if has_pos {
        Sphere::create_from_verts(&positions)
    } else {
        Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0)
    };

    // Default tangent if missing
    let default_tangent = [1.0, 0.0, 0.0, 1.0];

    let vertex_data = if has_pos && has_norm && has_tangents && has_tex_coords {
        izip!(positions, normals, tangents, tex_coords)
            .map(|(position, normal, tangent, tex_coord)| VertexPBR {
                position,
                normal,
                tangent,
                tex_coord0: tex_coord,
            })
            .collect()
    } else if has_pos && has_norm && has_tex_coords {
        // No tangents - use default
        izip!(positions, normals, tex_coords)
            .map(|(position, normal, tex_coord)| VertexPBR {
                position,
                normal,
                tangent: default_tangent,
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
                tangent: default_tangent,
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
                tangent: default_tangent,
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
                    tangent: default_tangent,
                    tex_coord0: [0.0, 0.0],
                }
            })
            .collect()
    } else {
        vec![]
    };

    (vertex_data, sphere)
}

/// Build skinned vertex data with joint indices and weights for skeletal animation.
///
/// Falls back to default skinning data (joint 0, weight 1.0) if skinning data is missing.
pub fn build_skinned_vertex_data(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    tex_coords: Vec<[f32; 2]>,
    joint_indices: Vec<[u16; 4]>,
    joint_weights: Vec<[f32; 4]>,
) -> (Vec<VertexPBRSkinned>, Sphere) {
    use itertools::izip;

    let has_pos = !positions.is_empty();
    let has_skinning = !joint_indices.is_empty() && !joint_weights.is_empty();

    let sphere = if has_pos {
        Sphere::create_from_verts(&positions)
    } else {
        Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0)
    };

    // Default skinning: all vertices bound to joint 0 with full weight
    let default_joints = [0u16, 0, 0, 0];
    let default_weights = [1.0f32, 0.0, 0.0, 0.0];
    // Default tangent: +X direction with positive handedness (for models without tangent data)
    let default_tangent = [1.0f32, 0.0, 0.0, 1.0];

    let vertex_count = positions.len();
    let vertex_data: Vec<VertexPBRSkinned> = if has_pos && has_skinning {
        izip!(positions, normals, tex_coords, joint_indices, joint_weights)
            .map(
                |(position, normal, tex_coord, joints, weights)| VertexPBRSkinned {
                    position,
                    normal,
                    tangent: default_tangent,
                    tex_coord0: tex_coord,
                    joint_indices: joints,
                    joint_weights: weights,
                },
            )
            .collect()
    } else if has_pos {
        // No skinning data - use defaults
        (0..vertex_count)
            .zip(positions)
            .zip(normals)
            .zip(tex_coords)
            .map(|(((i, position), normal), tex_coord)| {
                let joints = joint_indices.get(i).copied().unwrap_or(default_joints);
                let weights = joint_weights.get(i).copied().unwrap_or(default_weights);
                VertexPBRSkinned {
                    position,
                    normal,
                    tangent: default_tangent,
                    tex_coord0: tex_coord,
                    joint_indices: joints,
                    joint_weights: weights,
                }
            })
            .collect()
    } else {
        vec![]
    };

    (vertex_data, sphere)
}

/// Parsed attribute data in SoA (Structure of Arrays) format.
///
/// This struct holds separate arrays for each vertex attribute type,
/// enabling flexible rendering pipelines and efficient GPU memory access patterns.
#[derive(Clone)]
pub struct ParsedAttributes {
    /// Vertex positions (vec3<f32>)
    pub positions: Vec<[f32; 3]>,
    /// Vertex normals (vec3<f32>)
    pub normals: Vec<[f32; 3]>,
    /// Vertex tangents (vec4<f32>)
    pub tangents: Vec<[f32; 4]>,
    /// Primary texture coordinates (vec2<f32>)
    pub tex_coords0: Vec<[f32; 2]>,
    /// Joint indices for skeletal animation (uvec4, u16x4)
    pub joint_indices: Vec<[u16; 4]>,
    /// Joint weights for skeletal animation (vec4<f32>)
    pub joint_weights: Vec<[f32; 4]>,
    /// Bounding sphere computed from positions
    pub bounds: Sphere,
}

impl ParsedAttributes {
    /// Create ParsedAttributes from a GLTF primitive.
    ///
    /// This method parses all vertex attributes from a primitive
    /// and returns them in SoA format.
    ///
    /// # Arguments
    /// * `primitive` - GLTF primitive to parse
    /// * `parser` - AttributeParser for the GLTF buffers
    ///
    /// # Returns
    /// ParsedAttributes with all available vertex data
    pub fn from_gltf(primitive: &gltf::Primitive, parser: &AttributeParser) -> Self {
        let mut positions = vec![];
        let mut normals = vec![];
        let tangents = vec![];
        let mut tex_coords0 = vec![];
        let mut joint_indices = vec![];
        let mut joint_weights = vec![];

        // Parse all attributes
        for (semantic, accessor) in primitive.attributes() {
            match semantic {
                gltf::mesh::Semantic::Positions => {
                    positions = parser.parse_positions(accessor);
                    log::debug!("    Parsed {} positions", positions.len());
                }
                gltf::mesh::Semantic::Normals => {
                    normals = parser.parse_normals(accessor);
                    log::debug!("    Parsed {} normals", normals.len());
                }
                gltf::mesh::Semantic::Tangents => {
                    // Tangents are optional in GLTF
                    log::debug!("    Tangent attribute found but not parsed (not yet implemented)");
                }
                gltf::mesh::Semantic::TexCoords(0) => {
                    tex_coords0 = parser.parse_tex_coords(accessor);
                    log::debug!("    Parsed {} tex_coords", tex_coords0.len());
                }
                gltf::mesh::Semantic::Joints(0) => {
                    joint_indices = parser.parse_joint_indices(accessor);
                    log::debug!("    Parsed {} joint indices", joint_indices.len());
                }
                gltf::mesh::Semantic::Weights(0) => {
                    joint_weights = parser.parse_joint_weights(accessor);
                    log::debug!("    Parsed {} joint weights", joint_weights.len());
                }
                _ => {
                    // Ignore other semantics
                }
            }
        }

        // Compute bounding sphere from positions
        let bounds = if !positions.is_empty() {
            Sphere::create_from_verts(&positions)
        } else {
            Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0)
        };

        Self {
            positions,
            normals,
            tangents,
            tex_coords0,
            joint_indices,
            joint_weights,
            bounds,
        }
    }

    /// Get the vertex count.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Check if this has skeletal animation data.
    pub fn has_skinning(&self) -> bool {
        !self.joint_indices.is_empty() && !self.joint_weights.is_empty()
    }

    /// Check if this has specific attributes.
    pub fn has_attributes(&self, required: &[&str]) -> bool {
        required.iter().all(|&attr| match attr {
            "POSITION" => !self.positions.is_empty(),
            "NORMAL" => !self.normals.is_empty(),
            "TANGENT" => !self.tangents.is_empty(),
            "TEX_COORD_0" => !self.tex_coords0.is_empty(),
            "JOINTS_0" => !self.joint_indices.is_empty(),
            "WEIGHTS_0" => !self.joint_weights.is_empty(),
            _ => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_vertex_data_complete() {
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
        let tangents = vec![
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
        ];
        let tex_coords = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];

        let (vertices, sphere) = build_vertex_data(positions, normals, tangents, tex_coords);

        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[0].position, [0.0, 0.0, 0.0]);
        assert_eq!(vertices[0].normal, [0.0, 0.0, 1.0]);
        assert_eq!(vertices[0].tangent, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(vertices[0].tex_coord0, [0.0, 0.0]);
        // Sphere should have a non-zero radius
        assert!(sphere.radius > 0.0);
    }

    #[test]
    fn test_build_vertex_data_positions_only() {
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = vec![];
        let tangents = vec![];
        let tex_coords = vec![];

        let (vertices, sphere) = build_vertex_data(positions, normals, tangents, tex_coords);

        assert_eq!(vertices.len(), 3);
        // Normals should be normalized positions
        assert_eq!(vertices[0].position, [0.0, 0.0, 0.0]);
        assert!(sphere.radius > 0.0);
    }

    #[test]
    fn test_build_vertex_data_empty() {
        let positions = vec![];
        let normals = vec![];
        let tangents = vec![];
        let tex_coords = vec![];

        let (vertices, sphere) = build_vertex_data(positions, normals, tangents, tex_coords);

        assert_eq!(vertices.len(), 0);
        assert_eq!(sphere.radius, 0.0);
    }
}
