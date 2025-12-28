use crate::{rendering::vertextypes::*, util::GLTFModel};

use katla_math::Mat4;
use katla_vulkan::{
    context::VulkanContext, CommandBuffer, Format, MaterialBuilder, MaterialPipeline, RenderPass,
    Texture,
};

use std::rc::Rc;

pub struct Material {
    pub material_pipeline: MaterialPipeline,
    pub texture: Option<Rc<Texture>>,
}

impl Material {
    pub fn new(model: Rc<GLTFModel>, context: Rc<VulkanContext>, render_pass: &RenderPass) -> Self {
        let vertex_binding = VertexPBR::get_vertex_binding();

        let mut texture = None;
        if !model.images.is_empty() {
            let image = &model.images[0];
            let pixels = &image.pixels;

            match image.format {
                gltf::image::Format::R8G8B8 => {
                    let tex = Texture::create_image_rgb(
                        context.clone(),
                        image.width,
                        image.height,
                        pixels.as_slice(),
                    );
                    texture = Some(Rc::new(tex));
                }
                gltf::image::Format::R8G8B8A8 => {
                    let tex = Texture::create_image(
                        context.clone(),
                        image.width,
                        image.height,
                        Format::R8G8B8A8_SRGB,
                        pixels.as_slice(),
                    );
                    texture = Some(Rc::new(tex));
                }
                _ => {
                    println!("Unsupported texture format: {:?}", image.format);
                }
            }
        }

        let mut builder = MaterialBuilder::new(context.clone())
            .with_vertex_binding(vertex_binding)
            .with_vertex_shader(include_bytes!(
                "../../../resources/shaders/model_pbr.vert.spv"
            ))
            .with_fragment_shader(include_bytes!("../../../resources/shaders/model.frag.spv"))
            .with_backface_culling(true)
            .with_depth_test(true)
            .with_depth_write(true);

        if let Some(ref tex) = texture {
            builder = builder.with_texture(tex.clone());
        }

        let material_pipeline = builder
            .build(render_pass)
            .expect("Failed to create material pipeline");

        Self {
            material_pipeline,
            texture,
        }
    }

    pub fn bind(&self, command_buffer: &CommandBuffer) {
        self.material_pipeline
            .bind(command_buffer.vk_command_buffer());
    }

    pub fn upload_pipeline_data(&mut self, view: Mat4, proj: Mat4, model: Mat4) {
        let mat = [model, view, proj];
        let data_slice = unsafe {
            std::slice::from_raw_parts(mat.as_ptr() as *const u8, std::mem::size_of_val(&mat))
        };
        self.material_pipeline.update_buffer(data_slice);
    }
}

impl Drop for Material {
    fn drop(&mut self) {
        self.material_pipeline.destroy();
    }
}
