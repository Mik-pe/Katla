use std::{ffi::CString, rc::Rc};

use ash::vk;

use super::super::context::VulkanContext;
use crate::pipeline::{BlendFactor, BlendOp, CompareOp, CullMode, FrontFace, PolygonMode};
use crate::sync::VkRenderPass;
use crate::texture::ImageFormat;
use crate::vulkan::pipeline_state::{DynamicState, PrimitiveTopology};
use crate::vulkan::vertexbinding::{VertexBinding, VertexFormat};

pub struct PipelineBuilder {
    context: Rc<VulkanContext>,
    vertex_shader: Option<vk::ShaderModule>,
    fragment_shader: Option<vk::ShaderModule>,
    vertex_shader_entry_point: CString,
    fragment_shader_entry_point: CString,
    vertex_bindings: Vec<vk::VertexInputBindingDescription>,
    vertex_attributes: Vec<vk::VertexInputAttributeDescription>,
    topology: PrimitiveTopology,
    polygon_mode: PolygonMode,
    cull_mode: CullMode,
    front_face: FrontFace,
    line_width: f32,
    depth_test: bool,
    depth_write: bool,
    depth_compare_op: CompareOp,
    depth_bias_enable: bool,
    depth_bias_constant: f32,
    depth_bias_slope: f32,
    depth_bias_clamp: f32,
    stencil_test_enable: bool,
    stencil_front: Option<vk::StencilOpState>,
    stencil_back: Option<vk::StencilOpState>,
    blend_enable: bool,
    blend_src_color: BlendFactor,
    blend_dst_color: BlendFactor,
    blend_color_op: BlendOp,
    blend_src_alpha: BlendFactor,
    blend_dst_alpha: BlendFactor,
    blend_alpha_op: BlendOp,
    color_write_mask: vk::ColorComponentFlags,
    descriptor_layouts: Vec<vk::DescriptorSetLayout>,
    push_constant_ranges: Vec<vk::PushConstantRange>,
    dynamic_states: Vec<DynamicState>,
    // For dynamic rendering (Vulkan 1.3)
    color_format: Option<vk::Format>,
    depth_format: Option<vk::Format>,
}

