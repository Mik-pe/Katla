use ash::vk;
use std::{path::Path, rc::Rc};

use super::{
    DescriptorLayoutBuilder, ImageInfo, MaterialDescriptor, MaterialError, MaterialPipeline,
    PipelineBuilder, ShaderModule, ShaderSource,
};
use crate::render_graph::types::ShaderStages;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::pipeline_state::{CullMode, CompareOp, FrontFace};
use crate::{ImageFormat, Texture, VertexBinding};

pub struct MaterialBuilder {
    context: Rc<VulkanContext>,
    vertex_shader: Option<ShaderModule>,
    fragment_shader: Option<ShaderModule>,
    vertex_binding: Option<VertexBinding>,
    texture: Option<Rc<Texture>>,
    depth_test: bool,
    depth_write: bool,
    depth_compare_op: CompareOp,
    cull_back_faces: bool,
    alpha_blending: bool,
    has_color: bool,
    color_format: Option<ImageFormat>,
    depth_format: Option<ImageFormat>,
    push_constant_ranges: Vec<vk::PushConstantRange>,
    /// Use UI-style texture layout (sampled image, sampler, sampled image)
    ui_texture_layout: bool,
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
            depth_compare_op: CompareOp::Less,
            cull_back_faces: true,
            alpha_blending: false,
            has_color: false,
            color_format: None,
            depth_format: None,
            push_constant_ranges: Vec::new(),
            ui_texture_layout: false,
        }
    }

    /// Create a MaterialBuilder from a MaterialDescriptor.
    ///
    /// This provides a high-level API for material creation without manual flag configuration.
    /// The descriptor contains all information needed to build the material pipeline.
    pub fn from_descriptor(
        descriptor: MaterialDescriptor,
        context: Rc<VulkanContext>,
    ) -> Result<Self, MaterialError> {
        let mut builder = Self::new(context.clone());

        // Load vertex shader
        let vertex_shader = match &descriptor.vertex_shader {
            ShaderSource::WgslFile(path) => ShaderModule::from_wgsl(
                context.device.clone(),
                path,
                vk::ShaderStageFlags::VERTEX,
                "vs_main",
            )
            .map_err(|e| MaterialError::ShaderCompilationFailed {
                stage: crate::vulkan::material::ShaderStage::Vertex,
                error: format!("{:?}", e),
            })?,
            ShaderSource::WgslString(wgsl) => ShaderModule::from_wgsl_string(
                context.device.clone(),
                wgsl,
                vk::ShaderStageFlags::VERTEX,
                "vs_main",
            )
            .map_err(|e| MaterialError::ShaderCompilationFailed {
                stage: crate::vulkan::material::ShaderStage::Vertex,
                error: format!("{:?}", e),
            })?,
            ShaderSource::PreCompiled(bytes) => ShaderModule::from_bytes(
                context.device.clone(),
                bytes,
                vk::ShaderStageFlags::VERTEX,
                "main",
            )
            .map_err(|e| MaterialError::ShaderCompilationFailed {
                stage: crate::vulkan::material::ShaderStage::Vertex,
                error: format!("{:?}", e),
            })?,
        };
        builder.vertex_shader = Some(vertex_shader);

        // Load fragment shader
        let fragment_shader = match &descriptor.fragment_shader {
            ShaderSource::WgslFile(path) => ShaderModule::from_wgsl(
                context.device.clone(),
                path,
                vk::ShaderStageFlags::FRAGMENT,
                "fs_main",
            )
            .map_err(|e| MaterialError::ShaderCompilationFailed {
                stage: crate::vulkan::material::ShaderStage::Fragment,
                error: format!("{:?}", e),
            })?,
            ShaderSource::WgslString(wgsl) => ShaderModule::from_wgsl_string(
                context.device.clone(),
                wgsl,
                vk::ShaderStageFlags::FRAGMENT,
                "fs_main",
            )
            .map_err(|e| MaterialError::ShaderCompilationFailed {
                stage: crate::vulkan::material::ShaderStage::Fragment,
                error: format!("{:?}", e),
            })?,
            ShaderSource::PreCompiled(bytes) => ShaderModule::from_bytes(
                context.device.clone(),
                bytes,
                vk::ShaderStageFlags::FRAGMENT,
                "main",
            )
            .map_err(|e| MaterialError::ShaderCompilationFailed {
                stage: crate::vulkan::material::ShaderStage::Fragment,
                error: format!("{:?}", e),
            })?,
        };
        builder.fragment_shader = Some(fragment_shader);

        // Configure render state from descriptor
        builder.depth_test = descriptor.render_state.depth_test;
        builder.depth_write = descriptor.render_state.depth_write;
        builder.cull_back_faces = descriptor.render_state.cull_backfaces;
        builder.alpha_blending = descriptor.render_state.alpha_blending;

        // All shaders are WGSL now, which uses separate bindings
        builder.has_color = descriptor.has_color_uniform();

        // Set default formats for dynamic rendering (Vulkan 1.3)
        // Color format matches the HDR viewport render target (R16G16B16A16_SFLOAT)
        // Depth format matches the depth buffer (D32_SFLOAT_S8_UINT)
        builder.color_format = Some(ImageFormat::R16G16B16A16Sfloat);
        builder.depth_format = Some(ImageFormat::D32SfloatS8Uint);

        Ok(builder)
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

    /// Use a UI-style texture descriptor layout.
    ///
    /// This creates a layout suitable for UI rendering:
    /// - binding 0: SAMPLED_IMAGE (font atlas)
    /// - binding 1: SAMPLER
    /// - binding 2: SAMPLED_IMAGE (viewport texture)
    pub fn with_ui_texture_layout(mut self) -> Self {
        self.ui_texture_layout = true;
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

    pub fn with_depth_compare_op(mut self, op: CompareOp) -> Self {
        self.depth_compare_op = op;
        self
    }

    /// Configure for sky/background rendering.
    ///
    /// Sets up the pipeline for sky rendering with:
    /// - Depth test enabled but depth write disabled
    /// - Depth compare = ALWAYS (sky always behind geometry)
    /// - No backface culling
    pub fn with_sky_rendering(mut self) -> Self {
        self.depth_test = true;
        self.depth_write = false;
        self.depth_compare_op = CompareOp::Always;
        self.cull_back_faces = false;
        self
    }

    /// Configure for grid rendering.
    ///
    /// Sets up the pipeline for editor grid rendering with:
    /// - Alpha blending enabled (grid lines have transparency for AA)
    /// - Depth test enabled but depth write DISABLED (grid doesn't block objects behind it)
    /// - Depth compare = LESS_OR_EQUAL (grid visible where not occluded by closer geometry)
    /// - No backface culling
    pub fn with_grid_rendering(mut self) -> Self {
        self.alpha_blending = true;
        self.depth_test = true;
        self.depth_write = false; // Don't write depth - let alpha blending handle visibility
        self.depth_compare_op = CompareOp::LessOrEqual;
        self.cull_back_faces = false;
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

    pub fn with_color_uniform(mut self, has_color: bool) -> Self {
        self.has_color = has_color;
        self
    }

    /// Set color attachment format for dynamic rendering (Vulkan 1.3).
    ///
    /// This is used instead of a render pass for pipeline creation.
    pub fn with_color_format(mut self, format: ImageFormat) -> Self {
        self.color_format = Some(format);
        self
    }

    /// Set depth attachment format for dynamic rendering (Vulkan 1.3).
    ///
    /// This is used instead of a render pass for pipeline creation.
    pub fn with_depth_format(mut self, format: ImageFormat) -> Self {
        self.depth_format = Some(format);
        self
    }

    /// Add a push constant range to the pipeline layout.
    ///
    /// Push constants are fast uniform updates that don't require descriptor sets.
    /// Commonly used for small frequently-changing data like transform matrices.
    ///
    /// # Arguments
    /// * `stages` - Shader stages that can access this push constant range
    /// * `offset` - Offset in bytes within the push constant block
    /// * `size` - Size in bytes of the push constant range
    pub fn with_push_constant(mut self, stages: ShaderStages, offset: u32, size: u32) -> Self {
        self.push_constant_ranges.push(
            vk::PushConstantRange::default()
                .stage_flags(stages.into())
                .offset(offset)
                .size(size),
        );
        self
    }

    /// Build the material pipeline with an existing descriptor set layout.
    ///
    /// This is used during hot reload to preserve the descriptor set layout,
    /// ensuring that material instances' descriptor sets remain valid.
    ///
    /// # Arguments
    /// * `existing_desc_layout` - The existing descriptor set layout to reuse
    pub fn build_with_desc_layout(
        self,
        existing_desc_layout: vk::DescriptorSetLayout,
    ) -> Result<MaterialPipeline, MaterialBuildError> {
        let vertex_binding = self
            .vertex_binding
            .ok_or(MaterialBuildError::MissingVertexBinding)?;

        let vert_shader = self
            .vertex_shader
            .ok_or(MaterialBuildError::MissingVertexShader)?;
        let frag_shader = self
            .fragment_shader
            .ok_or(MaterialBuildError::MissingFragmentShader)?;

        // Use the existing descriptor set layout instead of creating a new one
        // This preserves compatibility with material instances' descriptor sets
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
            .with_depth_test(self.depth_test, self.depth_write, self.depth_compare_op)
            .with_descriptor_layouts(vec![existing_desc_layout]);

        if self.cull_back_faces {
            pipeline_builder = pipeline_builder
                .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise);
        } else {
            pipeline_builder = pipeline_builder
                .with_cull_mode(CullMode::None, FrontFace::CounterClockwise);
        }

        if self.alpha_blending {
            pipeline_builder = pipeline_builder.with_alpha_blending();
        }

        // Always use null render pass for dynamic rendering (Vulkan 1.3)
        let vk_render_pass = vk::RenderPass::null();

        let pipeline = pipeline_builder
            .build(crate::sync::VkRenderPass::from(vk_render_pass))
            .map_err(|e| MaterialBuildError::PipelineCreationFailed(format!("{:?}", e)))?;

        // All shaders are WGSL, which uses separate bindings
        let material_pipeline = MaterialPipeline::new_custom(
            pipeline,
            existing_desc_layout,
            self.context.clone(),
        );

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
