use crate::rendering::VertexBuffer;
use crate::vulkanstuff::RenderPipeline;
use crate::vulkanstuff::VulkanRenderer;

use erupt::{utils::allocator::Allocator, vk1_0::*, DeviceLoader};
use mikpe_math::{Mat4, Vec3};

//TODO: Decouple pipeline from the "Mesh" struct
pub struct Mesh {
    pub vertex_buffer: Option<VertexBuffer>,
    pub renderpipeline: RenderPipeline,
    pub num_verts: u32,
    pub position: Vec3,
}

impl Mesh {
    pub fn new_from_data<T>(
        renderer: &mut VulkanRenderer,
        vertex_data: Vec<T>,
        position: Vec3,
    ) -> Self {
        let render_pass = renderer.render_pass;
        let surface_caps = renderer.surface_caps();
        let num_images = renderer.num_images();
        let (device, mut allocator) = renderer.device_and_allocator();
        let data_slice = unsafe {
            std::slice::from_raw_parts(
                vertex_data.as_ptr() as *const u8,
                vertex_data.len() * std::mem::size_of::<T>(),
            )
        };
        let mut vertex_buffer = VertexBuffer::new(device, allocator, data_slice.len() as u64);
        vertex_buffer.upload_data(device, data_slice);

        let renderpipeline = RenderPipeline::new(
            &device,
            &mut allocator,
            render_pass,
            surface_caps,
            num_images,
        );

        Self {
            vertex_buffer: Some(vertex_buffer),
            renderpipeline,
            num_verts: vertex_data.len() as u32,
            position,
        }
    }

    pub fn add_draw_cmd(
        &self,
        device: &DeviceLoader,
        command_buffer: CommandBuffer,
        image_index: usize,
    ) {
        if let Some(vertex_buffer) = &self.vertex_buffer {
            unsafe {
                device.cmd_bind_pipeline(
                    command_buffer,
                    PipelineBindPoint::GRAPHICS,
                    self.renderpipeline.pipeline,
                );
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    PipelineBindPoint::GRAPHICS,
                    self.renderpipeline.pipeline_layout,
                    0,
                    &[self.renderpipeline.uniform_descs[image_index as usize].desc_set],
                    &[],
                );
                device.cmd_bind_vertex_buffers(
                    command_buffer,
                    0,
                    &[*vertex_buffer.buffer.object()],
                    &[0],
                );
                device.cmd_draw(command_buffer, 3, 1, 0, 0);
            }
        }
    }

    pub fn upload_pipeline_data(
        &mut self,
        device: &DeviceLoader,
        image_index: usize,
        view: Mat4,
        proj: Mat4,
    ) {
        let mat = [
            Mat4::from_translation([self.position[0], self.position[1], self.position[2]]),
            view.clone(),
            proj.clone(),
        ];
        let data_slice = unsafe {
            std::slice::from_raw_parts(mat.as_ptr() as *const u8, std::mem::size_of_val(&mat))
        };
        self.renderpipeline.uniform_descs[image_index as usize].update_buffer(device, data_slice);
    }

    pub fn destroy(&mut self, device: &DeviceLoader, allocator: &mut Allocator) {
        self.renderpipeline.destroy(device, allocator);
        if self.vertex_buffer.is_some() {
            println!("Destroying buffer!");
            let buffer = self.vertex_buffer.take().unwrap();
            buffer.destroy(device, allocator);
        }
    }
}
// use crate::util::CachedGLTFModel;

// use gltf;
// use mikpe_math;
// use std::cmp::{Ordering, PartialOrd};
// use std::path::Path;
// enum IndexType {
//     UnsignedByte,
//     UnsignedShort,
//     UnsignedInt,
//     Array,
// }

// struct Vertex {
//     position: [f32; 4],
// }
// struct VertexNormal {
//     position: [f32; 4],
//     normal: [f32; 3],
// }

