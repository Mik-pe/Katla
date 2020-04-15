use std::path::PathBuf;
use std::sync::Arc;
use vulkano::descriptor::PipelineLayoutAbstract;
use vulkano::device::Device;
use vulkano::framebuffer::{RenderPassAbstract, Subpass};
use vulkano::pipeline::vertex::SingleBufferDefinition;
use vulkano::pipeline::vertex::Vertex;
use vulkano::pipeline::GraphicsPipeline;

pub mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        path: "resources/shaders/model.vert"
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "resources/shaders/model.frag"
    }
}

pub struct RenderPipeline<V> {
    pub pipeline: Arc<
        GraphicsPipeline<
            SingleBufferDefinition<V>,
            Box<dyn PipelineLayoutAbstract + Send + Sync>,
            Arc<dyn RenderPassAbstract + Send + Sync>,
        >,
    >,
}

impl<V> RenderPipeline<V> {
    //Call with e.g. SingleBufferDefinition::new() as V
    pub fn new_with_shaders(
        vs_path: PathBuf,
        device: Arc<Device>,
        render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    ) -> Self
    where
        V: Vertex,
    {
        let vs = vs::Shader::load(device.clone()).unwrap();
        let fs = fs::Shader::load(device.clone()).unwrap();

        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input(SingleBufferDefinition::<V>::new())
                .vertex_shader(vs.main_entry_point(), ())
                .triangle_list()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(fs.main_entry_point(), ())
                .depth_stencil_simple_depth()
                .render_pass(Subpass::from(render_pass, 0).unwrap())
                .build(device.clone())
                .unwrap(),
        );
        Self { pipeline }
    }
}
