use std::time::Instant;

use crate::renderer::{VulkanContext, VulkanFrameCtx};

use ash::vk;
use gpu_allocator::vulkan::Allocation;
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

        context.allocate_buffer(&create_info, gpu_allocator::MemoryLocation::CpuToGpu)
    }

    fn transition_image_layout(
        context: &VulkanContext,
        command_buffer: vk::CommandBuffer,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) {
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
        }
    }

    fn copy_buffer_to_image(
        context: &VulkanContext,
        command_buffer: vk::CommandBuffer,
        src_buffer: vk::Buffer,
        dst_image: vk::Image,
        dst_image_layout: vk::ImageLayout,
        extent: vk::Extent3D,
    ) {
        //TODO: expose a transfer command buffer?
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
        let total_start = Instant::now();
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
            context.create_image(create_info.build(), gpu_allocator::MemoryLocation::GpuOnly);
        let ms_image = total_start.elapsed().as_micros() as f64 / 1000.0;

        let total_size = pixel_data.len() as u64;

        let (staging_buffer, staging_allocation) = Self::create_staging_buffer(context, total_size);
        let ms_staging = total_start.elapsed().as_micros() as f64 / 1000.0;

        let map = context.map_buffer(&staging_allocation);
        let ms_map_buffer = total_start.elapsed().as_micros() as f64 / 1000.0;

        unsafe {
            std::ptr::copy_nonoverlapping(pixel_data.as_ptr(), map, total_size as usize);
            let ms_copy = total_start.elapsed().as_micros() as f64 / 1000.0;

            // context.unmap_buffer(&staging_allocation);
            let ms_unmap = total_start.elapsed().as_micros() as f64 / 1000.0;

            let command_buffer = context.begin_single_time_commands();
            Self::transition_image_layout(
                context,
                command_buffer.vk_command_buffer(),
                image_object,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            let ms_trans_1 = total_start.elapsed().as_micros() as f64 / 1000.0;

            Self::copy_buffer_to_image(
                context,
                command_buffer.vk_command_buffer(),
                staging_buffer,
                image_object,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                extent,
            );
            let ms_copy_im = total_start.elapsed().as_micros() as f64 / 1000.0;
            Self::transition_image_layout(
                context,
                command_buffer.vk_command_buffer(),
                image_object,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );

            //TODO: submitting this command buffer takes lots of time
            //TODO: Fix better handling of these command buffers from the renderer
            context.end_single_time_commands(command_buffer);
            let ms_trans_2 = total_start.elapsed().as_micros() as f64 / 1000.0;

            context.free_buffer(staging_buffer, staging_allocation);
            let ms_free = total_start.elapsed().as_micros() as f64 / 1000.0;

            let image_view = VulkanFrameCtx::create_image_view(
                &context.device,
                image_object,
                format,
                vk::ImageAspectFlags::COLOR,
            );
            let image_sampler = Self::create_texture_sampler(context);
            let ms_total = total_start.elapsed().as_micros() as f64 / 1000.0;
            println!(
                "[Create Image] Image size: {:.2}MiB",
                total_size as f64 / (1024f64 * 1024f64)
            );
            println!("[Create Image] Total time spent: {}ms", ms_total);

            println!(
                "image: \t\t\t\t{:.3}ms {:.2}%",
                ms_image,
                ms_image / ms_total * 100.0
            );
            println!(
                "stage: \t\t\t\t{:.3}ms {:.2}% ",
                ms_staging,
                (ms_staging - ms_image) / ms_total * 100.0
            );
            println!(
                "map: \t\t\t\t{:.3}ms {:.2}%",
                ms_map_buffer,
                (ms_map_buffer - ms_staging) / ms_total * 100.0
            );
            println!(
                "copy to map: \t\t\t{:.3}ms {:.2}%",
                ms_copy,
                (ms_copy - ms_map_buffer) / ms_total * 100.0
            );
            println!(
                "unmap: \t\t\t\t{:.3}ms {:.2}%",
                ms_unmap,
                (ms_unmap - ms_copy) / ms_total * 100.0
            );
            println!(
                "transition image: \t\t{:.3}ms {:.2}%",
                ms_trans_1,
                (ms_trans_1 - ms_unmap) / ms_total * 100.0
            );
            println!(
                "copy buffer to image: \t\t{:.3}ms {:.2}%",
                ms_copy_im,
                (ms_copy_im - ms_trans_1) / ms_total * 100.0
            );
            println!(
                "transition + submit cmdbuf: \t{:.3}ms {:.2}%",
                ms_trans_2,
                (ms_trans_2 - ms_copy_im) / ms_total * 100.0
            );
            println!(
                "free: \t\t\t\t{:.3}ms {:.2}%",
                ms_free,
                (ms_free - ms_trans_2) / ms_total * 100.0
            );

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

    pub fn destroy(self, context: &VulkanContext) {
        unsafe {
            context.device.destroy_sampler(self.image_sampler, None);
            context.device.destroy_image_view(self.image_view, None);
        }
        context.free_image(self.image, self.image_memory);
    }
}
