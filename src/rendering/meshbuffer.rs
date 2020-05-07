use std::sync::Arc;

use crate::rendering::pipeline::RenderPipeline;

use vulkano::buffer::cpu_access::CpuAccessibleBuffer;
use vulkano::buffer::BufferUsage;
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::descriptor::descriptor_set::DescriptorSet;
use vulkano::device::Device;
use vulkano::framebuffer::RenderPassAbstract;
use vulkano::pipeline::vertex::Vertex;
use vulkano::pipeline::GraphicsPipelineAbstract;
pub struct MeshBuffer<T>
where
    T: Vertex + Clone,
{
    pub vertex_buffer: Arc<CpuAccessibleBuffer<[T]>>,
    pub pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
}

impl<T> MeshBuffer<T>
where
    T: Vertex + Clone,
{
    pub fn new(
        device: Arc<Device>,
        render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
        data: Vec<T>,
    ) -> Self {
        let pipeline = RenderPipeline::new_with_shaders::<T>(
            std::path::PathBuf::new(),
            device.clone(),
            render_pass.clone(),
        )
        .pipeline;

        let vertex_buffer = CpuAccessibleBuffer::from_iter(
            device.clone(),
            BufferUsage::all(),
            false,
            data.iter().cloned(),
        )
        .unwrap();
        Self {
            vertex_buffer,
            pipeline,
        }
    }
}

pub trait MeshData {
    fn draw_data(
        &self,
        cmd_buffer_builder: AutoCommandBufferBuilder,
        set: Arc<dyn DescriptorSet + Send + Sync>,
        dynamic_state: &DynamicState,
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
    ) -> AutoCommandBufferBuilder {
        cmd_buffer_builder
            .draw(
                self.pipeline.clone(),
                dynamic_state,
                vec![self.vertex_buffer.clone()],
                set,
                (),
            )
            .unwrap()
    }
}
