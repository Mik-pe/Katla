use crate::gl;
use crate::rendering::drawable::Drawable;
use gltf;
use mikpe_math;
use std::cmp::{Ordering, PartialOrd};
use std::path::Path;

enum IndexType {
    UnsignedByte,
    UnsignedShort,
    UnsignedInt,
}

struct MeshBufferView {
    stride: usize,
    semantic: gltf::mesh::Semantic,
    data: Vec<u8>,
}

impl MeshBufferView {
    fn new(stride: usize, semantic: gltf::mesh::Semantic, data: Vec<u8>) -> Self {
        Self {
            stride,
            semantic,
            data,
        }
    }
}

impl PartialOrd for MeshBufferView {
    fn partial_cmp(&self, other: &MeshBufferView) -> Option<Ordering> {
        let sorted_key = |semantic: &gltf::mesh::Semantic| -> i32 {
            match semantic {
                gltf::mesh::Semantic::Positions => 0,
                gltf::mesh::Semantic::Normals => 1,
                _ => 2,
            }
        };
        let sort_a = sorted_key(&self.semantic);
        let sort_b = sorted_key(&other.semantic);
        sort_a.partial_cmp(&sort_b)
    }
}

impl PartialEq for MeshBufferView {
    fn eq(&self, other: &MeshBufferView) -> bool {
        let sorted_key = |semantic: &gltf::mesh::Semantic| -> i32 {
            match semantic {
                gltf::mesh::Semantic::Positions => 0,
                gltf::mesh::Semantic::Normals => 1,
                _ => 2,
            }
        };
        let sort_a = sorted_key(&self.semantic);
        let sort_b = sorted_key(&other.semantic);
        sort_a.eq(&sort_b)
    }
}

pub struct Mesh {
    buffer: u32,
    vao: u32,
    num_triangles: u32,
    index_type: IndexType,
    pos: mikpe_math::Vec3,
    model_matrix: mikpe_math::Mat4,
    vert_attr_offset: isize,
}

impl Mesh {
    pub fn new() -> Self {
        Self {
            buffer: 0,
            vao: 0,
            num_triangles: 0,
            index_type: IndexType::UnsignedShort,
            pos: mikpe_math::Vec3::new(0.0, 0.0, 0.0),
            model_matrix: mikpe_math::Mat4::new(),
            vert_attr_offset: 0,
        }
    }

    pub fn read_gltf<P>(&mut self, path: P)
    where
        P: AsRef<Path>,
    {
        let (document, buffers, _images) = gltf::import(path).unwrap();
        let mut used_nodes = vec![];
        for scene in document.scenes() {
            // parse_scene();
            for node in scene.nodes() {
                println!("Scene #{} uses node #{}", scene.index(), node.index());
                used_nodes.push(node.index());
                for child in node.children() {
                    used_nodes.push(child.index());
                }
            }
        }
        for node in document.nodes() {
            if used_nodes.contains(&node.index()) {
                self.parse_node(&node, &buffers);
            }
        }
        for buffer_desc in document.buffers() {
            println!(
                "Buffer id {} has bytelen: {}",
                buffer_desc.index(),
                buffer_desc.length()
            );
            println!("Buffer index: {}", buffers[0].len());
        }
    }

    pub fn set_pos(&mut self, pos: mikpe_math::Vec3) {
        self.pos = pos;
    }

    pub fn rotate_z(&mut self, angle: f32) {
        self.model_matrix = mikpe_math::Mat4::from_translation(self.pos.0);
        let rotaxis = mikpe_math::Vec3::new(0.1, 0.75, 1.0);
        let rot_mat = mikpe_math::Mat4::from_rotaxis(&angle, rotaxis.normalize().0);
        self.model_matrix = self.model_matrix.mul(&rot_mat);
    }

    pub unsafe fn update_model_matrix(&self, program: &crate::rendering::Program) {
        program.uniform_mat(&"u_modelMatrix".to_owned(), &self.model_matrix);
    }

    fn upload_vertex_data(&mut self, vertices: &[u8], indices: &[u8]) {
        let ind_len = match self.index_type {
            IndexType::UnsignedByte => 1,
            IndexType::UnsignedShort => 2,
            IndexType::UnsignedInt => 4,
        };
        self.num_triangles = (indices.len() / (ind_len * 3)) as u32;

        self.vert_attr_offset = indices.len() as isize;
        let total_buffer_size = vertices.len() + indices.len();
        unsafe {
            gl::CreateBuffers(1, &mut self.buffer);
            gl::NamedBufferStorage(
                self.buffer,
                total_buffer_size as isize,
                std::ptr::null(),
                gl::DYNAMIC_STORAGE_BIT,
            );
            gl::NamedBufferSubData(
                self.buffer,
                0,
                indices.len() as isize,
                indices.as_ptr() as *const _,
            );
            gl::NamedBufferSubData(
                self.buffer,
                self.vert_attr_offset,
                vertices.len() as isize,
                vertices.as_ptr() as *const _,
            );
        }
    }

