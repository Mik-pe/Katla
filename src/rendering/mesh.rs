use crate::util::GLTFModel;

use katla_vulkan::context::VulkanContext;
use katla_vulkan::{self, IndexBuffer, IndexType, VertexBuffer};

use std::{rc::Rc, sync::Arc};

//TODO:
// Handle the GPU-side in katla_vulkan
// Ideally a Mesh would only contain the vertex/index data
// Either own the data or, as now, the handles to the GPU data, any way works I guess
// A future Mesh could be split into a CPU/GPU part, for certain applications
pub struct Mesh {
    pub vertex_buffer: Option<VertexBuffer>,
    pub index_buffer: Option<IndexBuffer>,
    pub num_verts: u32,
}

impl Mesh {
    pub fn new_from_model(model: Rc<GLTFModel>, context: Arc<VulkanContext>) -> Self {
        let index_type = match model.index_stride {
            1 => IndexType::UINT8_EXT,
            2 => IndexType::UINT16,
            4 => IndexType::UINT32,
            _ => IndexType::NONE_KHR,
        };
        let index_buffer = Self::create_index_buffer(&context, model.index_data(), index_type);
        let vertex_buffer = Self::create_vertex_buffer(&context, model.vertpbr());

        Self {
            vertex_buffer,
            index_buffer,
            num_verts: 0,
        }
    }

    fn create_index_buffer<DataType>(
        context: &Arc<VulkanContext>,
        data: Vec<DataType>,
        index_type: IndexType,
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
                IndexType::UINT8_EXT => data_slice.len() as u32,
                IndexType::UINT16 => (data_slice.len() as u32) / 2,
                IndexType::UINT32 => (data_slice.len() as u32) / 4,
                _ => 0 as u32,
            };
            let mut index_buffer =
                IndexBuffer::new(context.clone(), data_slice.len() as u64, index_type, count);
            index_buffer.upload_data(data_slice);
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
            vertex_buffer.upload_data(data_slice);
            Some(vertex_buffer)
        }
    }

    pub fn draw(&self, command_buffer: &katla_vulkan::CommandBuffer) {
        if let Some(index_buffer) = &self.index_buffer {
            command_buffer.bind_index_buffer(index_buffer.object(), 0, index_buffer.index_type);

            if let Some(vertex_buffer) = &self.vertex_buffer {
                command_buffer.bind_vertex_buffers(0, &[vertex_buffer.object()], &[0]);
                command_buffer.draw_indexed(index_buffer.count(), 1, 0, 0, 0);
            }
        } else {
            if let Some(vertex_buffer) = &self.vertex_buffer {
                command_buffer.bind_vertex_buffers(0, &[vertex_buffer.object()], &[0]);
                command_buffer.draw_array(vertex_buffer.count(), 1, 0, 0);
            }
        }
    }
}
