use ash::vk;
use std::rc::Rc;

use super::{DescriptorLayoutBuilder, ImageInfo, MaterialPipeline, PipelineBuilder, ShaderModule};
use crate::{context::VulkanContext, RenderPass, Texture, VertexBinding};

const DEFAULT_SHADER_VERT: &[u8] =
    include_bytes!("../../../../resources/shaders/model_pbr.vert.spv");
const DEFAULT_SHADER_FRAG: &[u8] = include_bytes!("../../../../resources/shaders/model.frag.spv");

pub struct MaterialBuilder {
    context: Rc<VulkanContext>,
    vertex_shader: Option<Vec<u8>>,
    fragment_shader: Option<Vec<u8>>,
    vertex_binding: Option<VertexBinding>,
    texture: Option<Rc<Texture>>,
    depth_test: bool,
    depth_write: bool,
    cull_back_faces: bool,
    alpha_blending: bool,
}

impl MaterialBuilder {
    pub fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            vertex_shader: None,
            fragment_shader: None,
            vertex_binding: None,
            texture: None,
            depth_test: true,
            depth_write: true,
            cull_back_faces: true,
            alpha_blending: false,
        }
    }

    pub fn with_vertex_shader(mut self, shader_bytes: &[u8]) -> Self {
        self.vertex_shader = Some(shader_bytes.to_vec());
        self
    }

    pub fn with_fragment_shader(mut self, shader_bytes: &[u8]) -> Self {
        self.fragment_shader = Some(shader_bytes.to_vec());
        self
    }

    pub fn with_vertex_binding(mut self, binding: VertexBinding) -> Self {
        self.vertex_binding = Some(binding);
        self
    }

    pub fn with_texture(mut self, texture: Rc<Texture>) -> Self {
        self.texture = Some(texture);
        self
    }

    pub fn with_depth_test(mut self, enable: bool) -> Self {
        self.depth_test = enable;
        self
    }

    pub fn with_depth_write(mut self, enable: bool) -> Self {
        self.depth_write = enable;
        self
    }

    pub fn with_backface_culling(mut self, enable: bool) -> Self {
        self.cull_back_faces = enable;
        self
    }

    pub fn with_alpha_blending(mut self, enable: bool) -> Self {
        self.alpha_blending = enable;
        self
    }

    pub fn build(self, render_pass: &RenderPass) -> Result<MaterialPipeline, MaterialBuildError> {
        let vertex_binding = self
            .vertex_binding
            .ok_or(MaterialBuildError::MissingVertexBinding)?;

        let vert_bytes = self.vertex_shader.as_deref().unwrap_or(DEFAULT_SHADER_VERT);
        let frag_bytes = self
            .fragment_shader
            .as_deref()
            .unwrap_or(DEFAULT_SHADER_FRAG);

        let vert_shader = ShaderModule::from_bytes(
            self.context.device.clone(),
            vert_bytes,
            vk::ShaderStageFlags::VERTEX,
        )
        .map_err(|e| MaterialBuildError::ShaderCreationFailed(format!("{:?}", e)))?;

        let frag_shader = ShaderModule::from_bytes(
            self.context.device.clone(),
            frag_bytes,
            vk::ShaderStageFlags::FRAGMENT,
        )
        .map_err(|e| MaterialBuildError::ShaderCreationFailed(format!("{:?}", e)))?;

        let desc_layout = DescriptorLayoutBuilder::new()
            .add_binding(
                0,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                1,
            )
            .add_binding(
                1,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
                1,
            )
            .build(&self.context.device)
            .map_err(|e| MaterialBuildError::DescriptorLayoutFailed(format!("{:?}", e)))?;

        let mut pipeline_builder = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_shader.module, frag_shader.module)
            .with_vertex_input(
                vec![vertex_binding.get_binding_desc(0)],
                vertex_binding.get_attribute_desc(0),
            )
            .with_depth_test(self.depth_test, self.depth_write, vk::CompareOp::LESS)
            .with_descriptor_layouts(vec![desc_layout]);

        if self.cull_back_faces {
            pipeline_builder =
                pipeline_builder.with_cull_mode(vk::CullModeFlags::BACK, vk::FrontFace::CLOCKWISE);
        } else {
            pipeline_builder =
                pipeline_builder.with_cull_mode(vk::CullModeFlags::NONE, vk::FrontFace::CLOCKWISE);
        }

        if self.alpha_blending {
            pipeline_builder = pipeline_builder.with_alpha_blending();
        }

        let pipeline = pipeline_builder
            .build(render_pass.get_vk_renderpass())
            .map_err(|e| MaterialBuildError::PipelineCreationFailed(format!("{:?}", e)))?;

        let mut material_pipeline =
            MaterialPipeline::new(pipeline, desc_layout, self.context.clone());

        if let Some(texture) = self.texture {
            material_pipeline
                .uniform
                .add_image_info(ImageInfo::new(texture.image_view, texture.image_sampler));
        }

        Ok(material_pipeline)
    }
}

#[derive(Debug)]
pub enum MaterialBuildError {
    MissingVertexBinding,
    ShaderCreationFailed(String),
    DescriptorLayoutFailed(String),
    PipelineCreationFailed(String),
}

impl std::fmt::Display for MaterialBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVertexBinding => write!(f, "Vertex binding not provided"),
            Self::ShaderCreationFailed(e) => write!(f, "Shader creation failed: {}", e),
            Self::DescriptorLayoutFailed(e) => {
                write!(f, "Descriptor layout creation failed: {}", e)
            }
            Self::PipelineCreationFailed(e) => write!(f, "Pipeline creation failed: {}", e),
        }
    }
}

impl std::error::Error for MaterialBuildError {}
