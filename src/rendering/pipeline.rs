use std::path::PathBuf;
use std::sync::Arc;
use vulkano::descriptor::PipelineLayoutAbstract;
use vulkano::device::Device;
use vulkano::framebuffer::{RenderPassAbstract, Subpass};
use vulkano::pipeline::vertex::SingleBufferDefinition;
use vulkano::pipeline::vertex::Vertex;
use vulkano::pipeline::GraphicsPipeline;

mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: "
            #version 450
            layout(location = 0) in vec2 position;
            void main() {
                gl_Position = vec4(position, 0.0, 1.0);
            }
        "
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: "
            #version 450
            layout(location = 0) out vec4 f_color;
            void main() {
                f_color = vec4(0.7, 0.5, 0.3, 1.0);
            }
        "
    }
}

//Call with e.g. SingleBufferDefinition::new() as V
pub fn create_pipeline_with_shaders<V>(
    vs_path: PathBuf,
    device: Arc<Device>,
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
) -> Arc<
    GraphicsPipeline<
        SingleBufferDefinition<V>,
        Box<dyn PipelineLayoutAbstract + Send + Sync>,
        Arc<dyn RenderPassAbstract + Send + Sync>,
    >,
>
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
            .render_pass(Subpass::from(render_pass, 0).unwrap())
            .build(device.clone())
            .unwrap(),
    );
    pipeline
}
