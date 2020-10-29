use crate::rendering::vertextypes::*;
use erupt::{
    utils,
    utils::allocator::{Allocation, Allocator, MemoryTypeFinder},
    vk1_0::*,
    DeviceLoader,
};

use std::ffi::CString;

use super::context::VulkanContext;
const SHADER_VERT: &[u8] = include_bytes!("../../resources/shaders/model_pbr.vert.spv");
const SHADER_FRAG: &[u8] = include_bytes!("../../resources/shaders/model.frag.spv");

pub struct RenderPipeline {
    pub pipeline: Pipeline,
    pub pipeline_layout: PipelineLayout,
    pub uniform: UniformHandle,
    pub desc_layout: DescriptorSetLayout,
    vert_module: ShaderModule,
    frag_module: ShaderModule,
}

pub struct UniformBuffer {
    buffer: Allocation<Buffer>,
    buf_size: DeviceSize,
}

#[derive(Clone, Copy)]
pub struct ImageInfo {
    pub image_view: ImageView,
    pub sampler: Sampler,
}

pub struct UniformHandle {
    next_bind_index: usize,
    next_update_index: usize,
    descriptors: Vec<UniformDescriptor>,
}

pub struct UniformDescriptor {
    pub desc_set: DescriptorSet,
    pub desc_pool: DescriptorPool,
    pub uniform_buffer: Option<UniformBuffer>,
    pub image_info: Option<ImageInfo>,
}

impl UniformHandle {
    pub fn new(
        num_buffered_frames: usize,
        context: &VulkanContext,
        desc_layout: &DescriptorSetLayout,
    ) -> Self {
        let mut uniform_descs = vec![];
        for _ in 0..num_buffered_frames {
            let uniform_desc = Self::create_descriptor_sets(context, &desc_layout);
            uniform_descs.push(uniform_desc);
        }

        Self {
            next_bind_index: 0,
            next_update_index: 0,
            descriptors: uniform_descs,
        }
    }

    pub fn add_image_info(&mut self, image_info: ImageInfo) {
        for descr in &mut self.descriptors {
            descr.image_info = Some(image_info);
        }
    }

    pub fn update_buffer(&mut self, device: &DeviceLoader, data: &[u8]) {
        self.descriptors[self.next_update_index].update_buffer(device, data);
        self.next_bind_index = self.next_update_index;
        self.next_update_index = (self.next_update_index + 1) % self.descriptors.len();
    }

    pub fn next_descriptor(&self) -> &UniformDescriptor {
        // println!("Next update index is: {}", self.next_update_index);
        // println!("Next bind index is: {}", self.next_bind_index);
        let out_descr = &self.descriptors[self.next_bind_index];
        out_descr
    }

    pub fn destroy(&mut self, device: &DeviceLoader, allocator: &mut Allocator) {
        for desc in &mut self.descriptors {
            desc.destroy(device, allocator);
        }
    }

    fn create_descriptor_sets(
        context: &VulkanContext,
        desc_layout: &DescriptorSetLayout,
    ) -> UniformDescriptor {
        let data_size = 4 * 16 * 3 as DeviceSize;

        let create_info = BufferCreateInfoBuilder::new()
            .sharing_mode(SharingMode::EXCLUSIVE)
            .usage(BufferUsageFlags::UNIFORM_BUFFER)
            .size(data_size);

        let buffer = context
            .allocate_object(
                unsafe { context.device.create_buffer(&create_info, None, None) }.unwrap(),
                MemoryTypeFinder::dynamic(),
            )
            .unwrap();
        let uniform_buffer = Some(UniformBuffer {
            buffer: buffer,
            buf_size: data_size,
        });

        let desc_pool_sizes = &[
            DescriptorPoolSizeBuilder::new()
                .descriptor_count(1)
                ._type(DescriptorType::UNIFORM_BUFFER),
            DescriptorPoolSizeBuilder::new()
                .descriptor_count(1)
                ._type(DescriptorType::COMBINED_IMAGE_SAMPLER),
        ];
        let desc_pool_info = DescriptorPoolCreateInfoBuilder::new()
            .pool_sizes(desc_pool_sizes)
            .max_sets(1);
        let desc_pool = unsafe {
            context
                .device
                .create_descriptor_pool(&desc_pool_info, None, None)
        }
        .unwrap();

        let desc_layouts = &[desc_layout.clone()];
        let desc_info = DescriptorSetAllocateInfoBuilder::new()
            .descriptor_pool(desc_pool)
            .set_layouts(desc_layouts);
        let desc_set = unsafe { context.device.allocate_descriptor_sets(&desc_info) }.unwrap()[0];

        let image_info = None;

        UniformDescriptor {
            desc_set,
            desc_pool,
            uniform_buffer,
            image_info,
        }
    }
}

