use std::{ffi::CString, rc::Rc};

use ash::vk;

use crate::VulkanContext;

pub struct PipelineBuilder {
    context: Rc<VulkanContext>,
    vertex_shader: Option<vk::ShaderModule>,
    fragment_shader: Option<vk::ShaderModule>,
    vertex_shader_entry_point: CString,
    fragment_shader_entry_point: CString,
    vertex_bindings: Vec<vk::VertexInputBindingDescription>,
    vertex_attributes: Vec<vk::VertexInputAttributeDescription>,
    topology: vk::PrimitiveTopology,
    polygon_mode: vk::PolygonMode,
    cull_mode: vk::CullModeFlags,
    front_face: vk::FrontFace,
    line_width: f32,
    depth_test: bool,
    depth_write: bool,
    depth_compare_op: vk::CompareOp,
    blend_enable: bool,
    blend_src_color: vk::BlendFactor,
    blend_dst_color: vk::BlendFactor,
    blend_color_op: vk::BlendOp,
    blend_src_alpha: vk::BlendFactor,
    blend_dst_alpha: vk::BlendFactor,
    blend_alpha_op: vk::BlendOp,
    descriptor_layouts: Vec<vk::DescriptorSetLayout>,
    push_constant_ranges: Vec<vk::PushConstantRange>,
    dynamic_states: Vec<vk::DynamicState>,
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
            topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            polygon_mode: vk::PolygonMode::FILL,
            cull_mode: vk::CullModeFlags::BACK,
            front_face: vk::FrontFace::CLOCKWISE,
            line_width: 1.0,
            depth_test: true,
            depth_write: true,
            depth_compare_op: vk::CompareOp::LESS,
            blend_enable: false,
            blend_src_color: vk::BlendFactor::SRC_ALPHA,
            blend_dst_color: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            blend_color_op: vk::BlendOp::ADD,
            blend_src_alpha: vk::BlendFactor::ONE,
            blend_dst_alpha: vk::BlendFactor::ZERO,
            blend_alpha_op: vk::BlendOp::ADD,
            descriptor_layouts: Vec::new(),
            push_constant_ranges: Vec::new(),
            dynamic_states: vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR],
        }
    }

    pub fn with_shaders(mut self, vert: vk::ShaderModule, frag: vk::ShaderModule) -> Self {
        self.vertex_shader = Some(vert);
        self.fragment_shader = Some(frag);
        self
    }

    pub fn with_entry_points(mut self, vertex: CString, fragment: CString) -> Self {
        self.vertex_shader_entry_point = vertex;
        self.fragment_shader_entry_point = fragment;
        self
    }

    pub fn with_vertex_input(
        mut self,
        bindings: Vec<vk::VertexInputBindingDescription>,
        attributes: Vec<vk::VertexInputAttributeDescription>,
    ) -> Self {
        self.vertex_bindings = bindings;
        self.vertex_attributes = attributes;
        self
    }

    pub fn with_topology(mut self, topology: vk::PrimitiveTopology) -> Self {
        self.topology = topology;
        self
    }

    pub fn with_polygon_mode(mut self, mode: vk::PolygonMode) -> Self {
        self.polygon_mode = mode;
        self
    }

    pub fn with_depth_test(mut self, enable: bool, write: bool, op: vk::CompareOp) -> Self {
        self.depth_test = enable;
        self.depth_write = write;
        self.depth_compare_op = op;
        self
    }

    pub fn with_cull_mode(mut self, mode: vk::CullModeFlags, front: vk::FrontFace) -> Self {
        self.cull_mode = mode;
        self.front_face = front;
        self
    }

    pub fn with_line_width(mut self, width: f32) -> Self {
        self.line_width = width;
        self
    }

    pub fn with_blending(
        mut self,
        enable: bool,
        src: vk::BlendFactor,
        dst: vk::BlendFactor,
    ) -> Self {
        self.blend_enable = enable;
        self.blend_src_color = src;
        self.blend_dst_color = dst;
        self.blend_src_alpha = vk::BlendFactor::ONE;
        self.blend_dst_alpha = vk::BlendFactor::ZERO;
        self
    }

    pub fn with_blending_advanced(
        mut self,
        enable: bool,
        src_color: vk::BlendFactor,
        dst_color: vk::BlendFactor,
        color_op: vk::BlendOp,
        src_alpha: vk::BlendFactor,
        dst_alpha: vk::BlendFactor,
        alpha_op: vk::BlendOp,
    ) -> Self {
        self.blend_enable = enable;
        self.blend_src_color = src_color;
        self.blend_dst_color = dst_color;
        self.blend_color_op = color_op;
        self.blend_src_alpha = src_alpha;
        self.blend_dst_alpha = dst_alpha;
        self.blend_alpha_op = alpha_op;
        self
    }

    pub fn with_alpha_blending(mut self) -> Self {
        self.blend_enable = true;
        self.blend_src_color = vk::BlendFactor::SRC_ALPHA;
        self.blend_dst_color = vk::BlendFactor::ONE_MINUS_SRC_ALPHA;
        self.blend_color_op = vk::BlendOp::ADD;
        self.blend_src_alpha = vk::BlendFactor::ONE;
        self.blend_dst_alpha = vk::BlendFactor::ZERO;
        self.blend_alpha_op = vk::BlendOp::ADD;
        self
    }

    pub fn with_additive_blending(mut self) -> Self {
        self.blend_enable = true;
        self.blend_src_color = vk::BlendFactor::SRC_ALPHA;
        self.blend_dst_color = vk::BlendFactor::ONE;
        self.blend_color_op = vk::BlendOp::ADD;
        self.blend_src_alpha = vk::BlendFactor::ONE;
        self.blend_dst_alpha = vk::BlendFactor::ONE;
        self.blend_alpha_op = vk::BlendOp::ADD;
        self
    }

    pub fn with_descriptor_layouts(mut self, layouts: Vec<vk::DescriptorSetLayout>) -> Self {
        self.descriptor_layouts = layouts;
        self
    }

    pub fn with_push_constants(mut self, ranges: Vec<vk::PushConstantRange>) -> Self {
        self.push_constant_ranges = ranges;
        self
    }

    pub fn add_push_constant_range(
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

    pub fn with_dynamic_states(mut self, states: Vec<vk::DynamicState>) -> Self {
        self.dynamic_states = states;
        self
    }

    pub fn build(self, render_pass: vk::RenderPass) -> Result<Pipeline, PipelineError> {
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
            .topology(self.topology)
            .primitive_restart_enable(false);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(self.polygon_mode)
            .line_width(self.line_width)
            .cull_mode(self.cull_mode)
            .front_face(self.front_face)
            .depth_bias_enable(false);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            )
            .blend_enable(self.blend_enable)
            .src_color_blend_factor(self.blend_src_color)
            .dst_color_blend_factor(self.blend_dst_color)
            .color_blend_op(self.blend_color_op)
            .src_alpha_blend_factor(self.blend_src_alpha)
            .dst_alpha_blend_factor(self.blend_dst_alpha)
            .alpha_blend_op(self.blend_alpha_op);

        let color_blend_attachments = vec![color_blend_attachment];

        let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(self.depth_test)
            .depth_write_enable(self.depth_write)
            .depth_compare_op(self.depth_compare_op)
            .depth_bounds_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0)
            .stencil_test_enable(false);

        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&self.dynamic_states);

        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&self.descriptor_layouts)
            .push_constant_ranges(&self.push_constant_ranges);

        let pipeline_layout = unsafe {
            self.context
                .device
                .create_pipeline_layout(&pipeline_layout_info, None)
        }
        .map_err(|e| PipelineError::LayoutCreationFailed(e))?;

        let create_info = vk::GraphicsPipelineCreateInfo::default()
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
            .render_pass(render_pass)
            .subpass(0);

        let pipeline = unsafe {
            self.context.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[create_info],
                None,
            )
        }
        .map_err(|e| PipelineError::CreationFailed(e.1))?[0];

        Ok(Pipeline {
            handle: pipeline,
            layout: pipeline_layout,
            device: self.context.device.clone(),
        })
    }
}

pub struct Pipeline {
    pub handle: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    device: ash::Device,
}

impl Pipeline {
    pub fn destroy(&self) {
        unsafe {
            self.device.destroy_pipeline(self.handle, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
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
