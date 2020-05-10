use crate::rendering::vertextypes::*;
use erupt::{extensions::khr_surface::*, utils, vk1_0::*, DeviceLoader};
use std::ffi::CString;

pub struct RenderPipeline {
    pub pipeline: Pipeline,
    pub pipeline_layout: PipelineLayout,
    pub desc_set: DescriptorSet,
    pub desc_layout: DescriptorSetLayout,
    pub desc_pool: DescriptorPool,
    vert_module: ShaderModule,
    frag_module: ShaderModule,
}

const SHADER_VERT: &[u8] = include_bytes!("../../resources/shaders/model_pos.vert.spv");
const SHADER_FRAG: &[u8] = include_bytes!("../../resources/shaders/model.frag.spv");

impl RenderPipeline {
    //Call with e.g. SingleBufferDefinition::new() as V

    fn create_descriptor_sets(
        device: &DeviceLoader,
    ) -> (DescriptorSet, DescriptorSetLayout, DescriptorPool) {
        let desc_pool_sizes = &[DescriptorPoolSizeBuilder::new()
            .descriptor_count(1)
            ._type(DescriptorType::UNIFORM_BUFFER)];
        let desc_pool_info = DescriptorPoolCreateInfoBuilder::new()
            .pool_sizes(desc_pool_sizes)
            .max_sets(1);
        let desc_pool =
            unsafe { device.create_descriptor_pool(&desc_pool_info, None, None) }.unwrap();

        let desc_layout_bindings = &[DescriptorSetLayoutBindingBuilder::new()
            .binding(0)
            .descriptor_count(1)
            .descriptor_type(DescriptorType::UNIFORM_BUFFER)
            .stage_flags(ShaderStageFlags::VERTEX | ShaderStageFlags::FRAGMENT)];
        let desc_layout_info =
            DescriptorSetLayoutCreateInfoBuilder::new().bindings(desc_layout_bindings);
        let desc_layout =
            unsafe { device.create_descriptor_set_layout(&desc_layout_info, None, None) }.unwrap();

        let desc_layouts = &[desc_layout];
        let desc_info = DescriptorSetAllocateInfoBuilder::new()
            .descriptor_pool(desc_pool)
            .set_layouts(desc_layouts);
        let desc_set = unsafe { device.allocate_descriptor_sets(&desc_info) }.unwrap()[0];
        (desc_set, desc_layout, desc_pool)
    }

    pub fn new(
        device: &DeviceLoader,
        render_pass: RenderPass,
        surface_caps: SurfaceCapabilitiesKHR,
    ) -> Self {
        let entry_point = CString::new("main").unwrap();

        let vert_decoded = utils::decode_spv(SHADER_VERT).unwrap();
        let create_info = ShaderModuleCreateInfoBuilder::new().code(&vert_decoded);
        let shader_vert = unsafe { device.create_shader_module(&create_info, None, None) }.unwrap();

        let frag_decoded = utils::decode_spv(SHADER_FRAG).unwrap();
        let create_info = ShaderModuleCreateInfoBuilder::new().code(&frag_decoded);
        let shader_frag = unsafe { device.create_shader_module(&create_info, None, None) }.unwrap();

        let shader_stages = vec![
            PipelineShaderStageCreateInfoBuilder::new()
                .stage(ShaderStageFlagBits::VERTEX)
                .module(shader_vert)
                .name(&entry_point),
            PipelineShaderStageCreateInfoBuilder::new()
                .stage(ShaderStageFlagBits::FRAGMENT)
                .module(shader_frag)
                .name(&entry_point),
        ];
        //TODO: Descrpitor sets!
        let (desc_set, desc_layout, desc_pool) = RenderPipeline::create_descriptor_sets(device);
        let pipeline_layout_desc_layouts = &[desc_layout];

        let create_info =
            PipelineLayoutCreateInfoBuilder::new().set_layouts(pipeline_layout_desc_layouts);
        let pipeline_layout =
            unsafe { device.create_pipeline_layout(&create_info, None, None) }.unwrap();

        let vertex_binding_desc = vec![VertexPosition::get_binding_desc(0)];
        let vertex_attrib_descs = VertexPosition::get_attribute_desc(0);
        let vertex_input = PipelineVertexInputStateCreateInfoBuilder::new()
            .vertex_binding_descriptions(vertex_binding_desc.as_slice())
            .vertex_attribute_descriptions(vertex_attrib_descs.as_slice());

        // https://vulkan-tutorial.com/Drawing_a_triangle/Graphics_pipeline_basics/Fixed_functions
        let input_assembly = PipelineInputAssemblyStateCreateInfoBuilder::new()
            .topology(PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewports = vec![ViewportBuilder::new()
            .x(0.0)
            .y(0.0)
            .width(surface_caps.current_extent.width as f32)
            .height(surface_caps.current_extent.height as f32)
            .min_depth(0.0)
            .max_depth(1.0)];
        let scissors = vec![Rect2DBuilder::new()
            .offset(Offset2D { x: 0, y: 0 })
            .extent(surface_caps.current_extent)];
        let viewport_state = PipelineViewportStateCreateInfoBuilder::new()
            .viewports(&viewports)
            .scissors(&scissors);

        let rasterizer = PipelineRasterizationStateCreateInfoBuilder::new()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(CullModeFlags::BACK)
            .front_face(FrontFace::CLOCKWISE)
            .depth_clamp_enable(false);

        let multisampling = PipelineMultisampleStateCreateInfoBuilder::new()
            .sample_shading_enable(false)
            .rasterization_samples(SampleCountFlagBits::_1);

        let color_blend_attachments = vec![PipelineColorBlendAttachmentStateBuilder::new()
            .color_write_mask(
                ColorComponentFlags::R
                    | ColorComponentFlags::G
                    | ColorComponentFlags::B
                    | ColorComponentFlags::A,
            )
            .blend_enable(false)];
        let color_blending = PipelineColorBlendStateCreateInfoBuilder::new()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let create_info = GraphicsPipelineCreateInfoBuilder::new()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blending)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0);

        let pipeline = unsafe {
            device.create_graphics_pipelines(PipelineCache::null(), &[create_info], None)
        }
        .unwrap()[0];

        RenderPipeline {
            pipeline,
            pipeline_layout,
            desc_set,
            desc_layout,
            desc_pool,
            vert_module: shader_vert,
            frag_module: shader_frag,
        }
    }

    pub fn destroy(&mut self, device: &DeviceLoader) {
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_shader_module(self.vert_module, None);
            device.destroy_shader_module(self.frag_module, None);
            device.destroy_descriptor_set_layout(self.desc_layout, None);
            device.destroy_descriptor_pool(self.desc_pool, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}
