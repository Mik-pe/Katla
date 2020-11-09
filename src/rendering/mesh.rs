use crate::rendering::{Drawable, Material};
use crate::{renderer::vulkan::VulkanContext, util::GLTFModel};

use crate::renderer::{IndexBuffer, VertexBuffer};

use ash::{version::DeviceV1_0, vk, Device};
use mikpe_math::{Mat4, Sphere, Vec3};
use std::{rc::Rc, sync::Arc};

//TODO: Decouple pipeline from the "Mesh" struct,
//Ideally a Mesh would only contain the vertex data and a reference to a pipeline,
//either on its own or through a Model struct
pub struct Mesh {
    pub vertex_buffer: Option<VertexBuffer>,
    pub index_buffer: Option<IndexBuffer>,
    pub material: Material,
    pub num_verts: u32,
    pub position: Vec3,
    pub bounds: Sphere,
}

impl Mesh {
    pub fn new_from_cache(
        model: Rc<GLTFModel>,
        context: Arc<VulkanContext>,
        render_pass: vk::RenderPass,
        num_images: usize,
        position: Vec3,
    ) -> Self {
        println!("Creating material");
        let material = Material::new(model.clone(), context.clone(), render_pass, num_images);
        let mut bound_sphere = model.bounds.clone();
        bound_sphere.center = position;

        let mut mesh = Self {
            vertex_buffer: None,
            index_buffer: None,
            material,
            num_verts: 0,
            position,
            bounds: bound_sphere,
        };
        println!("Creating vertex buffer");
        mesh.vertex_buffer = Self::create_vertex_buffer(&context, model.vertpbr());
        let index_type = match model.index_stride {
            1 => vk::IndexType::UINT8_EXT,
            2 => vk::IndexType::UINT16,
            4 => vk::IndexType::UINT32,
            _ => vk::IndexType::NONE_KHR,
        };
        println!("Creating index buffer");
        mesh.index_buffer = Self::create_index_buffer(&context, model.index_data(), index_type);
        mesh
    }

    fn create_index_buffer<DataType>(
        context: &Arc<VulkanContext>,
        data: Vec<DataType>,
        index_type: vk::IndexType,
    ) -> Option<IndexBuffer> {
        if data.is_empty() {
            None
        } else {
            let data_slice = unsafe {
                std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    data.len() * std::mem::size_of::<DataType>(),
                )
            };
            let count = match index_type {
                vk::IndexType::UINT8_EXT => data_slice.len() as u32,
                vk::IndexType::UINT16 => (data_slice.len() as u32) / 2,
                vk::IndexType::UINT32 => (data_slice.len() as u32) / 4,
                _ => 0 as u32,
            };
            let mut index_buffer =
                IndexBuffer::new(context.clone(), data_slice.len() as u64, index_type, count);
            index_buffer.upload_data(&context.device, data_slice);
            Some(index_buffer)
        }
    }

    fn create_vertex_buffer<DataType>(
        context: &Arc<VulkanContext>,
        data: Vec<DataType>,
    ) -> Option<VertexBuffer> {
        if data.is_empty() {
            None
        } else {
            let data_slice = unsafe {
                std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    data.len() * std::mem::size_of::<DataType>(),
                )
            };
            let mut vertex_buffer =
                VertexBuffer::new(context.clone(), data_slice.len() as u64, data.len() as u32);
            vertex_buffer.upload_data(&context.device, data_slice);
            Some(vertex_buffer)
        }
    }

    // pub fn destroy(&mut self, device: &Device, allocator: &mut Allocator) {
    //     self.material.destroy(device, allocator);
    // }

    pub fn draw(&self, device: &Device, command_buffer: vk::CommandBuffer) {
        unsafe {
            if let Some(index_buffer) = &self.index_buffer {
                device.cmd_bind_index_buffer(
                    command_buffer,
                    index_buffer.object().clone(),
                    0,
                    index_buffer.index_type,
                );
                if let Some(vertex_buffer) = &self.vertex_buffer {
                    device.cmd_bind_vertex_buffers(
                        command_buffer,
                        0,
                        &[vertex_buffer.object().clone()],
                        &[0],
                    );
                    device.cmd_draw_indexed(command_buffer, index_buffer.count(), 1, 0, 0, 0);
                }
            } else {
                if let Some(vertex_buffer) = &self.vertex_buffer {
                    device.cmd_bind_vertex_buffers(
                        command_buffer,
                        0,
                        &[vertex_buffer.object().clone()],
                        &[0],
                    );
                    device.cmd_draw(command_buffer, vertex_buffer.count(), 1, 0, 0);
                }
            }
        }
    }
}

impl Drawable for Mesh {
    fn update(&mut self, device: &Device, view: &Mat4, proj: &Mat4) {
        let model = Mat4::from_translation(self.position.0);
        self.material
            .upload_pipeline_data(device, view.clone(), proj.clone(), model);
    }

    fn draw(&self, device: &Device, command_buffer: vk::CommandBuffer) {
        self.material.bind(device, command_buffer);

        self.draw(device, command_buffer);
    }
}
