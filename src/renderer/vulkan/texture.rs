use crate::renderer::{VulkanContext, VulkanFrameCtx};

use ash::{version::DeviceV1_0, vk};
use vk_mem::Allocation;
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    image_memory: Allocation,
    image: vk::Image,
    pub image_view: vk::ImageView,
    pub image_sampler: vk::Sampler,
}

impl Texture {
    fn create_staging_buffer(
        context: &VulkanContext,
        size: vk::DeviceSize,
    ) -> (vk::Buffer, Allocation) {
        let create_info = vk::BufferCreateInfo::builder()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .size(size);

        context.allocate_buffer(&create_info, vk_mem::MemoryUsage::CpuToGpu)
    }

    fn transition_image_layout(
        context: &VulkanContext,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) {
        let command_buffer = context.begin_transfer_commands();
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);

        unsafe {
            let src_stage_mask;
            let dst_stage_mask;
            let mut barrier_builder = vk::ImageMemoryBarrier::builder()
                .old_layout(old_layout)
                .new_layout(new_layout)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(subresource_range.build());

            if old_layout == vk::ImageLayout::UNDEFINED
                && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
            {
                barrier_builder = barrier_builder
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

                src_stage_mask = vk::PipelineStageFlags::TOP_OF_PIPE;
                dst_stage_mask = vk::PipelineStageFlags::TRANSFER;
            } else if old_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
                && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            {
                barrier_builder = barrier_builder
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ);

                src_stage_mask = vk::PipelineStageFlags::TRANSFER;
                dst_stage_mask = vk::PipelineStageFlags::FRAGMENT_SHADER;
            } else {
                panic!("unsupported layout transition!");
            }

            context.device.cmd_pipeline_barrier(
                command_buffer,
                src_stage_mask,
                dst_stage_mask,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_builder.build()],
            );

            context.end_transfer_commands(command_buffer);
        }
    }

    fn copy_buffer_to_image(
        context: &VulkanContext,
        src_buffer: vk::Buffer,
        dst_image: vk::Image,
        dst_image_layout: vk::ImageLayout,
        extent: vk::Extent3D,
    ) {
        //TODO: expose a transfer command buffer?
        let command_buffer = context.begin_transfer_commands();
        let subresources = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0)
            .base_array_layer(0)
            .layer_count(1);
        unsafe {
            let regions = vk::BufferImageCopy::builder()
                .image_extent(extent)
                .image_subresource(subresources.build());
            context.device.cmd_copy_buffer_to_image(
                command_buffer,
                src_buffer,
                dst_image,
                dst_image_layout,
                &[regions.build()],
            );
        }

        context.end_transfer_commands(command_buffer);
    }

    fn create_texture_sampler(context: &VulkanContext) -> vk::Sampler {
        let create_info = vk::SamplerCreateInfo::builder()
            .anisotropy_enable(true)
            .max_anisotropy(16.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .min_filter(vk::Filter::LINEAR)
            .mag_filter(vk::Filter::LINEAR)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(0.0);
        unsafe { context.device.create_sampler(&create_info, None).unwrap() }
    }

    pub fn create_image(
        context: &VulkanContext,
        width: u32,
        height: u32,
        format: vk::Format,
        pixel_data: &[u8],
    ) -> Self {
        unsafe {
            let extent = vk::Extent3D {
                width,
                height,
                depth: 1,
            };
            //Create the image memory gpu_only:
            let create_info = vk::ImageCreateInfo::builder()
                .extent(extent)
                .image_type(vk::ImageType::TYPE_2D)
                .mip_levels(1)
                .array_layers(1)
                .format(format)
                .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .tiling(vk::ImageTiling::OPTIMAL)
                .samples(vk::SampleCountFlags::TYPE_1)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let (image_object, image_memory) =
                context.create_image(create_info.build(), vk_mem::MemoryUsage::GpuOnly);

            let total_size = pixel_data.len() as u64;

            let (staging_buffer, staging_allocation) =
                Self::create_staging_buffer(context, total_size);
            let mut map = context.map_buffer(&staging_allocation);
            std::ptr::copy_nonoverlapping(pixel_data.as_ptr(), map, total_size as usize);
            context.unmap_buffer(&staging_allocation);

            Self::transition_image_layout(
                context,
                image_object,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            Self::copy_buffer_to_image(
                context,
                staging_buffer,
                image_object,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                extent,
            );
            Self::transition_image_layout(
                context,
                image_object,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );

            context.free_buffer(staging_buffer, staging_allocation);

            let image_view = VulkanFrameCtx::create_image_view(
                &context.device,
                image_object,
                format,
                vk::ImageAspectFlags::COLOR,
            );
            let image_sampler = Self::create_texture_sampler(context);
            Self {
                width,
                height,
                channels: 4,
                image_memory,
                image: image_object,
                image_view,
                image_sampler,
            }
        }
    }

    pub fn get_size(&self) -> u32 {
        self.width * self.height * self.channels
    }

    pub fn destroy(self, context: &VulkanContext) {
        unsafe {
            context.device.destroy_sampler(self.image_sampler, None);
            context.device.destroy_image_view(self.image_view, None);
        }
        context.free_image(self.image, &self.image_memory);
    }
}
