use crate::rendering::vertextypes::*;
use ash::{util::read_spv, vk};
use gpu_allocator::vulkan::Allocation;

use std::{ffi::CString, io::Cursor, sync::Arc};

use super::context::VulkanContext;
//TODO: A more flexible shader system
const SHADER_VERT: &[u8] = include_bytes!("../../../resources/shaders/model_pbr.vert.spv");
const SHADER_FRAG: &[u8] = include_bytes!("../../../resources/shaders/model.frag.spv");

//TODO: Make these traits more usable and dynamic for a pipeline.
pub trait UpdateOnce {
    fn update_once(&self, set: vk::DescriptorSet, binding: u32) -> vk::WriteDescriptorSet;
}
pub trait UpdateAlways {
    fn update_always(&self, set: vk::DescriptorSet, binding: u32) -> vk::WriteDescriptorSet;
}

pub struct RenderPipeline {
    context: Arc<VulkanContext>,
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    pub uniform: UniformHandle,
    pub desc_layout: vk::DescriptorSetLayout,
    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,
}

pub struct UniformBuffer {
    allocation: Allocation,
    buffer: vk::Buffer,
    buf_size: vk::DeviceSize,
}

#[derive(Clone, Copy)]
pub struct ImageInfo {
    pub image_view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub is_updated: bool,
}

pub struct UniformHandle {
    next_bind_index: usize,
    next_update_index: usize,
    descriptors: Vec<UniformDescriptor>,
}

pub struct UniformDescriptor {
    pub desc_set: vk::DescriptorSet,
    pub desc_pool: vk::DescriptorPool,
    pub uniform_buffer: Option<UniformBuffer>,
    pub image_info: Option<ImageInfo>,
    pub static_descriptors: Vec<Box<dyn UpdateOnce>>,
}

impl ImageInfo {
    pub fn new(image_view: vk::ImageView, sampler: vk::Sampler) -> Self {
        Self {
            image_view,
            sampler,
            is_updated: false,
        }
    }
}

impl UpdateOnce for ImageInfo {
    fn update_once(&self, set: vk::DescriptorSet, binding: u32) -> vk::WriteDescriptorSet {
        let mut image_infos = vec![];
        image_infos.push(
            vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(self.image_view)
                .sampler(self.sampler)
                .build(),
        );
        vk::WriteDescriptorSet::builder()
            .dst_set(set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(image_infos.as_slice())
            .build()
    }
}

impl UniformHandle {
    pub fn new(
        num_buffered_frames: usize,
        context: &VulkanContext,
        desc_layout: &vk::DescriptorSetLayout,
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
            //TODO: Something like this maybe? -
            // descr.static_descriptors.push(Box::new(image_info));
        }
    }

    pub fn update_buffer(&mut self, context: &VulkanContext, data: &[u8]) {
        self.descriptors[self.next_update_index].update_buffer(context, data);

        self.next_bind_index = self.next_update_index;
        self.next_update_index = (self.next_update_index + 1) % self.descriptors.len();
    }

    pub fn next_descriptor(&self) -> &UniformDescriptor {
        let out_descr = &self.descriptors[self.next_bind_index];
        out_descr
    }

    pub fn destroy(&mut self, context: &VulkanContext) {
        for desc in &mut self.descriptors {
            desc.destroy(context);
        }
    }

    fn create_descriptor_sets(
        context: &VulkanContext,
        desc_layout: &vk::DescriptorSetLayout,
    ) -> UniformDescriptor {
        let data_size = 4 * 16 * 3 as vk::DeviceSize;

        let create_info = vk::BufferCreateInfo::builder()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .size(data_size);

        let (buffer, allocation) =
            context.allocate_buffer(&create_info, gpu_allocator::MemoryLocation::CpuToGpu);
        let uniform_buffer = Some(UniformBuffer {
            allocation,
            buffer,
            buf_size: data_size,
        });

        let desc_pool_sizes = &[
            vk::DescriptorPoolSize::builder()
                .descriptor_count(1)
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .build(),
            vk::DescriptorPoolSize::builder()
                .descriptor_count(1)
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .build(),
        ];
        let desc_pool_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(desc_pool_sizes)
            .max_sets(1);
        let desc_pool =
            unsafe { context.device.create_descriptor_pool(&desc_pool_info, None) }.unwrap();

        let desc_layouts = &[desc_layout.clone()];
        let desc_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(desc_pool)
            .set_layouts(desc_layouts);
        let desc_set = unsafe { context.device.allocate_descriptor_sets(&desc_info) }.unwrap()[0];

        let image_info = None;

        UniformDescriptor {
            desc_set,
            desc_pool,
            uniform_buffer,
            image_info,
            static_descriptors: vec![],
        }
    }
}

