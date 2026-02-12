use crate::{rendering::vertextypes::*, util::GLTFModel};

use katla_math::{Color, Mat4};
use katla_vulkan::{
    context::VulkanContext, material::UniformHandle, CommandBuffer, ImageFormat, MaterialBuilder,
    MaterialHandle, MaterialPipeline, MaterialTemplate, Texture, VertexBinding,
};

use std::{cell::RefCell, path::Path, rc::Rc};

pub struct Material {
    pub material_pipeline: Rc<RefCell<MaterialPipeline>>,
    pub texture: Option<Rc<Texture>>,
    /// Vertex binding description (needed for renderer registration)
    pub vertex_binding: VertexBinding,
    /// Handle after registration with renderer (None until registered)
    pub handle: Option<MaterialHandle>,
    /// Optional material color for blending (multiplied with texture)
    pub color: Option<Color>,
    /// Optional per-material uniform buffer (for template-based materials)
    /// This allows multiple materials to share a pipeline while having different uniforms
    /// Note: This is NOT cloneable - when cloning a Material, you must create a new uniform buffer
    uniform: Option<UniformHandle>,
}

impl Clone for Material {
    fn clone(&self) -> Self {
        Self {
            material_pipeline: Rc::clone(&self.material_pipeline),
            texture: self.texture.clone(),
            vertex_binding: self.vertex_binding.clone(),
            handle: None, // Cloned materials need re-registration
            color: self.color,
            uniform: None, // Cloned materials lose their uniform buffer (must re-upload data)
        }
    }
}

impl Material {
    pub fn new(model: Rc<GLTFModel>, context: Rc<VulkanContext>) -> Self {
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
            .with_color_uniform(true) // Enable color uniform (shader expects it)
            .with_backface_culling(true)
            .with_depth_test(true)
            .with_depth_write(true)
            // Dynamic rendering: specify attachment formats
            .with_color_format(ImageFormat::B8G8R8A8Srgb)
            .with_depth_format(ImageFormat::D32SfloatS8Uint);

        if let Some(ref tex) = texture {
            builder = builder.with_texture(tex.clone());
        }

        let material_pipeline = builder
            .build()
            .expect("Failed to create material pipeline");

        Self {
            material_pipeline: Rc::new(RefCell::new(material_pipeline)),
            texture,
            vertex_binding,
            handle: None,
            color: None,
            uniform: None, // Non-template materials have embedded uniform in pipeline
        }
    }

    /// Create a material from a MaterialTemplate.
    ///
    /// This allows materials to share pipelines with templates, enabling
    /// hot reload to update all materials using the same template.
    ///
    /// Each material gets its own uniform buffer to avoid conflicts when
    /// multiple materials use the same template.
    ///
    /// # Arguments
    /// * `template` - The material template containing the shared pipeline
    /// * `texture` - Optional texture for this material instance
    /// * `color` - Optional color override for this material instance
    pub fn from_template(
        template: &MaterialTemplate,
        texture: Option<Rc<Texture>>,
        color: Option<Color>,
    ) -> Self {
        // Get the vertex binding from the template's descriptor
        // For now, we use a default PBR binding - this should come from the template
        use katla_vulkan::vertexbinding::get_pbr_vertex_binding;

        // Create a new uniform buffer for this material instance
        // This allows each material to have its own uniforms while sharing the pipeline
        let mut uniform = template.create_uniform();

        // If a texture is provided, update the uniform's descriptor sets with the texture image info
        if let Some(ref tex) = texture {
            use katla_vulkan::material::ImageInfo;
            let image_info = ImageInfo::new(tex.image_view.vk(), tex.image_sampler.vk());
            uniform.add_image_info(image_info);
        }

        Self {
            material_pipeline: template.pipeline(),
            texture,
            vertex_binding: get_pbr_vertex_binding(),
            handle: None,
            color,
            uniform: Some(uniform),
        }
    }

    /// Create a material from a MaterialPipeline directly (non-template).
    ///
    /// This is used for materials that don't use templates and have their
    /// pipeline with embedded uniform buffer.
    ///
    /// # Arguments
    /// * `material_pipeline` - The material pipeline
    /// * `texture` - Optional texture
    /// * `vertex_binding` - Vertex binding description
    /// * `color` - Optional color
    pub fn from_pipeline(
        material_pipeline: MaterialPipeline,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
        color: Option<Color>,
    ) -> Self {
        Self {
            material_pipeline: Rc::new(RefCell::new(material_pipeline)),
            texture,
            vertex_binding,
            handle: None,
            color,
            uniform: None, // Non-template materials have embedded uniform in pipeline
        }
    }

    pub fn bind(&self, command_buffer: &CommandBuffer) {
        let pipeline = self.material_pipeline.borrow();
        let vk_cmd = command_buffer.vk_command_buffer();

        // If material has its own uniform buffer, bind with custom descriptor set
        // Otherwise use the pipeline's standard bind method
        if let Some(ref uniform) = self.uniform {
            // Bind pipeline with material's own descriptor set
            pipeline.bind_with_descriptor(vk_cmd, uniform.next_descriptor().desc_set);
        } else {
            // Use pipeline's standard bind (embedded uniform)
            pipeline.bind(vk_cmd);
        }
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

        // Update the material's own uniform buffer if it has one, otherwise update pipeline's
        if let Some(ref mut uniform) = self.uniform {
            // Get context from pipeline to update the uniform buffer
            let pipeline = self.material_pipeline.borrow();
            uniform.update_buffer(pipeline.context(), &data);
        } else {
            self.material_pipeline.borrow_mut().update_buffer(&data);
        }
    }

    /// Get the handle (returns None if not yet registered with renderer)
    pub fn handle(&self) -> Option<MaterialHandle> {
        self.handle
    }

    /// Get the pipeline, texture, vertex binding, and optional uniform for registration
    ///
    /// This consumes the material and returns ownership of all fields.
    /// Used when registering materials with the renderer's AssetRegistry.
    #[allow(clippy::type_complexity)]
    pub fn get_registration_data(
        self,
    ) -> (
        Rc<RefCell<MaterialPipeline>>,
        Option<Rc<Texture>>,
        VertexBinding,
        Option<UniformHandle>,
    ) {
        (
            self.material_pipeline,
            self.texture,
            self.vertex_binding,
            self.uniform,
        )
    }

    /// Destroy the per-material uniform buffer (if any).
    ///
    /// This should be called during shutdown after waiting for the GPU to finish.
    pub fn destroy_uniform(&mut self, context: &Rc<VulkanContext>) {
        if let Some(mut uniform) = self.uniform.take() {
            uniform.destroy(context);
        }
    }
}
