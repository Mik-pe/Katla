use std::rc::Rc;

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc};
use log::info;

use crate::shadow::cascade::ShadowFrameData;
use crate::vulkan::context::VulkanContext;

fn create_buffer(
    context: &Rc<VulkanContext>,
    name: &str,
    size: u64,
    usage: vk::BufferUsageFlags,
    location: gpu_allocator::MemoryLocation,
) -> Result<(vk::Buffer, Allocation), String> {
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let buffer = unsafe {
        context
            .device
            .create_buffer(&buffer_info, None)
            .map_err(|e| format!("Failed to create buffer '{}': {:?}", name, e))?
    };

    let requirements = unsafe { context.device.get_buffer_memory_requirements(buffer) };

    let allocation = context
        .allocator
        .borrow_mut()
        .allocate(&AllocationCreateDesc {
            name,
            requirements,
            location,
            linear: true,
            allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
        })
        .map_err(|e| format!("Failed to allocate '{}': {}", name, e))?;

    unsafe {
        context
            .device
            .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
            .map_err(|e| format!("Failed to bind buffer '{}': {:?}", name, e))?;
    }

    Ok((buffer, allocation))
}

pub struct ShadowBuffers {
    context: Rc<VulkanContext>,

    shadow_data_buffer: vk::Buffer,
    shadow_data_allocation: Option<Allocation>,

    shadow_data_mapped_ptr: *mut u8,

    shadow_atlas_views: Vec<Option<vk::ImageView>>,

    shadow_sampler: Option<vk::Sampler>,

    destroyed: bool,
}

unsafe impl Send for ShadowBuffers {}
unsafe impl Sync for ShadowBuffers {}

impl ShadowBuffers {
    pub fn new(
        context: Rc<VulkanContext>,
        shadow_atlas_view: Option<vk::ImageView>,
        shadow_sampler: vk::Sampler,
    ) -> Result<Self, String> {
        let shadow_data_size = std::mem::size_of::<ShadowFrameData>() as u64;

        info!(
            "Creating shadow buffers: shadow_data={} bytes",
            shadow_data_size,
        );

        let (shadow_data_buffer, shadow_data_allocation) = create_buffer(
            &context,
            "shadow_data_buffer",
            shadow_data_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
        )?;

        let shadow_data_mapped_ptr = context.map_buffer(&shadow_data_allocation);
        unsafe {
            std::ptr::write_bytes(shadow_data_mapped_ptr, 0, shadow_data_size as usize);
        }

        let shadow_atlas_views = (0..crate::renderer::FRAMES_IN_FLIGHT)
            .map(|_| shadow_atlas_view)
            .collect();

        Ok(Self {
            context,
            shadow_data_buffer,
            shadow_data_allocation: Some(shadow_data_allocation),
            shadow_data_mapped_ptr,
            shadow_atlas_views,
            shadow_sampler: Some(shadow_sampler),
            destroyed: false,
        })
    }

    pub fn upload_shadow_data(&mut self, data: &ShadowFrameData) {
        if self.shadow_data_mapped_ptr.is_null() {
            return;
        }

        unsafe {
            std::ptr::copy_nonoverlapping(
                data as *const ShadowFrameData as *const u8,
                self.shadow_data_mapped_ptr,
                std::mem::size_of::<ShadowFrameData>(),
            );
        }

        if let Some(ref alloc) = self.shadow_data_allocation.as_ref() {
            self.context.flush_mapped_memory(
                alloc,
                0,
                std::mem::size_of::<ShadowFrameData>() as u64,
            );
        }
    }

    pub fn len(&self) -> usize {
        self.shadow_atlas_views.len()
    }

    pub fn set_shadow_atlas_view(&mut self, frame_idx: usize, view: vk::ImageView) {
        if frame_idx < self.shadow_atlas_views.len() {
            self.shadow_atlas_views[frame_idx] = Some(view);
        }
    }

    pub fn update_and_bind_descriptors(
        &self,
        cmd: vk::CommandBuffer,
        device: &ash::Device,
        pipeline_layout: vk::PipelineLayout,
        descriptor_set: vk::DescriptorSet,
        frame_idx: usize,
    ) -> Result<(), String> {
        let shadow_atlas_view = self
            .shadow_atlas_views
            .get(frame_idx)
            .and_then(|v| *v)
            .filter(|v| *v != vk::ImageView::null())
            .ok_or_else(|| "Shadow atlas view not set or null".to_string())?;
        let shadow_sampler = self
            .shadow_sampler
            .filter(|s| *s != vk::Sampler::null())
            .ok_or_else(|| "Shadow sampler not set or null".to_string())?;

        let buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(self.shadow_data_buffer)
            .offset(0)
            .range(std::mem::size_of::<ShadowFrameData>() as u64)];

        let image_info = [vk::DescriptorImageInfo::default()
            .image_view(shadow_atlas_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];

        let sampler_info = [vk::DescriptorImageInfo::default().sampler(shadow_sampler)];

        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&buffer_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .image_info(&image_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .image_info(&sampler_info),
        ];

        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }

        unsafe {
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout,
                4, // Set 4
                &[descriptor_set],
                &[],
            );
        }

        Ok(())
    }

    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;

        unsafe {
            let device = &self.context.device;

            self.shadow_data_mapped_ptr = std::ptr::null_mut();

            if let Some(alloc) = self.shadow_data_allocation.take() {
                device.destroy_buffer(self.shadow_data_buffer, None);
                let _ = self.context.allocator.borrow_mut().free(alloc);
            }
        }
    }
}

impl Drop for ShadowBuffers {
    fn drop(&mut self) {
        self.destroy();
    }
}