// struct VertexNormalTex {
//     position: [f32; 4],
//     normal: [f32; 3],
//     uv: [f32; 2],
// }

// struct VertexNormalTangentTex {
//     position: [f32; 4],
//     normal: [f32; 3],
//     tangent: [f32; 4],
//     uv: [f32; 2],
// }

// struct MeshBufferView {
//     stride: usize,
//     semantic: gltf::mesh::Semantic,
//     data: Vec<u8>,
// }

// impl MeshBufferView {
//     fn new(stride: usize, semantic: gltf::mesh::Semantic, data: Vec<u8>) -> Self {
//         Self {
//             stride,
//             semantic,
//             data,
//         }
//     }
// }

// impl PartialOrd for MeshBufferView {
//     fn partial_cmp(&self, other: &MeshBufferView) -> Option<Ordering> {
//         let sorted_key = |semantic: &gltf::mesh::Semantic| -> i32 {
//             match semantic {
//                 gltf::mesh::Semantic::Positions => 0,
//                 gltf::mesh::Semantic::Normals => 1,
//                 gltf::mesh::Semantic::Tangents => 2,
//                 gltf::mesh::Semantic::TexCoords(index) => 3 + *index as i32,
//                 _ => 14,
//             }
//         };
//         let sort_a = sorted_key(&self.semantic);
//         let sort_b = sorted_key(&other.semantic);
//         sort_a.partial_cmp(&sort_b)
//     }
// }

// impl PartialEq for MeshBufferView {
//     fn eq(&self, other: &MeshBufferView) -> bool {
//         let sorted_key = |semantic: &gltf::mesh::Semantic| -> i32 {
//             match semantic {
//                 gltf::mesh::Semantic::Positions => 0,
//                 gltf::mesh::Semantic::Normals => 1,
//                 gltf::mesh::Semantic::Tangents => 2,
//                 _ => 3,
//             }
//         };
//         let sort_a = sorted_key(&self.semantic);
//         let sort_b = sorted_key(&other.semantic);
//         sort_a.eq(&sort_b)
//     }
// }

// pub struct Mesh {
//     buffer: u32,
//     vao: u32,
//     num_triangles: u32,
//     index_type: IndexType,
//     pos: mikpe_math::Vec3,
//     scale: f32,
//     model_matrix: mikpe_math::Mat4,
//     vert_attr_offset: isize,
//     semantics: Vec<gltf::Semantic>,
//     material: Option<Material>,
// }

// impl Mesh {
//     pub fn new() -> Self {
//         Self {
//             buffer: 0,
//             vao: 0,
//             num_triangles: 0,
//             index_type: IndexType::UnsignedShort,
//             pos: mikpe_math::Vec3::new(0.0, 0.0, 0.0),
//             scale: 1.0,
//             model_matrix: mikpe_math::Mat4::new(),
//             vert_attr_offset: 0,
//             semantics: Vec::new(),
//             material: None,
//         }
//     }

//     pub fn read_gltf<P>(&mut self, path: P)
//     where
//         P: AsRef<Path>,
//     {
//         let (document, buffers, images) = gltf::import(path).unwrap();
//         self.parse_gltf(document, buffers, images);
//     }

//     pub fn init_from_cache(&mut self, model: CachedGLTFModel) {
//         self.parse_gltf(model.document, model.buffers, model.images);
//     }

//     pub fn parse_gltf(
//         &mut self,
//         document: gltf::Document,
//         buffers: Vec<gltf::buffer::Data>,
//         images: Vec<gltf::image::Data>,
//     ) {
//         let mut used_nodes = vec![];
//         for scene in document.scenes() {
//             for node in scene.nodes() {
//                 used_nodes.push(node.index());
//                 for child in node.children() {
//                     used_nodes.push(child.index());
//                 }
//             }
//         }
//         // let mut parsed_mats = vec![];
//         for material in document.materials() {
//             if self.material.is_none() {
//                 self.material = Some(Material::new(material, &images));
//             }
//             // parsed_mats.push(Material::new(material, &images));
//         }

