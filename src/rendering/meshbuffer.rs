use crate::rendering::vertextypes;
use std::sync::Arc;

use vulkano::buffer::cpu_access::CpuAccessibleBuffer;
use vulkano::buffer::BufferUsage;
use vulkano::device::Device;
use vulkano::pipeline::vertex::Vertex;
pub struct MeshBuffer<T>
where
    T: Vertex + Clone,
{
    pub vertex_buffer: Arc<CpuAccessibleBuffer<[T]>>,
}

impl<T> MeshBuffer<T>
where
    T: Vertex + Clone,
{
    pub fn new(device: Arc<Device>, data: Vec<T>) -> Self {
        let buf = CpuAccessibleBuffer::from_iter(
            device.clone(),
            BufferUsage::all(),
            false,
            data.iter().cloned(),
        )
        .unwrap();
        Self { vertex_buffer: buf }
    }
}