impl UniformDescriptor {
    pub fn update_buffer(&mut self, device: &DeviceLoader, data: &[u8]) {
        if let Some(buffer) = &self.uniform_buffer {
            let data_size = std::mem::size_of_val(data) as DeviceSize;
            if buffer.buf_size < data_size {
                panic!(
                    "Too little memory allocated for buffer of size {}",
                    data_size
                );
            }
            //This is a bit awkward.. Something finicky within erupt?
            let range = ..buffer.buffer.region().start + data_size;

            let mut map = buffer.buffer.map(&device, range).unwrap();
            map.import(data);
            map.unmap(&device).unwrap();
            let buf_info = [DescriptorBufferInfoBuilder::new()
                .buffer(*buffer.buffer.object())
                .offset(0)
                .range(data_size)];
            let mut desc_writes = vec![];
            desc_writes.push(
                WriteDescriptorSetBuilder::new()
                    .dst_set(self.desc_set)
                    .dst_binding(0)
                    .descriptor_type(DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buf_info),
            );
            let mut image_infos = vec![];
            if let Some(image_info) = &self.image_info {
                image_infos.push(
                    DescriptorImageInfoBuilder::new()
                        .image_layout(ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .image_view(image_info.image_view)
                        .sampler(image_info.sampler),
                );
                desc_writes.push(
                    WriteDescriptorSetBuilder::new()
                        .dst_set(self.desc_set)
                        .dst_binding(1)
                        .descriptor_type(DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(image_infos.as_slice()),
                );
            } else {
                println!("No descriptor image to update!!!");
            }
            unsafe { device.update_descriptor_sets(desc_writes.as_slice(), &[]) };
        } else {
            println!("No descriptor buffer to update!!!");
        }
    }

    pub fn destroy(&mut self, device: &DeviceLoader, allocator: &mut Allocator) {
        unsafe {
            if self.uniform_buffer.is_some() {
                let buffer = self.uniform_buffer.take().unwrap();
                allocator.free(device, buffer.buffer);
            }
            device.destroy_descriptor_pool(Some(self.desc_pool), None);
        }
    }
}

impl RenderPipeline {
    pub fn new<BindingType: VertexBinding>(
        context: &VulkanContext,
        render_pass: RenderPass,
        num_buffered_frames: usize,
    ) -> Self {
        let entry_point = CString::new("main").unwrap();

        let vert_decoded = utils::decode_spv(SHADER_VERT).unwrap();
        let create_info = ShaderModuleCreateInfoBuilder::new().code(&vert_decoded);
        let shader_vert = unsafe {
            context
                .device
                .create_shader_module(&create_info, None, None)
        }
        .unwrap();

        let frag_decoded = utils::decode_spv(SHADER_FRAG).unwrap();
        let create_info = ShaderModuleCreateInfoBuilder::new().code(&frag_decoded);
        let shader_frag = unsafe {
            context
                .device
                .create_shader_module(&create_info, None, None)
        }
        .unwrap();

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
        //TODO: Descrpitor sets
        let desc_layout_bindings = &[
            DescriptorSetLayoutBindingBuilder::new()
                .binding(0)
                .descriptor_count(1)
                .descriptor_type(DescriptorType::UNIFORM_BUFFER)
                .stage_flags(ShaderStageFlags::VERTEX | ShaderStageFlags::FRAGMENT),
            DescriptorSetLayoutBindingBuilder::new()
                .binding(1)
                .descriptor_count(1)
                .descriptor_type(DescriptorType::COMBINED_IMAGE_SAMPLER)
                .stage_flags(ShaderStageFlags::FRAGMENT),
        ];
        let desc_layout_info =
            DescriptorSetLayoutCreateInfoBuilder::new().bindings(desc_layout_bindings);
        let desc_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&desc_layout_info, None, None)
        }
        .unwrap();

        let uniform = UniformHandle::new(num_buffered_frames, context, &desc_layout);

        let pipeline_layout_desc_layouts = &[desc_layout];

        let create_info =
            PipelineLayoutCreateInfoBuilder::new().set_layouts(pipeline_layout_desc_layouts);
        let pipeline_layout = unsafe {
            context
                .device
                .create_pipeline_layout(&create_info, None, None)
        }
        .unwrap();

        let vertex_binding_desc = vec![BindingType::get_binding_desc(0)];
        let vertex_attrib_descs = BindingType::get_attribute_desc(0);
        let vertex_input = PipelineVertexInputStateCreateInfoBuilder::new()
            .vertex_binding_descriptions(vertex_binding_desc.as_slice())
            .vertex_attribute_descriptions(vertex_attrib_descs.as_slice());

        // https://vulkan-tutorial.com/Drawing_a_triangle/Graphics_pipeline_basics/Fixed_functions
        let input_assembly = PipelineInputAssemblyStateCreateInfoBuilder::new()
            .topology(PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewport_state = PipelineViewportStateCreateInfoBuilder::new()
            .viewport_count(1)
            .scissor_count(1);

        let rasterizer = PipelineRasterizationStateCreateInfoBuilder::new()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(CullModeFlags::BACK)
            .front_face(FrontFace::CLOCKWISE);

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

        let depth_stencil_state = PipelineDepthStencilStateCreateInfoBuilder::new()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(CompareOp::LESS)
            .depth_bounds_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0)
            .stencil_test_enable(false);
        let dynamic_state = PipelineDynamicStateCreateInfoBuilder::new()
            .dynamic_states(&[DynamicState::VIEWPORT, DynamicState::SCISSOR]);

        let create_info = GraphicsPipelineCreateInfoBuilder::new()
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
            context
                .device
                .create_graphics_pipelines(None, &[create_info], None)
        }
        .unwrap()[0];

        RenderPipeline {
            pipeline,
            pipeline_layout,
            desc_layout,
            uniform,
            vert_module: shader_vert,
            frag_module: shader_frag,
        }
    }

    pub fn destroy(&mut self, device: &DeviceLoader, allocator: &mut Allocator) {
        unsafe {
            device.destroy_pipeline(Some(self.pipeline), None);
            device.destroy_shader_module(Some(self.vert_module), None);
            device.destroy_shader_module(Some(self.frag_module), None);
            self.uniform.destroy(device, allocator);
            device.destroy_descriptor_set_layout(Some(self.desc_layout), None);
            device.destroy_pipeline_layout(Some(self.pipeline_layout), None);
        }
    }
}