//         for node in document.nodes() {
//             if used_nodes.contains(&node.index()) {
//                 self.parse_node(&node, &buffers);
//             }
//         }
//     }

//     pub fn set_pos(&mut self, pos: mikpe_math::Vec3) {
//         self.pos = pos;
//         self.model_matrix = mikpe_math::Mat4::from_translation(self.pos.0);
//     }

//     pub fn rotate_z(&mut self, angle: f32) {
//         self.model_matrix = mikpe_math::Mat4::from_translation(self.pos.0);
//         let rotaxis = mikpe_math::Vec3::new(0.0, 1.0, 0.0);
//         let rot_mat = mikpe_math::Mat4::from_rotaxis(&angle, rotaxis.normalize().0);
//         let scale_mat = mikpe_math::Mat4([
//             mikpe_math::Vec4([self.scale, 0.0, 0.0, 0.0]),
//             mikpe_math::Vec4([0.0, self.scale, 0.0, 0.0]),
//             mikpe_math::Vec4([0.0, 0.0, self.scale, 0.0]),
//             mikpe_math::Vec4([0.0, 0.0, 0.0, 1.0]),
//         ]);
//         self.model_matrix = self.model_matrix.mul(&rot_mat).mul(&scale_mat);
//     }

// pub fn set_scale(&mut self, scale: f32) {
//     self.scale = scale;
// }

// pub unsafe fn update_model_matrix(&self, program: &crate::rendering::Program) {
//     program.uniform_mat("u_modelMatrix", &self.model_matrix);
// }

// fn upload_vertex_data(&mut self, vertices: &[u8], indices: &[u8]) {
//     let ind_len = match self.index_type {
//         IndexType::UnsignedByte => 1,
//         IndexType::UnsignedShort => 2,
//         IndexType::UnsignedInt => 4,
//         IndexType::Array => 0,
//     };
//     if ind_len != 0 {
//         self.num_triangles = (indices.len() / (ind_len * 3)) as u32;
//     }

//     self.vert_attr_offset = indices.len() as isize;
//     let total_buffer_size = vertices.len() + indices.len();
// unsafe {
//     gl::CreateBuffers(1, &mut self.buffer);

//     gl::NamedBufferStorage(
//         self.buffer,
//         total_buffer_size as isize,
//         std::ptr::null(),
//         gl::MAP_WRITE_BIT,
//     );
//     if !indices.is_empty() {
//         let buf = gl::MapNamedBufferRange(
//             self.buffer,
//             0,
//             indices.len() as isize,
//             gl::MAP_WRITE_BIT | gl::MAP_FLUSH_EXPLICIT_BIT,
//         );
//         if !buf.is_null() {
//             std::ptr::copy(indices.as_ptr(), buf as *mut _, indices.len());
//             gl::FlushMappedNamedBufferRange(self.buffer, 0, indices.len() as isize);
//             gl::UnmapNamedBuffer(self.buffer);
//         }
//     }
//     let buf = gl::MapNamedBufferRange(
//         self.buffer,
//         self.vert_attr_offset,
//         vertices.len() as isize,
//         gl::MAP_WRITE_BIT | gl::MAP_FLUSH_EXPLICIT_BIT,
//     );
//     if !buf.is_null() {
//         std::ptr::copy(vertices.as_ptr(), buf as *mut _, vertices.len());
//         gl::FlushMappedNamedBufferRange(self.buffer, 0, vertices.len() as isize);
//         gl::UnmapNamedBuffer(self.buffer);
//     }
// }
// }