impl PipelineBuilder {
    pub fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            vertex_shader: None,
            fragment_shader: None,
            vertex_bindings: Vec::new(),
            vertex_attributes: Vec::new(),
            vertex_shader_entry_point: CString::new("vs_main").unwrap(),
            fragment_shader_entry_point: CString::new("fs_main").unwrap(),
            topology: PrimitiveTopology::TriangleList,
            polygon_mode: PolygonMode::Fill,
            cull_mode: CullMode::Back,
            front_face: FrontFace::CounterClockwise,
            line_width: 1.0,
            depth_test: true,
            depth_write: true,
            depth_compare_op: CompareOp::Greater,
            depth_bias_enable: false,
            depth_bias_constant: 0.0,
            depth_bias_slope: 0.0,
            depth_bias_clamp: 0.0,
            stencil_test_enable: false,
            stencil_front: None,
            stencil_back: None,
            blend_enable: false,
            blend_src_color: BlendFactor::SrcAlpha,
            blend_dst_color: BlendFactor::OneMinusSrcAlpha,
            blend_color_op: BlendOp::Add,
            blend_src_alpha: BlendFactor::One,
            blend_dst_alpha: BlendFactor::Zero,
            blend_alpha_op: BlendOp::Add,
            color_write_mask: vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
            descriptor_layouts: Vec::new(),
            push_constant_ranges: Vec::new(),
            dynamic_states: vec![DynamicState::Viewport, DynamicState::Scissor],
            color_format: None,
            depth_format: None,
        }
    }

    pub fn with_shaders(mut self, vertex: vk::ShaderModule, fragment: vk::ShaderModule) -> Self {
        self.vertex_shader = Some(vertex);
        self.fragment_shader = Some(fragment);
        self
    }

    pub fn with_vertex_binding(mut self, binding: VertexBinding) -> Self {
        let binding_desc = binding.get_binding_desc(0);
        let attribute_descs = binding.get_attribute_desc(0);
        self.vertex_bindings.push(binding_desc);
        self.vertex_attributes.extend(attribute_descs);
        self
    }

    pub fn with_vertex_binding_soa(mut self, binding: VertexBinding) -> Self {
        let (binding_descs, attribute_descs) = binding.get_soa_descriptions();
        self.vertex_bindings.extend(binding_descs);
        self.vertex_attributes.extend(attribute_descs);
        self
    }

    /// Add a single SOA vertex attribute at a specific shader location.
    ///
    /// Each call adds one binding at `binding = location` and one attribute
    /// at the same location, suitable for per-attribute SOA vertex buffers.
    pub fn with_soa_attribute(mut self, location: u32, format: VertexFormat) -> Self {
        let stride = format.get_offset();
        self.vertex_bindings.push(
            vk::VertexInputBindingDescription::default()
                .binding(location)
                .stride(stride)
                .input_rate(vk::VertexInputRate::VERTEX),
        );
        self.vertex_attributes.push(
            vk::VertexInputAttributeDescription::default()
                .binding(location)
                .location(location)
                .format(format.get_vk_format())
                .offset(0),
        );
        self
    }

    pub fn with_descriptor_layouts(mut self, layouts: Vec<vk::DescriptorSetLayout>) -> Self {
        self.descriptor_layouts = layouts;
        self
    }

    /// Add a push constant range to the pipeline layout.
    #[allow(dead_code)]
    pub fn with_push_constant_range(
        mut self,
        stages: vk::ShaderStageFlags,
        offset: u32,
        size: u32,
    ) -> Self {
        self.push_constant_ranges.push(
            vk::PushConstantRange::default()
                .stage_flags(stages)
                .offset(offset)
                .size(size),
        );
        self
    }

    pub fn with_polygon_mode(mut self, mode: PolygonMode) -> Self {
        self.polygon_mode = mode;
        self
    }

    pub fn with_depth_test(mut self, enable: bool, write: bool, op: CompareOp) -> Self {
        self.depth_test = enable;
        self.depth_write = write;
        self.depth_compare_op = op;
        self
    }

    pub fn with_depth_bias(mut self, constant: f32, slope: f32, clamp: f32) -> Self {
        self.depth_bias_enable = true;
        self.depth_bias_constant = constant;
        self.depth_bias_slope = slope;
        self.depth_bias_clamp = clamp;
        self
    }

    pub fn with_cull_mode(mut self, mode: CullMode, front: FrontFace) -> Self {
        self.cull_mode = mode;
        self.front_face = front;
        self
    }

    pub fn with_alpha_blending(mut self) -> Self {
        self.blend_enable = true;
        self.blend_src_color = BlendFactor::SrcAlpha;
        self.blend_dst_color = BlendFactor::OneMinusSrcAlpha;
        self.blend_color_op = BlendOp::Add;
        self.blend_src_alpha = BlendFactor::One;
        self.blend_dst_alpha = BlendFactor::Zero;
        self.blend_alpha_op = BlendOp::Add;
        self
    }

    /// Enable stencil testing with separate front/back state.
    pub fn with_stencil_test(
        mut self,
        front: vk::StencilOpState,
        back: vk::StencilOpState,
    ) -> Self {
        self.stencil_test_enable = true;
        self.stencil_front = Some(front);
        self.stencil_back = Some(back);
        self
    }

    /// Set the color write mask (which color channels are written to the framebuffer).
    /// Use `vk::ColorComponentFlags::empty()` to disable all color writes (stencil-only pass).
    pub fn with_color_write_mask(mut self, mask: vk::ColorComponentFlags) -> Self {
        self.color_write_mask = mask;
        self
    }

    pub fn with_rendering_formats(
        mut self,
        color_format: Option<ImageFormat>,
        depth_format: Option<ImageFormat>,
    ) -> Self {
        self.color_format = color_format.map(|f| f.into());
        self.depth_format = depth_format.map(|f| f.into());
        self
    }

    pub(crate) fn build(
        self,
        render_pass: crate::sync::VkRenderPass,
    ) -> Result<Pipeline, PipelineError> {
        let vk_render_pass: vk::RenderPass = render_pass.into();
        let shader_vert = self
            .vertex_shader
            .ok_or(PipelineError::MissingVertexShader)?;
        let shader_frag = self
            .fragment_shader
            .ok_or(PipelineError::MissingFragmentShader)?;

        let shader_stages = vec![
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(shader_vert)
                .name(&self.vertex_shader_entry_point),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(shader_frag)
                .name(&self.fragment_shader_entry_point),
        ];

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&self.vertex_bindings)
            .vertex_attribute_descriptions(&self.vertex_attributes);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(self.topology.into())
            .primitive_restart_enable(false);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(self.polygon_mode.into())
            .line_width(self.line_width)
            .cull_mode(self.cull_mode.into())
            .front_face(self.front_face.into())
            .depth_bias_enable(self.depth_bias_enable)
            .depth_bias_constant_factor(self.depth_bias_constant)
            .depth_bias_slope_factor(self.depth_bias_slope)
            .depth_bias_clamp(self.depth_bias_clamp);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(self.color_write_mask)
            .blend_enable(self.blend_enable)
            .src_color_blend_factor(self.blend_src_color.into())
            .dst_color_blend_factor(self.blend_dst_color.into())
            .color_blend_op(self.blend_color_op.into())
            .src_alpha_blend_factor(self.blend_src_alpha.into())
            .dst_alpha_blend_factor(self.blend_dst_alpha.into())
            .alpha_blend_op(self.blend_alpha_op.into());

        let color_blend_attachments = vec![color_blend_attachment];

        let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let stencil_front = self.stencil_front.unwrap_or_default();
        let stencil_back = self.stencil_back.unwrap_or_default();

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(self.depth_test)
            .depth_write_enable(self.depth_write)
            .depth_compare_op(self.depth_compare_op.into())
            .depth_bounds_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0)
            .stencil_test_enable(self.stencil_test_enable)
            .front(stencil_front)
            .back(stencil_back);

        let dynamic_states_vk: Vec<vk::DynamicState> =
            self.dynamic_states.iter().map(|s| (*s).into()).collect();
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states_vk);

        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&self.descriptor_layouts)
            .push_constant_ranges(&self.push_constant_ranges);

        let pipeline_layout = unsafe {
            self.context
                .device
                .create_pipeline_layout(&pipeline_layout_info, None)
        }
        .map_err(PipelineError::LayoutCreationFailed)?;

        // For dynamic rendering (Vulkan 1.3), we need to add VkPipelineRenderingCreateInfo
        // to the pNext chain when render_pass is null
        // Declare color_formats outside the if block so it lives long enough
        let mut color_formats = Vec::new();
        let mut rendering_create_info = if vk_render_pass == vk::RenderPass::null()
            && (self.color_format.is_some() || self.depth_format.is_some())
        {
            if let Some(fmt) = self.color_format {
                color_formats.push(fmt);
            }

            let depth_fmt = self.depth_format.unwrap_or(vk::Format::UNDEFINED);
            let stencil_fmt = if depth_fmt == vk::Format::D32_SFLOAT_S8_UINT
                || depth_fmt == vk::Format::D24_UNORM_S8_UINT
            {
                depth_fmt
            } else {
                vk::Format::UNDEFINED
            };

            Some(
                vk::PipelineRenderingCreateInfo::default()
                    .color_attachment_formats(&color_formats)
                    .depth_attachment_format(depth_fmt)
                    .stencil_attachment_format(stencil_fmt),
            )
        } else {
            None
        };

        let create_info = if let Some(ref mut rendering_info) = rendering_create_info {
            vk::GraphicsPipelineCreateInfo::default()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .depth_stencil_state(&depth_stencil_state)
                .rasterization_state(&rasterizer)
                .multisample_state(&multisampling)
                .color_blend_state(&color_blending)
                .dynamic_state(&dynamic_state)
                .layout(pipeline_layout)
                .render_pass(vk_render_pass)
                .subpass(0)
                .push_next(rendering_info)
        } else {
            vk::GraphicsPipelineCreateInfo::default()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .depth_stencil_state(&depth_stencil_state)
                .rasterization_state(&rasterizer)
                .multisample_state(&multisampling)
                .color_blend_state(&color_blending)
                .dynamic_state(&dynamic_state)
                .layout(pipeline_layout)
                .render_pass(vk_render_pass)
                .subpass(0)
        };

        let pipeline = unsafe {
            self.context.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[create_info],
                None,
            )
        }
        .map_err(|e| PipelineError::CreationFailed(e.1))?[0];

        // Clone descriptor layouts for storage in the pipeline
        let descriptor_set_layouts = self.descriptor_layouts.clone();

        Ok(Pipeline {
            handle: Some(pipeline),
            layout: Some(pipeline_layout),
            device: self.context.device.clone(),
            descriptor_set_layouts,
        })
    }

    /// Build a pipeline for dynamic rendering (Vulkan 1.3).
    ///
    /// This is a convenience method that creates a pipeline without a render pass,
    /// suitable for use with dynamic rendering. The color and depth formats must
    /// be set via `with_rendering_formats()` before calling this method.
    pub fn build_dynamic(self) -> Result<Pipeline, PipelineError> {
        self.build(VkRenderPass::from(vk::RenderPass::null()))
    }
}

