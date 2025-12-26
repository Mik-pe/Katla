use super::VulkanContext;
use crate::VulkanFrameCtx;

use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::time::Instant;

use ash::vk;
use gpu_allocator::vulkan::Allocation;

pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    image_memory: ManuallyDrop<Allocation>,
    image: vk::Image,
    pub image_view: vk::ImageView,
    pub image_sampler: vk::Sampler,
    context: Rc<VulkanContext>,
}

impl Texture {
    fn create_staging_buffer(
        context: &VulkanContext,
        size: vk::DeviceSize,
    ) -> (vk::Buffer, Allocation) {
        let create_info = vk::BufferCreateInfo::default()
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
        let subresource_range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);

        unsafe {
            let src_stage_mask;
            let dst_stage_mask;
            let mut barrier_default = vk::ImageMemoryBarrier::default()
                .old_layout(old_layout)
                .new_layout(new_layout)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(subresource_range);

            if old_layout == vk::ImageLayout::UNDEFINED
                && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
            {
                barrier_default = barrier_default
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

                src_stage_mask = vk::PipelineStageFlags::TOP_OF_PIPE;
                dst_stage_mask = vk::PipelineStageFlags::TRANSFER;
            } else if old_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
                && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            {
                barrier_default = barrier_default
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
                &[barrier_default],
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
        let subresources = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0)
            .base_array_layer(0)
            .layer_count(1);
        unsafe {
            let regions = vk::BufferImageCopy::default()
                .image_extent(extent)
                .image_subresource(subresources);
            context.device.cmd_copy_buffer_to_image(
                command_buffer,
                src_buffer,
                dst_image,
                dst_image_layout,
                &[regions],
            );
        }
    }

    fn create_texture_sampler(context: &VulkanContext) -> vk::Sampler {
        let create_info = vk::SamplerCreateInfo::default()
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

    fn convert_rgb_to_rgba(rgb_data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let pixel_count = (width * height) as usize;
        let mut rgba_data = Vec::with_capacity(pixel_count * 4);

        for chunk in rgb_data.chunks_exact(3) {
            rgba_data.push(chunk[0]);
            rgba_data.push(chunk[1]);
            rgba_data.push(chunk[2]);
            rgba_data.push(255);
        }

        rgba_data
    }

    pub fn create_image_rgb(
        context: Rc<VulkanContext>,
        width: u32,
        height: u32,
        pixel_data: &[u8],
    ) -> Self {
        let rgba_data = Self::convert_rgb_to_rgba(pixel_data, width, height);
        Self::create_image(
            context,
            width,
            height,
            vk::Format::R8G8B8A8_SRGB,
            &rgba_data,
        )
    }

    pub fn create_image(
        context: Rc<VulkanContext>,
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
        let create_info = vk::ImageCreateInfo::default()
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
            context.create_image(create_info, gpu_allocator::MemoryLocation::GpuOnly);
        let ms_image = total_start.elapsed().as_micros() as f64 / 1000.0;

        let total_size = pixel_data.len() as u64;

        let (staging_buffer, staging_allocation) =
            Self::create_staging_buffer(&context, total_size);
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
                &context,
                command_buffer.vk_command_buffer(),
                image_object,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            let ms_trans_1 = total_start.elapsed().as_micros() as f64 / 1000.0;

            Self::copy_buffer_to_image(
                &context,
                command_buffer.vk_command_buffer(),
                staging_buffer,
                image_object,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                extent,
            );
            let ms_copy_im = total_start.elapsed().as_micros() as f64 / 1000.0;
            Self::transition_image_layout(
                &context,
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
            let image_sampler = Self::create_texture_sampler(&context);
            let ms_total = total_start.elapsed().as_micros() as f64 / 1000.0;
            println!(
                "[Create Image] Image size: {:.2}MiB",
                total_size as f64 / (1024f64 * 1024f64)
            );
            println!("[Create Image] Total time spent: {ms_total}ms");

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
            let bytes_per_sec = total_size as f64 / (ms_copy / 1000.0);
            println!(
                "copy to map: \t\t\t{:.3}ms {:.2}% ({:.3} B/s)",
                ms_copy,
                (ms_copy - ms_map_buffer) / ms_total * 100.0,
                bytes_per_sec
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

            let channels = match format {
                vk::Format::R8G8B8_SRGB | vk::Format::R8G8B8_UNORM => 3,
                vk::Format::R8G8B8A8_SRGB | vk::Format::R8G8B8A8_UNORM => 4,
                _ => 4,
            };

            Self {
                width,
                height,
                channels,
                image_memory: ManuallyDrop::new(image_memory),
                image: image_object,
                image_view,
                image_sampler,
                context,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_rgb_to_rgba_single_pixel() {
        let rgb_data = vec![255, 128, 64];
        let result = Texture::convert_rgb_to_rgba(&rgb_data, 1, 1);

        assert_eq!(result.len(), 4);
        assert_eq!(result[0], 255);
        assert_eq!(result[1], 128);
        assert_eq!(result[2], 64);
        assert_eq!(result[3], 255);
    }

    #[test]
    fn test_convert_rgb_to_rgba_multiple_pixels() {
        let rgb_data = vec![255, 0, 0, 0, 255, 0, 0, 0, 255];
        let result = Texture::convert_rgb_to_rgba(&rgb_data, 3, 1);

        assert_eq!(result.len(), 12);
        assert_eq!(result[0], 255);
        assert_eq!(result[1], 0);
        assert_eq!(result[2], 0);
        assert_eq!(result[3], 255);
        assert_eq!(result[4], 0);
        assert_eq!(result[5], 255);
        assert_eq!(result[6], 0);
        assert_eq!(result[7], 255);
        assert_eq!(result[8], 0);
        assert_eq!(result[9], 0);
        assert_eq!(result[10], 255);
        assert_eq!(result[11], 255);
    }

    #[test]
    fn test_convert_rgb_to_rgba_2x2() {
        let rgb_data = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 128, 128, 128];
        let result = Texture::convert_rgb_to_rgba(&rgb_data, 2, 2);

        assert_eq!(result.len(), 16);
        assert_eq!(&result[12..16], &[128, 128, 128, 255]);
    }

    #[test]
    fn test_convert_rgb_to_rgba_capacity() {
        let rgb_data = vec![100, 150, 200];
        let result = Texture::convert_rgb_to_rgba(&rgb_data, 1, 1);

        assert_eq!(result.capacity(), 4);
    }

    #[test]
    fn test_convert_rgb_to_rgba_empty() {
        let rgb_data: Vec<u8> = vec![];
        let result = Texture::convert_rgb_to_rgba(&rgb_data, 0, 0);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_convert_rgb_to_rgba_preserves_black() {
        let rgb_data = vec![0, 0, 0];
        let result = Texture::convert_rgb_to_rgba(&rgb_data, 1, 1);

        assert_eq!(result, vec![0, 0, 0, 255]);
    }

    #[test]
    fn test_convert_rgb_to_rgba_preserves_white() {
        let rgb_data = vec![255, 255, 255];
        let result = Texture::convert_rgb_to_rgba(&rgb_data, 1, 1);

        assert_eq!(result, vec![255, 255, 255, 255]);
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_sampler(self.image_sampler, None);
            self.context
                .device
                .destroy_image_view(self.image_view, None);
        }
        let allocation = unsafe { ManuallyDrop::take(&mut self.image_memory) };
        self.context.free_image(self.image, allocation);
    }
}
