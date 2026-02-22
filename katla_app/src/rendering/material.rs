use crate::{rendering::vertextypes::*, util::GLTFModel};

use katla_math::Color;
use katla_vulkan::{
    context::VulkanContext, material::PbrTextureSet, ImageFormat, MaterialBuilder, MaterialHandle,
    MaterialPipeline, MaterialTemplate, Texture, VertexBinding,
};
use log::warn;

use std::{cell::RefCell, path::Path, rc::Rc};

pub struct Material {
    pub material_pipeline: Rc<RefCell<MaterialPipeline>>,
    pub texture: Option<Rc<Texture>>,
    pub vertex_binding: VertexBinding,
    pub handle: Option<MaterialHandle>,
    pub color: Option<Color>,
    pub pbr_textures: Option<PbrTextureSet>,
    pub pbr_texture_refs: Option<Vec<Rc<Texture>>>,
    pub texture_indices: [u32; 4],
    pub emission_index: u32,
}

impl Clone for Material {
    fn clone(&self) -> Self {
        Self {
            material_pipeline: Rc::clone(&self.material_pipeline),
            texture: self.texture.clone(),
            vertex_binding: self.vertex_binding.clone(),
            handle: None,
            color: self.color,
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices: self.texture_indices,
            emission_index: self.emission_index,
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
                    warn!("Unsupported texture format: {:?}", image.format);
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
            .with_color_format(ImageFormat::R16G16B16A16Sfloat)
            .with_depth_format(ImageFormat::D32SfloatS8Uint);

        if let Some(ref tex) = texture {
            builder = builder.with_texture(tex.clone());
        }

        let material_pipeline = builder.build().expect("Failed to create material pipeline");

        Self {
            material_pipeline: Rc::new(RefCell::new(material_pipeline)),
            texture,
            vertex_binding,
            handle: None,
            color: None,
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices: [0; 4],
            emission_index: 0,
        }
    }

    /// Create a material from a MaterialTemplate.
    pub fn from_template(
        template: &MaterialTemplate,
        texture: Option<Rc<Texture>>,
        color: Option<Color>,
    ) -> Self {
        use katla_vulkan::vertexbinding::get_pbr_vertex_binding;

        Self {
            material_pipeline: template.pipeline(),
            texture,
            vertex_binding: get_pbr_vertex_binding(),
            handle: None,
            color,
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices: [0; 4],
            emission_index: 0,
        }
    }

    /// Create a full PBR material from a MaterialTemplate with all texture maps.
    pub fn from_template_pbr(
        template: &MaterialTemplate,
        pbr_textures: PbrTextureSet,
        texture_refs: Vec<Rc<Texture>>,
        color: Option<Color>,
    ) -> Self {
        use katla_vulkan::vertexbinding::get_pbr_vertex_binding;

        Self {
            material_pipeline: template.pipeline(),
            texture: None,
            vertex_binding: get_pbr_vertex_binding(),
            handle: None,
            color,
            pbr_textures: Some(pbr_textures),
            pbr_texture_refs: Some(texture_refs),
            texture_indices: [0; 4],
            emission_index: 0,
        }
    }

    /// Create a skinned material from a template for skeletal animation.
    pub fn from_template_skinned(
        template: &MaterialTemplate,
        texture: Option<Rc<Texture>>,
        color: Option<Color>,
    ) -> Self {
        use katla_vulkan::vertexbinding::get_skinned_vertex_binding;

        Self {
            material_pipeline: template.pipeline(),
            texture,
            vertex_binding: get_skinned_vertex_binding(),
            handle: None,
            color,
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices: [0; 4],
            emission_index: 0,
        }
    }

    /// Create a skinned material with bindless texture indices.
    pub fn from_template_skinned_with_bindless(
        template: &MaterialTemplate,
        texture: Option<Rc<Texture>>,
        color: Option<Color>,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> Self {
        use katla_vulkan::vertexbinding::get_skinned_vertex_binding;

        Self {
            material_pipeline: template.pipeline(),
            texture,
            vertex_binding: get_skinned_vertex_binding(),
            handle: None,
            color,
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices,
            emission_index,
        }
    }

    /// Create a full PBR material with bindless texture indices.
    pub fn from_template_pbr_bindless(
        template: &MaterialTemplate,
        pbr_textures: PbrTextureSet,
        texture_refs: Vec<Rc<Texture>>,
        color: Option<Color>,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> Self {
        use katla_vulkan::vertexbinding::get_pbr_vertex_binding;

        Self {
            material_pipeline: template.pipeline(),
            texture: None,
            vertex_binding: get_pbr_vertex_binding(),
            handle: None,
            color,
            pbr_textures: Some(pbr_textures),
            pbr_texture_refs: Some(texture_refs),
            texture_indices,
            emission_index,
        }
    }

    /// Create a skinned PBR material with bindless texture indices.
    pub fn from_template_skinned_pbr_bindless(
        template: &MaterialTemplate,
        pbr_textures: PbrTextureSet,
        texture_refs: Vec<Rc<Texture>>,
        color: Option<Color>,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> Self {
        use katla_vulkan::vertexbinding::get_skinned_vertex_binding;

        Self {
            material_pipeline: template.pipeline(),
            texture: None,
            vertex_binding: get_skinned_vertex_binding(),
            handle: None,
            color,
            pbr_textures: Some(pbr_textures),
            pbr_texture_refs: Some(texture_refs),
            texture_indices,
            emission_index,
        }
    }

    /// Create a material from a MaterialPipeline directly (non-template).
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
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices: [0; 4],
            emission_index: 0,
        }
    }

    /// Create a bindless material from a MaterialPipeline.
    pub fn from_pipeline_with_textures(
        material_pipeline: MaterialPipeline,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
        color: Option<Color>,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> Self {
        Self {
            material_pipeline: Rc::new(RefCell::new(material_pipeline)),
            texture,
            vertex_binding,
            handle: None,
            color,
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices,
            emission_index,
        }
    }

    pub fn handle(&self) -> Option<MaterialHandle> {
        self.handle
    }

    /// Get the pipeline, texture, vertex binding for registration.
    #[allow(clippy::type_complexity)]
    pub fn get_registration_data(
        self,
    ) -> (
        Rc<RefCell<MaterialPipeline>>,
        Option<Rc<Texture>>,
        VertexBinding,
        Option<PbrTextureSet>,
        Option<Vec<Rc<Texture>>>,
        [u32; 4],
        u32,
    ) {
        (
            self.material_pipeline,
            self.texture,
            self.vertex_binding,
            self.pbr_textures,
            self.pbr_texture_refs,
            self.texture_indices,
            self.emission_index,
        )
    }
}
