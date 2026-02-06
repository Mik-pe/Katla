use crate::{rendering::vertextypes::*, util::GLTFModel};

use katla_math::{Color, Mat4};
use katla_vulkan::{
    context::VulkanContext, CommandBuffer, ImageFormat, MaterialBuilder, MaterialHandle,
    MaterialPipeline, RenderPass, Texture, VertexBinding,
};

use std::{cell::RefCell, path::Path, rc::Rc};

#[derive(Clone)]
pub struct Material {
    pub material_pipeline: Rc<RefCell<MaterialPipeline>>,
    pub texture: Option<Rc<Texture>>,
    /// Vertex binding description (needed for renderer registration)
    pub vertex_binding: VertexBinding,
    /// Handle after registration with renderer (None until registered)
    pub handle: Option<MaterialHandle>,
    /// Optional material color for blending (multiplied with texture)
    pub color: Option<Color>,
}

impl Material {
    pub fn new(model: Rc<GLTFModel>, context: Rc<VulkanContext>, render_pass: &RenderPass) -> Self {
        let vertex_binding = VertexPBR::get_vertex_binding();

        let mut texture: Option<Rc<Texture>> = None;
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
                        ImageFormat::R8G8B8A8Srgb,
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
            .with_vertex_binding(vertex_binding.clone())
            .with_wgsl_shader(Path::new("resources/shaders/model_pbr.wgsl"))
            .with_color_uniform(true)  // Enable color uniform (shader expects it)
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
            material_pipeline: Rc::new(RefCell::new(material_pipeline)),
            texture,
            vertex_binding,
            handle: None,
            color: None,
        }
    }

    pub fn bind(&self, command_buffer: &CommandBuffer) {
        self.material_pipeline
            .borrow()
            .bind(command_buffer.vk_command_buffer());
    }

    pub fn upload_pipeline_data(&mut self, view: Mat4, proj: Mat4, model: Mat4) {
        self.upload_pipeline_data_with_color(view, proj, model, None);
    }

    pub fn upload_pipeline_data_with_color(
        &mut self,
        view: Mat4,
        proj: Mat4,
        model: Mat4,
        color: Option<Color>,
    ) {
        let mat = [model, view, proj];
        let base_data_slice = unsafe {
            std::slice::from_raw_parts(mat.as_ptr() as *const u8, std::mem::size_of_val(&mat))
        };

        // Always include color (default to white if not specified)
        // This is necessary because the shader expects the color uniform when has_color is enabled
        let c = color.or(self.color).unwrap_or(Color::WHITE);
        let color_array = c.to_array();
        let mut data = Vec::with_capacity(base_data_slice.len() + color_array.len() * 4);
        data.extend_from_slice(base_data_slice);
        // Extend with color as f32 bytes
        unsafe {
            let color_bytes = std::slice::from_raw_parts(
                color_array.as_ptr() as *const u8,
                color_array.len() * 4,
            );
            data.extend_from_slice(color_bytes);
        }

        self.material_pipeline.borrow_mut().update_buffer(&data);
    }

    /// Get the handle (returns None if not yet registered with renderer)
    pub fn handle(&self) -> Option<MaterialHandle> {
        self.handle
    }
}
