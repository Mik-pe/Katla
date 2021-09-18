use std::path::{Path, PathBuf};

use byteorder::{ByteOrder, LittleEndian};

use gltf::buffer::Data as BufferData;
use gltf::image::Data as ImageData;
use gltf::Document;
use itertools::izip;
use katla_math::{Sphere, Vec3};

use crate::rendering::{VertexNormal, VertexPBR, VertexPosition};

#[derive(Clone)]
pub struct GLTFModel {
    pub document: Document,
    pub buffers: Vec<BufferData>,
    pub images: Vec<ImageData>,
    pub vertex_data: Vec<VertexPBR>,
    pub index_data: Vec<u8>,
    pub index_stride: u8,
    pub bounds: Sphere,
}

impl GLTFModel {
    fn parse_node(&self, node: &gltf::Node) -> (Vec<VertexPBR>, Vec<u8>, u8, Sphere) {
        let mut positions: Vec<[f32; 3]> = vec![];
        let mut normals: Vec<[f32; 3]> = vec![];
        let mut tex_coords: Vec<[f32; 2]> = vec![];
        let mut index_stride = 0u8;
        let mut index_data = vec![];
        let mut vertex_data = vec![];
        let mut sphere = Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0);
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                let mut start_index: usize;
                let mut end_index: usize;
                //TODO: Upload entire buffer and sample from it as the accessor tells us:
                let num_attributes = primitive.attributes().len();

                for (semantic, accessor) in primitive.attributes() {
                    let buffer_view = accessor.view().unwrap();
                    let acc_total_size = accessor.size() * accessor.count();
                    let acc_stride = accessor.size();
                    let buf_index = buffer_view.buffer().index();
                    let buf_stride = buffer_view.stride();
                    let mut interleaving_step = num_attributes;
                    if buf_stride.is_none() || buf_stride.unwrap() == acc_stride {
                        interleaving_step = 1;
                        end_index = acc_total_size;
                    } else {
                        end_index = buffer_view.length();
                    }
                    start_index = accessor.offset() + buffer_view.offset();
                    end_index += start_index;
                    let attr_buf = &self.buffers[buf_index];
                    let attr_arr = (&attr_buf[start_index..end_index]).to_vec();
                    let iter = attr_arr.chunks(acc_stride).step_by(interleaving_step);
                    //Striding needs to be acknowledged
                    match semantic {
                        gltf::mesh::Semantic::Positions => {
                            positions = iter
                                .map(|bytes| {
                                    [
                                        LittleEndian::read_f32(&bytes[0..4]),
                                        LittleEndian::read_f32(&bytes[4..8]),
                                        LittleEndian::read_f32(&bytes[8..12]),
                                    ]
                                })
                                .collect::<Vec<[f32; 3]>>();
                        }
                        gltf::mesh::Semantic::Normals => {
                            normals = iter
                                .map(|bytes| {
                                    [
                                        LittleEndian::read_f32(&bytes[0..4]),
                                        LittleEndian::read_f32(&bytes[4..8]),
                                        LittleEndian::read_f32(&bytes[8..12]),
                                    ]
                                })
                                .collect::<Vec<[f32; 3]>>();
                        }
                        gltf::mesh::Semantic::TexCoords(0) => {
                            tex_coords = iter
                                .map(|bytes| {
                                    [
                                        LittleEndian::read_f32(&bytes[0..4]),
                                        LittleEndian::read_f32(&bytes[4..8]),
                                    ]
                                })
                                .collect::<Vec<[f32; 2]>>();
                        }
                        _ => {
                            continue;
                        }
                    }
                }