//     fn parse_node(&mut self, node: &gltf::Node, buffers: &Vec<gltf::buffer::Data>) {
//         let mut vert_vec: Vec<u8> = Vec::new();
//         if let Some(mesh) = node.mesh() {
//             // println!("Found mesh {:?} in node!", mesh.name());
//             let dummy_index_arr = [];
//             let mut index_arr: &[u8] = &dummy_index_arr;
//             let mut mesh_bufferview_vec: Vec<MeshBufferView> = vec![];
//             for primitive in mesh.primitives() {
//                 let mut start_index: usize;
//                 let mut end_index: usize;
//                 let mut num_vertices = 0;
//                 //TODO: Upload entire buffer and sample from it as the accessor tells us:
//                 let num_attributes = primitive.attributes().len();
//                 for (semantic, accessor) in primitive.attributes() {
//                     //Striding needs to be acknowledged
//                     match semantic {
//                         gltf::mesh::Semantic::Positions => {
//                             self.semantics.push(gltf::Semantic::Positions)
//                         }
//                         gltf::mesh::Semantic::Normals => {
//                             self.semantics.push(gltf::Semantic::Normals)
//                         }
//                         gltf::mesh::Semantic::Tangents => {
//                             self.semantics.push(gltf::Semantic::Tangents)
//                         }
//                         gltf::mesh::Semantic::TexCoords(index) => {
//                             self.semantics.push(gltf::Semantic::TexCoords(index))
//                         }
//                         _ => {
//                             continue;
//                         }
//                     }
//                     let buffer_view = accessor.view().unwrap();
//                     let acc_total_size = accessor.size() * accessor.count();
//                     num_vertices = accessor.count();
//                     let acc_stride = accessor.size();
//                     let buf_index = buffer_view.buffer().index();
//                     let buf_stride = buffer_view.stride();
//                     let mut interleaving_step = num_attributes;
//                     if buf_stride.is_none() || buf_stride.unwrap() == acc_stride {
//                         interleaving_step = 1;
//                         end_index = acc_total_size;
//                     } else {
//                         end_index = buffer_view.length();
//                     }
//                     start_index = accessor.offset() + buffer_view.offset();
//                     end_index += start_index;
//                     let attr_buf = &buffers[buf_index];
//                     let attr_arr = &attr_buf[start_index..end_index];

//                     let noninterleaved_arr = attr_arr
//                         .to_vec()
//                         .chunks(acc_stride)
//                         .step_by(interleaving_step)
//                         .flatten()
//                         .copied()
//                         .collect::<Vec<u8>>();

//                     mesh_bufferview_vec.push(MeshBufferView::new(
//                         acc_stride,
//                         semantic,
//                         noninterleaved_arr,
//                     ));
//                 }

//                 if let Some(indices) = primitive.indices() {
//                     let ind_view = indices.view().unwrap();
//                     let ind_offset = ind_view.offset();
//                     let ind_size = ind_view.length();
//                     let acc_size = indices.size();
//                     if acc_size == 1 {
//                         self.index_type = IndexType::UnsignedByte;
//                     } else if acc_size == 2 {
//                         self.index_type = IndexType::UnsignedShort;
//                     } else if acc_size == 4 {
//                         self.index_type = IndexType::UnsignedInt;
//                     } else {
//                         panic!("Cannot parse this node");
//                     }
//                     let buf_index = ind_view.buffer().index();
//                     let ind_buf = &buffers[buf_index];
//                     index_arr = &ind_buf[ind_offset..ind_offset + ind_size];
//                 } else {
//                     self.index_type = IndexType::Array;
//                     self.num_triangles = num_vertices as u32;
//                 }
//             }
//             mesh_bufferview_vec.sort_by(|a, b| a.partial_cmp(b).unwrap());
//             let mut current_stride = 0;
//             for bufferview in mesh_bufferview_vec {
//                 if current_stride == 0 {
//                     //TODO: This does not work with interleaved data!
//                     vert_vec = bufferview.data;
//                 } else {
//                     vert_vec = vert_vec
//                         .chunks(current_stride)
//                         .zip(bufferview.data[..].chunks(bufferview.stride))
//                         .flat_map(|(a, b)| a.into_iter().chain(b))
//                         .copied()
//                         .collect::<Vec<u8>>();
//                 }
//                 current_stride += bufferview.stride;
//             }
//             self.upload_vertex_data(&vert_vec[..], index_arr);
//         }
//     }
// }
// impl Drop for Mesh {
//     fn drop(&mut self) {
//         unsafe {
//             println!("Deleted mesh!");
//             gl::DeleteBuffers(1, &self.buffer);
//             if self.vao != 0 {
//                 gl::DeleteVertexArrays(1, &self.vao);
//             }
//         }
//     }
// }