pub struct Pipeline {
    handle: Option<vk::Pipeline>,
    layout: Option<vk::PipelineLayout>,
    device: ash::Device,
    /// Descriptor set layouts used when creating this pipeline.
    /// These must be used when allocating descriptor sets for this pipeline.
    descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
}

impl Pipeline {
    /// Get the raw Vulkan pipeline handle.
    pub fn vk_pipeline(&self) -> vk::Pipeline {
        self.handle.unwrap_or(vk::Pipeline::null())
    }

    /// Get the raw Vulkan pipeline layout.
    pub fn vk_layout(&self) -> vk::PipelineLayout {
        self.layout.unwrap_or(vk::PipelineLayout::null())
    }

    /// Get the descriptor set layouts used when creating this pipeline.
    /// These must be used when allocating descriptor sets for this pipeline.
    pub fn descriptor_set_layouts(&self) -> &[vk::DescriptorSetLayout] {
        &self.descriptor_set_layouts
    }

    /// Destroy the pipeline and layout.
    ///
    /// Uses `take()` to prevent double-free when called explicitly
    /// before Drop runs. Safe to call multiple times.
    pub fn destroy(&mut self) {
        if let Some(handle) = self.handle.take() {
            unsafe {
                self.device.destroy_pipeline(handle, None);
            }
        }
        if let Some(layout) = self.layout.take() {
            unsafe {
                self.device.destroy_pipeline_layout(layout, None);
            }
        }
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[derive(Debug)]
pub enum PipelineError {
    MissingVertexShader,
    MissingFragmentShader,
    LayoutCreationFailed(vk::Result),
    CreationFailed(vk::Result),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVertexShader => write!(f, "Vertex shader not provided"),
            Self::MissingFragmentShader => write!(f, "Fragment shader not provided"),
            Self::LayoutCreationFailed(e) => write!(f, "Failed to create pipeline layout: {:?}", e),
            Self::CreationFailed(e) => write!(f, "Failed to create graphics pipeline: {:?}", e),
        }
    }
}

impl std::error::Error for PipelineError {}
