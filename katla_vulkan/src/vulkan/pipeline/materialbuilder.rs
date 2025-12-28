use ash::vk;
use std::{path::Path, rc::Rc};

use super::{DescriptorLayoutBuilder, ImageInfo, MaterialPipeline, PipelineBuilder, ShaderModule};
use crate::{context::VulkanContext, RenderPass, Texture, VertexBinding};

pub struct MaterialBuilder {
    context: Rc<VulkanContext>,
    vertex_shader: Option<ShaderModule>,
    fragment_shader: Option<ShaderModule>,
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
        let vertex_shader = ShaderModule::from_bytes(
            self.context.device.clone(),
            shader_bytes,
            vk::ShaderStageFlags::VERTEX,
            "main",
        )
        .unwrap();
        self.vertex_shader = Some(vertex_shader);
        self
    }

    pub fn with_fragment_shader(mut self, shader_bytes: &[u8]) -> Self {
        let fragment_shader = ShaderModule::from_bytes(
            self.context.device.clone(),
            shader_bytes,
            vk::ShaderStageFlags::FRAGMENT,
            "main",
        )
        .unwrap();
        self.fragment_shader = Some(fragment_shader);
        self
    }

    pub fn with_wgsl_shader(mut self, wgsl_path: &Path) -> Self {
        let vertex_shader = ShaderModule::from_wgsl(
            self.context.device.clone(),
            wgsl_path,
            vk::ShaderStageFlags::VERTEX,
            "vs_main",
        )
        .unwrap();
        self.vertex_shader = Some(vertex_shader);
        let fragment_shader = ShaderModule::from_wgsl(
            self.context.device.clone(),
            wgsl_path,
            vk::ShaderStageFlags::FRAGMENT,
            "fs_main",
        )
        .unwrap();
        self.fragment_shader = Some(fragment_shader);
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

        let vert_shader = self
            .vertex_shader
            .ok_or(MaterialBuildError::MissingVertexShader)?;
        let frag_shader = self
            .fragment_shader
            .ok_or(MaterialBuildError::MissingFragmentShader)?;

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
            .with_entry_points(
                vert_shader.entry_point.clone(),
                frag_shader.entry_point.clone(),
            )
            .with_vertex_input(
                vec![vertex_binding.get_binding_desc(0)],
                vertex_binding.get_attribute_desc(0),
            )
            .with_depth_test(self.depth_test, self.depth_write, vk::CompareOp::LESS)
            .with_descriptor_layouts(vec![desc_layout]);

        if self.cull_back_faces {
            pipeline_builder = pipeline_builder
                .with_cull_mode(vk::CullModeFlags::BACK, vk::FrontFace::COUNTER_CLOCKWISE);
        } else {
            pipeline_builder = pipeline_builder
                .with_cull_mode(vk::CullModeFlags::NONE, vk::FrontFace::COUNTER_CLOCKWISE);
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
    MissingVertexShader,
    MissingFragmentShader,
    ShaderCreationFailed(String),
    DescriptorLayoutFailed(String),
    PipelineCreationFailed(String),
}

impl std::fmt::Display for MaterialBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVertexBinding => write!(f, "Vertex binding not provided"),
            Self::MissingVertexShader => write!(f, "Vertex shader not provided"),
            Self::MissingFragmentShader => write!(f, "Fragment shader not provided"),
            Self::ShaderCreationFailed(e) => write!(f, "Shader creation failed: {}", e),
            Self::DescriptorLayoutFailed(e) => {
                write!(f, "Descriptor layout creation failed: {}", e)
            }
            Self::PipelineCreationFailed(e) => write!(f, "Pipeline creation failed: {}", e),
        }
    }
}

impl std::error::Error for MaterialBuildError {}