// impl Drawable for Mesh {
//     fn draw(&self) {
// unsafe {
//     if let Some(mat) = &self.material {
//         mat.bind();
//     }
//     gl::BindVertexArray(self.vao);
//     match self.index_type {
//         IndexType::UnsignedByte => {
//             gl::DrawElements(
//                 gl::TRIANGLES,
//                 (self.num_triangles * 3) as i32,
//                 gl::UNSIGNED_BYTE,
//                 std::ptr::null(),
//             );
//         }
//         IndexType::UnsignedShort => {
//             gl::DrawElements(
//                 gl::TRIANGLES,
//                 (self.num_triangles * 3) as i32,
//                 gl::UNSIGNED_SHORT,
//                 std::ptr::null(),
//             );
//         }
//         IndexType::UnsignedInt => {
//             gl::DrawElements(
//                 gl::TRIANGLES,
//                 (self.num_triangles * 3) as i32,
//                 gl::UNSIGNED_INT,
//                 std::ptr::null(),
//             );
//         }
//         IndexType::Array => {
//             gl::DrawArrays(gl::TRIANGLES, 0, (self.num_triangles) as i32);
//         }
//     }
//     if let Some(mat) = &self.material {
//         mat.unbind();
//     }
// }
// }

// unsafe fn rebind_gl(mut self) -> Self {
// gl::CreateVertexArrays(1, &mut self.vao);
// gl::VertexArrayElementBuffer(self.vao, self.buffer);

// //TODO: These can be fetched from semantics:
// let mut stride = 0u32;
// if self.semantics.contains(&gltf::Semantic::Positions) {
//     gl::EnableVertexArrayAttrib(self.vao, 0);
//     gl::VertexArrayAttribFormat(self.vao, 0, 3, gl::FLOAT, gl::FALSE, stride);
//     gl::VertexArrayAttribBinding(self.vao, 0, 0);
//     stride += 12;
// }
// if self.semantics.contains(&gltf::Semantic::Normals) {
//     gl::EnableVertexArrayAttrib(self.vao, 1);
//     gl::VertexArrayAttribFormat(self.vao, 1, 3, gl::FLOAT, gl::FALSE, stride);
//     gl::VertexArrayAttribBinding(self.vao, 1, 0);
//     stride += 12;
// }
// if self.semantics.contains(&gltf::Semantic::Tangents) {
//     gl::EnableVertexArrayAttrib(self.vao, 2);
//     gl::VertexArrayAttribFormat(self.vao, 2, 4, gl::FLOAT, gl::FALSE, stride);
//     gl::VertexArrayAttribBinding(self.vao, 2, 0);
//     stride += 16;
// }
// if self.semantics.contains(&gltf::Semantic::TexCoords(0)) {
//     gl::EnableVertexArrayAttrib(self.vao, 3);
//     gl::VertexArrayAttribFormat(self.vao, 3, 2, gl::FLOAT, gl::FALSE, stride);
//     gl::VertexArrayAttribBinding(self.vao, 3, 0);
//     stride += 8;
// }
// gl::VertexArrayVertexBuffer(
//     self.vao,
//     0,
//     self.buffer,
//     self.vert_attr_offset,
//     stride as i32,
// );
//         self
//     }
// }