impl UniformDescriptor {
    //TODO: Uniform buffer updates:
    pub fn update_buffer(&mut self, context: &VulkanContext, data: &[u8]) {
        if let Some(uniform_buffer) = &self.uniform_buffer {
            let data_size = std::mem::size_of_val(data) as vk::DeviceSize;
            if uniform_buffer.buf_size < data_size {
                panic!(
                    "Too little memory allocated for buffer of size {}",
                    data_size
                );
            }

            let mapped_data = context.map_buffer(&uniform_buffer.allocation);
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), mapped_data, data_size as usize);
            }

            let buf_info = [vk::DescriptorBufferInfo::builder()
                .buffer(uniform_buffer.buffer)
                .offset(0)
                .range(data_size)
                .build()];
            let mut desc_writes = vec![];
            desc_writes.push(
                vk::WriteDescriptorSet::builder()
                    .dst_set(self.desc_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buf_info)
                    .build(),
            );
            if let Some(image_info) = &mut self.image_info {
                if !image_info.is_updated {
                    let write_set = image_info.update_once(self.desc_set, 1);
                    desc_writes.push(write_set);
                    image_info.is_updated = true;
                }
            } else {
                println!("No descriptor image to update!!!");
            }

            unsafe {
                context
                    .device
                    .update_descriptor_sets(desc_writes.as_slice(), &[])
            };
        } else {
            println!("No descriptor buffer to update!!!");
        }
    }

    pub fn destroy(&mut self, context: &VulkanContext) {
        if self.uniform_buffer.is_some() {
            let buffer = self.uniform_buffer.take().unwrap();
            context.free_buffer(buffer.buffer, buffer.allocation);
        }
        unsafe {
            context.device.destroy_descriptor_pool(self.desc_pool, None);
        }
    }
}

impl RenderPipeline {
    pub fn new<BindingType: VertexBinding>(
        context: Arc<VulkanContext>,
        render_pass: vk::RenderPass,
        num_buffered_frames: usize,
    ) -> Self {
        let entry_point = CString::new("main").unwrap();
        let mut vertex_spv_file = Cursor::new(SHADER_VERT);
        let vert_decoded = read_spv(&mut vertex_spv_file).unwrap();
        let create_info = vk::ShaderModuleCreateInfo::builder().code(&vert_decoded);
        let shader_vert =
            unsafe { context.device.create_shader_module(&create_info, None) }.unwrap();

        let mut frag_spv_file = Cursor::new(SHADER_FRAG);
        let frag_decoded = read_spv(&mut frag_spv_file).unwrap();
        let create_info = vk::ShaderModuleCreateInfo::builder().code(&frag_decoded);
        let shader_frag =
            unsafe { context.device.create_shader_module(&create_info, None) }.unwrap();

        let shader_stages = vec![
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(shader_vert)
                .name(&entry_point)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(shader_frag)
                .name(&entry_point)
                .build(),
        ];
        //TODO: Descripitor sets
        let desc_layout_bindings = &[
            vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
        ];
        let desc_layout_info =
            vk::DescriptorSetLayoutCreateInfo::builder().bindings(desc_layout_bindings);
        let desc_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&desc_layout_info, None)
        }
        .unwrap();

        let uniform = UniformHandle::new(num_buffered_frames, &context, &desc_layout);

        let pipeline_layout_desc_layouts = &[desc_layout];

        let create_info =
            vk::PipelineLayoutCreateInfo::builder().set_layouts(pipeline_layout_desc_layouts);
        let pipeline_layout =
            unsafe { context.device.create_pipeline_layout(&create_info, None) }.unwrap();

        let vertex_binding_desc = [BindingType::get_binding_desc(0).build()];
        let vertex_attrib_descs = BindingType::get_attribute_desc(0);
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&vertex_binding_desc)
            .vertex_attribute_descriptions(vertex_attrib_descs.as_slice());

        // https://vulkan-tutorial.com/Drawing_a_triangle/Graphics_pipeline_basics/Fixed_functions
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewport_count(1)
            .scissor_count(1);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::builder()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::CLOCKWISE);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let color_blend_attachments = vec![vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            )
            .blend_enable(false)
            .build()];

        let color_blending = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS)
            .depth_bounds_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0)
            .stencil_test_enable(false);
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

        let create_info = vk::GraphicsPipelineCreateInfo::builder()
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
            context.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[create_info.build()],
                None,
            )
        }
        .unwrap()[0];

        RenderPipeline {
            context,
            pipeline,
            pipeline_layout,
            desc_layout,
            uniform,
            vert_module: shader_vert,
            frag_module: shader_frag,
        }
    }

    pub fn update_buffer(&mut self, data: &[u8]) {
        self.uniform.update_buffer(&self.context, data);
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.context.device.destroy_pipeline(self.pipeline, None);
            self.context
                .device
                .destroy_shader_module(self.vert_module, None);
            self.context
                .device
                .destroy_shader_module(self.frag_module, None);
            self.uniform.destroy(&self.context);
            self.context
                .device
                .destroy_descriptor_set_layout(self.desc_layout, None);
            self.context
                .device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}
