use super::VulkanContext;
use crate::render_graph::types::ImageFormat;
use crate::sync::{AccessFlags2, DependencyInfo, ImageMemoryBarrier2, PipelineStage2Flags};
use crate::VulkanFrameCtx;
use crate::{VkImage, VkImageView, VkSampler};

use std::mem::ManuallyDrop;
use std::rc::Rc;

use ash::vk;
use gpu_allocator::vulkan::Allocation;

pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    image_memory: ManuallyDrop<Allocation>,
    image: VkImage,
    pub image_view: VkImageView,
    pub image_sampler: VkSampler,
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

        // Modern Vulkan 1.3 barrier using Synchronization2
        let barrier = ImageMemoryBarrier2::new(VkImage::new(image))
            .src_stage(PipelineStage2Flags::from(src_stage_mask))
            .dst_stage(PipelineStage2Flags::from(dst_stage_mask))
            .src_access(AccessFlags2::from(barrier_default.src_access_mask))
            .dst_access(AccessFlags2::from(barrier_default.dst_access_mask))
            .old_layout(old_layout)
            .new_layout(new_layout)
            .subresource_range(subresource_range);

        let dep_info = DependencyInfo::new().add_image_barrier(barrier);
        dep_info.build(|dep_info| unsafe {
            context
                .device
                .cmd_pipeline_barrier2(command_buffer, dep_info);
        });
    }

    fn copy_buffer_to_image(
        context: &VulkanContext,
        command_buffer: vk::CommandBuffer,
        src_buffer: vk::Buffer,
        dst_image: vk::Image,
        dst_image_layout: vk::ImageLayout,
        extent: vk::Extent3D,
    ) {
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
            ImageFormat::R8G8B8A8Srgb,
            &rgba_data,
        )
    }

    pub fn create_image(
        context: Rc<VulkanContext>,
        width: u32,
        height: u32,
        format: ImageFormat,
        pixel_data: &[u8],
    ) -> Self {
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };

        // Convert ImageFormat to vk::Format for internal use
        let vk_format: ash::vk::Format = format.into();

        //Create the image memory gpu_only:
        let create_info = vk::ImageCreateInfo::default()
            .extent(extent)
            .image_type(vk::ImageType::TYPE_2D)
            .mip_levels(1)
            .array_layers(1)
            .format(vk_format)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .tiling(vk::ImageTiling::OPTIMAL)
            .samples(vk::SampleCountFlags::TYPE_1)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (image_object, image_memory) =
            context.create_image(create_info, gpu_allocator::MemoryLocation::GpuOnly);

        let total_size = pixel_data.len() as u64;

        let (staging_buffer, staging_allocation) =
            Self::create_staging_buffer(&context, total_size);

        let map = context.map_buffer(&staging_allocation);

        unsafe {
            std::ptr::copy_nonoverlapping(pixel_data.as_ptr(), map, total_size as usize);

            let command_buffer = context.begin_single_time_commands();
            Self::transition_image_layout(
                &context,
                command_buffer.vk_command_buffer(),
                image_object,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            Self::copy_buffer_to_image(
                &context,
                command_buffer.vk_command_buffer(),
                staging_buffer,
                image_object,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                extent,
            );
            Self::transition_image_layout(
                &context,
                command_buffer.vk_command_buffer(),
                image_object,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );

            // NOTE: Synchronous command buffer submission can be a bottleneck.
            // For better performance, batch multiple uploads or use async transfer queues.
            context.end_single_time_commands(command_buffer);
            context.free_buffer(staging_buffer, staging_allocation);

            let image_view = VulkanFrameCtx::create_image_view(
                &context.device,
                image_object,
                vk_format,
                vk::ImageAspectFlags::COLOR,
            );
            let image_sampler = Self::create_texture_sampler(&context);

            let channels = match format {
                ImageFormat::R8G8B8A8Srgb | ImageFormat::B8G8R8A8Srgb => 4,
                _ => 4,
            };

            Self {
                width,
                height,
                channels,
                image_memory: ManuallyDrop::new(image_memory),
                image: VkImage::new(image_object),
                image_view: VkImageView::new(image_view),
                image_sampler: VkSampler::new(image_sampler),
                context,
            }
        }
    }

    /// Create a default albedo texture (white 1x1).
    /// Used when a material doesn't have an albedo texture.
    pub fn create_default_albedo(context: Rc<VulkanContext>) -> Self {
        // White pixel: RGBA (255, 255, 255, 255)
        let pixel_data: [u8; 4] = [255, 255, 255, 255];
        Self::create_image(
            context,
            1,
            1,
            ImageFormat::R8G8B8A8Srgb,
            &pixel_data,
        )
    }

    /// Create a default normal map (flat normal 1x1).
    /// In tangent space, a flat surface normal is (0, 0, 1).
    /// Normalized and mapped to [0,255]: (128, 128, 255, 255)
    /// Note: Normal maps are LINEAR data, not SRGB.
    pub fn create_default_normal(context: Rc<VulkanContext>) -> Self {
        // Flat normal: RGB (128, 128, 255) = tangent-space Z-up normal
        let pixel_data: [u8; 4] = [128, 128, 255, 255];
        Self::create_image(
            context,
            1,
            1,
            ImageFormat::R8G8B8A8Unorm,
            &pixel_data,
        )
    }

    /// Create a default metallic/roughness texture.
    /// GLTF packed format: G = roughness, B = metallic
    /// Default: roughness = 0.5 (128), metallic = 0.0 (0)
    /// Note: MR textures are LINEAR data, not SRGB.
    pub fn create_default_metallic_roughness(context: Rc<VulkanContext>) -> Self {
        // R = unused (0), G = roughness 0.5 (128), B = metallic 0 (0), A = unused (255)
        let pixel_data: [u8; 4] = [0, 128, 0, 255];
        Self::create_image(
            context,
            1,
            1,
            ImageFormat::R8G8B8A8Unorm,
            &pixel_data,
        )
    }

    /// Create a default occlusion texture (white 1x1).
    /// White = full visibility (no occlusion).
    /// Note: AO textures are LINEAR data, not SRGB.
    pub fn create_default_occlusion(context: Rc<VulkanContext>) -> Self {
        // White pixel: full visibility
        let pixel_data: [u8; 4] = [255, 255, 255, 255];
        Self::create_image(
            context,
            1,
            1,
            ImageFormat::R8G8B8A8Unorm,
            &pixel_data,
        )
    }

    /// Create a default emission texture (black 1x1).
    /// Black = no emission / self-illumination.
    /// Note: Emission textures are LINEAR HDR data, not SRGB.
    pub fn create_default_emission(context: Rc<VulkanContext>) -> Self {
        // Black pixel: no emission
        let pixel_data: [u8; 4] = [0, 0, 0, 255];
        Self::create_image(
            context,
            1,
            1,
            ImageFormat::R8G8B8A8Unorm,
            &pixel_data,
        )
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
                .destroy_sampler(self.image_sampler.vk(), None);
            self.context
                .device
                .destroy_image_view(self.image_view.vk(), None);
        }
        let allocation = unsafe { ManuallyDrop::take(&mut self.image_memory) };
        self.context.free_image(self.image.vk(), allocation);
    }
}
