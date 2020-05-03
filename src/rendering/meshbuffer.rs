use std::sync::Arc;

use vulkano::buffer::cpu_access::CpuAccessibleBuffer;
use vulkano::buffer::BufferUsage;
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::descriptor::descriptor_set::DescriptorSet;
use vulkano::device::Device;
use vulkano::pipeline::vertex::Vertex;
use vulkano::pipeline::GraphicsPipelineAbstract;
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

pub trait MeshData {
    fn draw_data(
        &self,
        cmd_buffer_builder: AutoCommandBufferBuilder,
        set: Arc<dyn DescriptorSet + Send + Sync>,
        dynamic_state: &DynamicState,
        renderpipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    ) -> AutoCommandBufferBuilder;
}

impl<T> MeshData for MeshBuffer<T>
where
    T: Vertex + Clone,
{
    fn draw_data(
        &self,
        cmd_buffer_builder: AutoCommandBufferBuilder,
        set: Arc<dyn DescriptorSet + Send + Sync>,
        dynamic_state: &DynamicState,
        renderpipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    ) -> AutoCommandBufferBuilder {
        cmd_buffer_builder
            .draw(
                renderpipeline.clone(),
                dynamic_state,
                vec![self.vertex_buffer.clone()],
                set,
                (),
            )
            .unwrap()
    }
}
