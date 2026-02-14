pub mod builder;
pub mod cube;
pub mod cylinder;
pub mod plane;
pub mod sphere;
pub mod torus;

use crate::rendering::{VertexPBR, VertexSkinned};
use crate::util::GLTFModel;
pub use builder::*;
pub use cube::*;
pub use cylinder::*;
pub use plane::*;
pub use sphere::*;
pub use torus::*;

use katla_vulkan::context::VulkanContext;
use katla_vulkan::{self, IndexBuffer, IndexType, MeshHandle, VertexBuffer};

use std::rc::Rc;

/// Mesh represents geometry with GPU-side vertex and index buffers.
///
/// DESIGN NOTE: Currently `Mesh` owns GPU buffer handles directly. A future
/// improvement could split this into separate CPU/GPU representations, where
/// the CPU side holds the raw data and the GPU side manages Vulkan resources.
/// This would enable features like CPU-side mesh processing and dynamic updates.
pub struct Mesh {
    pub vertex_buffer: Option<VertexBuffer>,
    pub index_buffer: Option<IndexBuffer>,
    /// Handle after registration with renderer (None until registered)
    pub handle: Option<MeshHandle>,
}

impl Mesh {
    pub fn new(context: Rc<VulkanContext>, vertices: Vec<VertexPBR>, indices: Vec<u32>) -> Self {
        let index_buffer = Self::create_index_buffer(&context, indices, IndexType::Uint32);
        let vertex_buffer = Self::create_vertex_buffer(&context, vertices);

        Self {
            vertex_buffer,
            index_buffer,
            handle: None,
        }
    }

    pub fn new_from_model(model: Rc<GLTFModel>, context: Rc<VulkanContext>) -> Self {
        let index_type = match model.index_stride {
            1 => IndexType::Uint8,
            2 => IndexType::Uint16,
            4 => IndexType::Uint32,
            _ => IndexType::None,
        };
        let index_buffer = Self::create_index_buffer(&context, model.index_data(), index_type);
        let vertex_buffer = Self::create_vertex_buffer(&context, model.vertpbr());

        Self {
            vertex_buffer,
            index_buffer,
            handle: None,
        }
    }

    /// Create a skinned mesh from a GLTF model with skeletal animation data.
    pub fn new_skinned_from_model(model: Rc<GLTFModel>, context: Rc<VulkanContext>) -> Self {
        let index_type = match model.index_stride {
            1 => IndexType::Uint8,
            2 => IndexType::Uint16,
            4 => IndexType::Uint32,
            _ => IndexType::None,
        };
        let index_buffer = Self::create_index_buffer(&context, model.index_data(), index_type);
        let vertex_buffer = Self::create_vertex_buffer(&context, model.vertskinned());

        Self {
            vertex_buffer,
            index_buffer,
            handle: None,
        }
    }

    fn create_index_buffer<DataType>(
        context: &Rc<VulkanContext>,
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
                IndexType::Uint8 => data_slice.len() as u32,
                IndexType::Uint16 => (data_slice.len() as u32) / 2,
                IndexType::Uint32 => (data_slice.len() as u32) / 4,
                IndexType::None => 0_u32,
            };
            let mut index_buffer =
                IndexBuffer::new(context.clone(), data_slice.len() as u64, index_type, count);
            index_buffer.upload_data(data_slice);
            Some(index_buffer)
        }
    }

    fn create_vertex_buffer<DataType>(
        context: &Rc<VulkanContext>,
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
        } else if let Some(vertex_buffer) = &self.vertex_buffer {
            command_buffer.bind_vertex_buffers(0, &[vertex_buffer.object()], &[0]);
            command_buffer.draw_array(vertex_buffer.count(), 1, 0, 0);
        }
    }

    /// Get the handle (returns None if not yet registered with renderer)
    pub fn handle(&self) -> Option<MeshHandle> {
        self.handle
    }
}