                if let Some(indices) = primitive.indices() {
                    let ind_view = indices.view().unwrap();
                    let ind_offset = ind_view.offset();
                    let ind_size = ind_view.length();
                    let acc_size = indices.size();
                    index_stride = acc_size as u8;
                    let buf_index = ind_view.buffer().index();
                    let ind_buf = &self.buffers[buf_index];
                    index_data = ind_buf[ind_offset..ind_offset + ind_size].to_vec();
                }
            }
            let has_pos = !positions.is_empty();
            let has_norm = !normals.is_empty();
            let has_tex_coords = !tex_coords.is_empty();

            if has_pos {
                sphere = Sphere::create_from_verts(&positions);
            }

            if has_pos && has_norm && has_tex_coords {
                vertex_data = izip!(positions, normals, tex_coords)
                    .map(|(position, normal, tex_coord)| VertexPBR {
                        position,
                        normal,
                        tangent: [0.0, 0.0, 0.0, 0.0],
                        tex_coord0: tex_coord,
                    })
                    .collect::<Vec<VertexPBR>>();
            } else if has_pos && has_norm {
                vertex_data = positions
                    .into_iter()
                    .zip(normals.into_iter())
                    .map(|(position, normal)| VertexPBR {
                        position,
                        normal,
                        tangent: [0.0, 0.0, 0.0, 0.0],
                        tex_coord0: [0.0, 0.0],
                    })
                    .collect::<Vec<VertexPBR>>();
            } else if has_pos && has_tex_coords {
                vertex_data = positions
                    .into_iter()
                    .zip(tex_coords.into_iter())
                    .map(|(position, tex_coord0)| VertexPBR {
                        position,
                        normal: [0.0, 0.0, 0.0],
                        tangent: [0.0, 0.0, 0.0, 0.0],
                        tex_coord0,
                    })
                    .collect::<Vec<VertexPBR>>();
            } else if has_pos {
                //TODO: Auto-gen normals smoothly with triangle-data:
                vertex_data = positions
                    .into_iter()
                    .map(|position| {
                        let vert0 = katla_math::Vec3(position);
                        let norm0 = vert0.normalize();
                        VertexPBR {
                            position,
                            normal: norm0.0,
                            tangent: [0.0, 0.0, 0.0, 0.0],
                            tex_coord0: [0.0, 0.0],
                        }
                    })
                    .collect::<Vec<VertexPBR>>();
            }
        }
        (vertex_data, index_data, index_stride, sphere)
    }

    fn parse_gltf(&mut self) {
        let mut used_nodes = vec![];
        for scene in self.document.scenes() {
            for node in scene.nodes() {
                used_nodes.push(node.index());
                for child in node.children() {
                    used_nodes.push(child.index());
                }
            }
        }
        // let mut parsed_mats = vec![];

        for node in self.document.nodes() {
            if used_nodes.contains(&node.index()) {
                let (vertex_data, index_data, index_stride, sphere) = self.parse_node(&node);
                self.vertex_data.extend(vertex_data);
                self.index_data.extend(index_data);
                self.index_stride = index_stride;
                self.bounds = sphere;
            }
        }
        println!(
            "Bound center: {:?}, Bound radius: {}",
            self.bounds.center, self.bounds.radius
        );
    }

    fn new<P>(path: P) -> Self
    where
        P: AsRef<Path>,
    {
        let (document, buffers, images) = gltf::import(path).unwrap();

        let mut model = Self {
            document,
            buffers,
            images,
            vertex_data: vec![],
            index_data: vec![],
            index_stride: 0,
            bounds: Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0),
        };
        model.parse_gltf();
        model
    }

    //FIXME: This is only really valid for one node in the structure!
    pub fn vertpos(&self) -> Vec<VertexPosition> {
        self.vertex_data
            .iter()
            .map(|x| VertexPosition {
                position: x.position,
            })
            .collect::<Vec<VertexPosition>>()
    }

    pub fn vertposnorm(&self) -> Vec<VertexNormal> {
        self.vertex_data
            .iter()
            .map(|x| VertexNormal {
                position: x.position,
                normal: x.normal,
            })
            .collect::<Vec<VertexNormal>>()
    }

    pub fn vertpbr(&self) -> Vec<VertexPBR> {
        self.vertex_data.clone()
    }

    pub fn index_data(&self) -> Vec<u8> {
        self.index_data.clone()
    }
}

impl From<PathBuf> for GLTFModel {
    fn from(pathbuf: PathBuf) -> Self {
        GLTFModel::new(pathbuf.as_path())
    }
}
