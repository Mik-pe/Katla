use super::{VulkanContext, VulkanFrameCtx};
use erupt::{
    utils::allocator::{Allocation, MemoryTypeFinder},
    vk1_0::*,
};
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    image_memory: Allocation<Image>,
    pub image_view: ImageView,
    pub image_sampler: Sampler,
}

impl Texture {
    fn create_staging_buffer(context: &VulkanContext, size: DeviceSize) -> Allocation<Buffer> {
        let create_info = BufferCreateInfoBuilder::new()
            .sharing_mode(SharingMode::EXCLUSIVE)
            .usage(BufferUsageFlags::TRANSFER_SRC)
            .size(size);

        let buffer = context
            .allocate_object(
                unsafe {
                    context
                        .device
                        .create_buffer(&create_info, None, None)
                        .unwrap()
                },
                MemoryTypeFinder::upload(),
            )
            .unwrap();

        buffer
    }

    fn transition_image_layout(
        context: &VulkanContext,
        image: Image,
        old_layout: ImageLayout,
        new_layout: ImageLayout,
    ) {
        let command_buffer = context.begin_single_time_commands();
        let subresource_range = ImageSubresourceRangeBuilder::new()
            .aspect_mask(ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);

        unsafe {
            let src_stage_mask;
            let dst_stage_mask;
            let mut barrier_builder = ImageMemoryBarrierBuilder::new()
                .old_layout(old_layout)
                .new_layout(new_layout)
                .src_queue_family_index(QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(subresource_range.build());

            if old_layout == ImageLayout::UNDEFINED
                && new_layout == ImageLayout::TRANSFER_DST_OPTIMAL
            {
                barrier_builder = barrier_builder
                    .src_access_mask(AccessFlags::from_bits(0).unwrap())
                    .dst_access_mask(AccessFlags::TRANSFER_WRITE);

                src_stage_mask = PipelineStageFlags::TOP_OF_PIPE;
                dst_stage_mask = PipelineStageFlags::TRANSFER;
            } else if old_layout == ImageLayout::TRANSFER_DST_OPTIMAL
                && new_layout == ImageLayout::SHADER_READ_ONLY_OPTIMAL
            {
                barrier_builder = barrier_builder
                    .src_access_mask(AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(AccessFlags::SHADER_READ);

                src_stage_mask = PipelineStageFlags::TRANSFER;
                dst_stage_mask = PipelineStageFlags::FRAGMENT_SHADER;
            } else {
                panic!("unsupported layout transition!");
            }

            context.device.cmd_pipeline_barrier(
                command_buffer,
                src_stage_mask,
                dst_stage_mask,
                Some(DependencyFlags::from_bits(0).unwrap()),
                &vec![],
                &vec![],
                &vec![barrier_builder],
            );

            context.end_single_time_commands(command_buffer);
        }
    }

    fn copy_buffer_to_image(
        context: &VulkanContext,
        src_buffer: Buffer,
        dst_image: Image,
        dst_image_layout: ImageLayout,
        extent: Extent3D,
    ) {
        let command_buffer = context.begin_single_time_commands();
        let subresources = ImageSubresourceLayersBuilder::new()
            .aspect_mask(ImageAspectFlags::COLOR)
            .mip_level(0)
            .base_array_layer(0)
            .layer_count(1);
        unsafe {
            let regions = vec![BufferImageCopyBuilder::new()
                .image_extent(extent)
                .image_subresource(subresources.build())];
            context.device.cmd_copy_buffer_to_image(
                command_buffer,
                src_buffer,
                dst_image,
                dst_image_layout,
                &regions,
            );
        }

        context.end_single_time_commands(command_buffer);
    }

    fn create_texture_sampler(context: &VulkanContext) -> Sampler {
        let create_info = SamplerCreateInfoBuilder::new()
            .anisotropy_enable(true)
            .max_anisotropy(16.0)
            .border_color(BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .min_filter(Filter::LINEAR)
            .mag_filter(Filter::LINEAR)
            .compare_enable(false)
            .compare_op(CompareOp::ALWAYS)
            .mipmap_mode(SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(0.0);
        unsafe {
            context
                .device
                .create_sampler(&create_info, None, None)
                .unwrap()
        }
    }

    pub fn create_image(
        context: &VulkanContext,
        width: u32,
        height: u32,
        format: Format,
        pixel_data: &[u8],
    ) -> Self {
        unsafe {
            let extent = Extent3D {
                width,
                height,
                depth: 1,
            };
            //Create the image memory gpu_only:
            let create_info = ImageCreateInfoBuilder::new()
                .extent(extent)
                .image_type(ImageType::_2D)
                .mip_levels(1)
                .array_layers(1)
                .format(format)
                .usage(ImageUsageFlags::TRANSFER_DST | ImageUsageFlags::SAMPLED)
                .initial_layout(ImageLayout::UNDEFINED)
                .tiling(ImageTiling::OPTIMAL)
                .samples(SampleCountFlagBits::_1)
                .sharing_mode(SharingMode::EXCLUSIVE);

            let image_object = context
                .device
                .create_image(&create_info, None, None)
                .unwrap();
            let image_memory = context
                .allocate_object(image_object, MemoryTypeFinder::gpu_only())
                .unwrap();

            let total_size = pixel_data.len() as u64;
            let range = ..image_memory.region().start + total_size;

            let staging_buffer = Self::create_staging_buffer(context, total_size);
            let mut map = staging_buffer.map(&context.device, range).unwrap();
            map.import(pixel_data);
            map.unmap(&context.device).unwrap();

            Self::transition_image_layout(
                context,
                image_object,
                ImageLayout::UNDEFINED,
                ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            Self::copy_buffer_to_image(
                context,
                *staging_buffer.object(),
                image_object,
                ImageLayout::TRANSFER_DST_OPTIMAL,
                extent,
            );
            Self::transition_image_layout(
                context,
                image_object,
                ImageLayout::TRANSFER_DST_OPTIMAL,
                ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );

            context.free_object(staging_buffer);

            let image_view = VulkanFrameCtx::create_image_view(
                &context.device,
                image_object,
                format,
                ImageAspectFlags::COLOR,
            );
            let image_sampler = Self::create_texture_sampler(context);
            Self {
                width,
                height,
                channels: 4,
                image_memory,
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
            context
                .device
                .destroy_sampler(Some(self.image_sampler), None);
            context
                .device
                .destroy_image_view(Some(self.image_view), None);
        }
        context.free_object(self.image_memory);
    }
}
