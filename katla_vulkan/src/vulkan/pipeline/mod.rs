pub mod builder;
pub mod materialbuilder;
pub mod shadermodule;

pub use builder::*;
pub use materialbuilder::*;
pub use shadermodule::*;

use ash::vk;
use gpu_allocator::vulkan::Allocation;
use std::rc::Rc;

use super::context::VulkanContext;

pub struct UniformBuffer {
    allocation: Allocation,
    buffer: vk::Buffer,
    buf_size: vk::DeviceSize,
}

#[derive(Clone)]
pub struct ImageInfo {
    pub image_view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub is_updated: bool,
    image_info: Vec<vk::DescriptorImageInfo>,
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
}

impl ImageInfo {
    pub fn new(image_view: vk::ImageView, sampler: vk::Sampler) -> Self {
        Self {
            image_view,
            sampler,
            is_updated: false,
            image_info: vec![vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(image_view)
                .sampler(sampler)],
        }
    }

    fn update_once(&self, set: vk::DescriptorSet, binding: u32) -> vk::WriteDescriptorSet<'_> {
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&self.image_info)
    }
}

impl UniformHandle {
    pub fn new(context: &VulkanContext, desc_layout: &vk::DescriptorSetLayout) -> Self {
        let mut uniform_descs = vec![];
        for _ in 0..2 {
            let uniform_desc = Self::create_descriptor_sets(context, desc_layout);
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
            descr.image_info = Some(image_info.clone());
        }
    }

    pub fn update_buffer(&mut self, context: &VulkanContext, data: &[u8]) {
        self.descriptors[self.next_update_index].update_buffer(context, data);

        self.next_bind_index = self.next_update_index;
        self.next_update_index = (self.next_update_index + 1) % self.descriptors.len();
    }

    pub fn next_descriptor(&self) -> &UniformDescriptor {
        &self.descriptors[self.next_bind_index]
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

        let create_info = vk::BufferCreateInfo::default()
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
            vk::DescriptorPoolSize::default()
                .descriptor_count(1)
                .ty(vk::DescriptorType::UNIFORM_BUFFER),
            vk::DescriptorPoolSize::default()
                .descriptor_count(1)
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        ];
        let desc_pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(desc_pool_sizes)
            .max_sets(1);
        let desc_pool =
            unsafe { context.device.create_descriptor_pool(&desc_pool_info, None) }.unwrap();

        let desc_layouts = &[*desc_layout];
        let desc_info = vk::DescriptorSetAllocateInfo::default()
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

            let buf_info = [vk::DescriptorBufferInfo::default()
                .buffer(uniform_buffer.buffer)
                .offset(0)
                .range(data_size)];
            let mut desc_writes = vec![];
            desc_writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(self.desc_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buf_info),
            );
            if let Some(image_info) = &mut self.image_info {
                if !image_info.is_updated {
                    image_info.is_updated = true;
                    let write_set = image_info.update_once(self.desc_set, 1);
                    desc_writes.push(write_set);
                }
            }

            unsafe {
                context
                    .device
                    .update_descriptor_sets(desc_writes.as_slice(), &[])
            };
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

pub struct DescriptorLayoutBuilder {
    bindings: Vec<vk::DescriptorSetLayoutBinding<'static>>,
}

impl DescriptorLayoutBuilder {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub fn add_binding(
        mut self,
        binding: u32,
        descriptor_type: vk::DescriptorType,
        stage_flags: vk::ShaderStageFlags,
        count: u32,
    ) -> Self {
        self.bindings.push(vk::DescriptorSetLayoutBinding {
            binding,
            descriptor_type,
            descriptor_count: count,
            stage_flags,
            p_immutable_samplers: std::ptr::null(),
            _marker: std::marker::PhantomData,
        });
        self
    }

    pub fn build(&self, device: &ash::Device) -> Result<vk::DescriptorSetLayout, vk::Result> {
        let create_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&self.bindings);
        unsafe { device.create_descriptor_set_layout(&create_info, None) }
    }
}

impl Default for DescriptorLayoutBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct MaterialPipeline {
    pub pipeline: Pipeline,
    pub uniform: UniformHandle,
    pub desc_layout: vk::DescriptorSetLayout,
    context: Rc<VulkanContext>,
}

impl MaterialPipeline {
    pub fn new(
        pipeline: Pipeline,
        desc_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        let uniform = UniformHandle::new(&context, &desc_layout);
        Self {
            pipeline,
            uniform,
            desc_layout,
            context,
        }
    }

    pub fn bind(&self, command_buffer: vk::CommandBuffer) {
        unsafe {
            self.context.device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline.handle,
            );

            self.context.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline.layout,
                0,
                &[self.uniform.next_descriptor().desc_set],
                &[],
            );
        }
    }

    pub fn update_buffer(&mut self, data: &[u8]) {
        self.uniform.update_buffer(&self.context, data);
    }

    pub fn destroy(&mut self) {
        self.uniform.destroy(&self.context);
        self.pipeline.destroy();
        unsafe {
            self.context
                .device
                .destroy_descriptor_set_layout(self.desc_layout, None);
        }
    }
}
