use crate::rendering::vertextypes::*;
use crate::rendering::{Drawable, Material};
use crate::util::CachedGLTFModel;

use crate::vulkanstuff::VulkanRenderer;
use crate::vulkanstuff::{ImageInfo, RenderPipeline};
use crate::vulkanstuff::{IndexBuffer, Texture, VertexBuffer};

use erupt::{utils::allocator::Allocator, vk1_0::*, DeviceLoader};
use mikpe_math::{Mat4, Vec3};
use std::rc::Rc;

//TODO: Decouple pipeline from the "Mesh" struct,
//Ideally a Mesh would only contain the vertex data and a reference to a pipeline,
//either on its own or through a Model struct
pub struct Mesh {
    pub vertex_buffer: Option<VertexBuffer>,
    pub index_buffer: Option<IndexBuffer>,
    pub material: Material,
    pub num_verts: u32,
    pub position: Vec3,
}

impl Mesh {
    pub fn new_from_cache(
        model: Rc<CachedGLTFModel>,
        renderer: &mut VulkanRenderer,
        position: Vec3,
    ) -> Self {
        let material = Material::new(model.clone(), renderer);
        let mut mesh = Self {
            vertex_buffer: None,
            index_buffer: None,
            material,
            num_verts: 0,
            position,
        };
        mesh.vertex_buffer = Self::create_vertex_buffer(renderer, model.vertpbr());
        let index_type = match model.index_stride {
            1 => IndexType::UINT8_EXT,
            2 => IndexType::UINT16,
            4 => IndexType::UINT32,
            _ => IndexType::NONE_KHR,
        };
        mesh.index_buffer = Self::create_index_buffer(renderer, model.index_data(), index_type);
        mesh
    }

    fn create_index_buffer<DataType>(
        renderer: &mut VulkanRenderer,
        data: Vec<DataType>,
        index_type: IndexType,
    ) -> Option<IndexBuffer> {
        if data.is_empty() {
            None
        } else {
            let (device, allocator) = renderer.device_and_allocator();
            let data_slice = unsafe {
                std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    data.len() * std::mem::size_of::<DataType>(),
                )
            };
            let count = match index_type {
                IndexType::UINT8_EXT => data_slice.len() as u32,
                IndexType::UINT16 => (data_slice.len() as u32) / 2,
                IndexType::UINT32 => (data_slice.len() as u32) / 4,
                _ => 0 as u32,
            };
            let mut index_buffer = IndexBuffer::new(
                device,
                allocator,
                data_slice.len() as u64,
                index_type,
                count,
            );
            index_buffer.upload_data(device, data_slice);
            Some(index_buffer)
        }
    }

    fn create_vertex_buffer<DataType>(
        renderer: &mut VulkanRenderer,
        data: Vec<DataType>,
    ) -> Option<VertexBuffer> {
        if data.is_empty() {
            None
        } else {
            let (device, allocator) = renderer.device_and_allocator();
            let data_slice = unsafe {
                std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    data.len() * std::mem::size_of::<DataType>(),
                )
            };
            let mut vertex_buffer = VertexBuffer::new(
                device,
                allocator,
                data_slice.len() as u64,
                data.len() as u32,
            );
            vertex_buffer.upload_data(device, data_slice);
            Some(vertex_buffer)
        }
    }

    // pub fn new_from_data<VType: VertexBinding + Default, IType>(
    //     renderer: &mut VulkanRenderer,
    //     vertex_data: Vec<VType>,
    //     index_data: Vec<IType>,
    //     position: Vec3,
    // ) -> Self {
    //     let num_verts = vertex_data.len() as u32;
    //     let vertex_buffer = Self::create_vertex_buffer(renderer, vertex_data);
    //     // let index_type = IndexType::UINT32;
    //     let index_buffer = Self::create_index_buffer(renderer, index_data, IndexType::UINT32);

    //     let render_pass = renderer.render_pass;
    //     let surface_caps = renderer.surface_caps();
    //     let num_images = renderer.num_images();
    //     let (device, mut allocator) = renderer.device_and_allocator();

    //     let renderpipeline = RenderPipeline::new::<VType>(
    //         &device,
    //         &mut allocator,
    //         render_pass,
    //         surface_caps,
    //         num_images,
    //     );

    //     Self {
    //         vertex_buffer,
    //         index_buffer,
    //         texture: None,
    //         renderpipeline,
    //         num_verts,
    //         position,
    //     }
    // }

    pub fn add_draw_cmd(
        &self,
        device: &DeviceLoader,
        command_buffer: CommandBuffer,
        image_index: usize,
    ) {
        unsafe {
            self.material.bind(device, command_buffer);

            if let Some(index_buffer) = &self.index_buffer {
                device.cmd_bind_index_buffer(
                    command_buffer,
                    *index_buffer.object(),
                    0,
                    index_buffer.index_type,
                );
                if let Some(vertex_buffer) = &self.vertex_buffer {
                    device.cmd_bind_vertex_buffers(
                        command_buffer,
                        0,
                        &[*vertex_buffer.object()],
                        &[0],
                    );
                    device.cmd_draw_indexed(command_buffer, index_buffer.count(), 1, 0, 0, 0);
                }
            } else {
                if let Some(vertex_buffer) = &self.vertex_buffer {
                    device.cmd_bind_vertex_buffers(
                        command_buffer,
                        0,
                        &[*vertex_buffer.object()],
                        &[0],
                    );
                    device.cmd_draw(command_buffer, vertex_buffer.count(), 1, 0, 0);
                }
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
        let model = Mat4::from_translation(self.position.0);
        self.material
            .upload_pipeline_data(device, view, proj, model);
    }

    pub fn destroy(&mut self, device: &DeviceLoader, allocator: &mut Allocator) {
        self.material.destroy(device, allocator);
        if self.vertex_buffer.is_some() {
            println!("Destroying vertex buffer!");
            let buffer = self.vertex_buffer.take().unwrap();
            buffer.destroy(device, allocator);
        }
        if self.index_buffer.is_some() {
            println!("Destroying index buffer!");
            let buffer = self.index_buffer.take().unwrap();
            buffer.destroy(device, allocator);
        }
    }
}

impl Drawable for Mesh {
    fn draw(&self) {
        println!("Should draw mesh!");
    }
}