    fn parse_node(&mut self, node: &gltf::Node, buffers: &Vec<gltf::buffer::Data>) {
        // node.json.
        if let Some(nodename) = node.name() {
            println!("Got node: {}", nodename);
        }
        let mut vert_vec: Vec<u8> = Vec::new();
        if let Some(mesh) = node.mesh() {
            println!("Found mesh {:?} in node!", mesh.name());
            let mut index_arr: &[u8] = &[0u8];
            let mut mesh_bufferview_vec: Vec<MeshBufferView> = vec![];
            for primitive in mesh.primitives() {
                if let Some(indices) = primitive.indices() {
                    let ind_view = indices.view().unwrap();
                    let ind_offset = ind_view.offset();
                    let ind_size = ind_view.length();
                    let acc_size = indices.size();
                    if acc_size == 1 {
                        self.index_type = IndexType::UnsignedByte;
                    } else if acc_size == 2 {
                        self.index_type = IndexType::UnsignedShort;
                    } else if acc_size == 4 {
                        self.index_type = IndexType::UnsignedInt;
                    } else {
                        panic!("Cannot parse this node");
                    }
                    println!(
                        "Want an index buffer of stride: {}, with offset: {}, total bytelen: {}",
                        acc_size, ind_offset, ind_size
                    );
                    let buf_index = ind_view.buffer().index();
                    let ind_buf = &buffers[buf_index];
                    index_arr = &ind_buf[ind_offset..ind_offset + ind_size];
                }
                let mut start_index: usize;
                let mut end_index: usize;
                //TODO: Upload entire buffer and sample from it as the accessor tells us:
                let num_attributes = primitive.attributes().len();
                for attribute in primitive.attributes() {
                    //Striding needs to be acknowledged
                    let semantic = attribute.0;
                    let accessor = attribute.1;
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
                    let attr_buf = &buffers[buf_index];
                    let attr_arr = &attr_buf[start_index..end_index];

                    let noninterleaved_arr = attr_arr
                        .to_vec()
                        .chunks(acc_stride)
                        .step_by(interleaving_step)
                        .flatten()
                        .copied()
                        .collect::<Vec<u8>>();

                    mesh_bufferview_vec.push(MeshBufferView::new(
                        acc_stride,
                        semantic,
                        noninterleaved_arr,
                    ));
                }
            }
            mesh_bufferview_vec.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut current_stride = 0;
            for bufferview in mesh_bufferview_vec {
                if current_stride == 0 {
                    //TODO: This does not work with interleaved data!
                    vert_vec = bufferview.data;
                } else {
                    vert_vec = vert_vec
                        .chunks(current_stride)
                        .zip(bufferview.data[..].chunks(bufferview.stride))
                        .flat_map(|(a, b)| a.into_iter().chain(b))
                        .copied()
                        .collect::<Vec<u8>>();
                }
                current_stride += bufferview.stride;
            }
            self.upload_vertex_data(&vert_vec[..], index_arr);
        }
    }
}

impl Drawable for Mesh {
    fn draw(&self) {
        unsafe {
            gl::BindVertexArray(self.vao);
            match self.index_type {
                IndexType::UnsignedByte => {
                    gl::DrawElements(
                        gl::TRIANGLES,
                        (self.num_triangles * 3) as i32,
                        gl::UNSIGNED_BYTE,
                        std::ptr::null(),
                    );
                }
                IndexType::UnsignedShort => {
                    gl::DrawElements(
                        gl::TRIANGLES,
                        (self.num_triangles * 3) as i32,
                        gl::UNSIGNED_SHORT,
                        std::ptr::null(),
                    );
                }
                IndexType::UnsignedInt => {
                    gl::DrawElements(
                        gl::TRIANGLES,
                        (self.num_triangles * 3) as i32,
                        gl::UNSIGNED_INT,
                        std::ptr::null(),
                    );
                }
            }
        }
    }

    unsafe fn rebind_gl(mut self) -> Self {
        gl::CreateVertexArrays(1, &mut self.vao);
        gl::VertexArrayVertexBuffer(self.vao, 0, self.buffer, self.vert_attr_offset, 24);
        gl::VertexArrayElementBuffer(self.vao, self.buffer);

        //TODO: These can be fetched from semantics:
        gl::EnableVertexArrayAttrib(self.vao, 0);
        gl::VertexArrayAttribFormat(self.vao, 0, 3, gl::FLOAT, gl::FALSE, 0);
        gl::VertexArrayAttribBinding(self.vao, 0, 0);
        gl::EnableVertexArrayAttrib(self.vao, 1);
        gl::VertexArrayAttribFormat(self.vao, 1, 3, gl::FLOAT, gl::FALSE, 0);
        gl::VertexArrayAttribBinding(self.vao, 1, 0);
        self
    }
}
