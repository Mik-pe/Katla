//! Ordered image uploads and filtered mip generation with fence-owned staging.

use super::VulkanContext;
use crate::{
    RendererError,
    texture::{ImageFormat, TextureDescriptor},
};
use ash::vk;
use gpu_allocator::{MemoryLocation, vulkan::Allocation};
use std::{mem::ManuallyDrop, rc::Rc};

struct Staging {
    context: Rc<VulkanContext>,
    buffer: vk::Buffer,
    allocation: ManuallyDrop<Allocation>,
    transferred: bool,
}

impl Staging {
    fn new(context: &Rc<VulkanContext>, pixels: &[u8]) -> Result<Self, RendererError> {
        let info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .size(pixels.len() as u64);
        let (buffer, allocation) = context.allocate_buffer_named(
            &info,
            MemoryLocation::CpuToGpu,
            "texture upload staging",
        )?;
        let staging = Self {
            context: context.clone(),
            buffer,
            allocation: ManuallyDrop::new(allocation),
            transferred: false,
        };
        let mapped = context.map_buffer(&staging.allocation)?;
        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), mapped, pixels.len());
        }
        context.flush_mapped_memory(&staging.allocation, 0, pixels.len() as u64)?;
        Ok(staging)
    }

    fn transfer(mut self) -> (vk::Buffer, Allocation) {
        self.transferred = true;
        (self.buffer, unsafe {
            ManuallyDrop::take(&mut self.allocation)
        })
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.transferred {
            self.context.free_buffer(self.buffer, unsafe {
                ManuallyDrop::take(&mut self.allocation)
            });
        }
    }
}

pub(super) fn image_aspects(format: ImageFormat) -> vk::ImageAspectFlags {
    match format {
        ImageFormat::D32Sfloat => vk::ImageAspectFlags::DEPTH,
        ImageFormat::D32SfloatS8Uint | ImageFormat::D24UnormS8Uint => {
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        }
        _ => vk::ImageAspectFlags::COLOR,
    }
}

pub(super) fn validate_mip_generation(
    context: &VulkanContext,
    desc: &TextureDescriptor,
) -> Result<(), RendererError> {
    if desc.generate_mips {
        let features = unsafe {
            context
                .instance
                .get_physical_device_format_properties(context.physical_device, desc.format.into())
        }
        .optimal_tiling_features;
        let required = vk::FormatFeatureFlags::BLIT_SRC
            | vk::FormatFeatureFlags::BLIT_DST
            | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
        if !features.contains(required) {
            return Err(RendererError::UnsupportedFeature(format!(
                "{:?} cannot generate filtered mipmaps on this Vulkan device",
                desc.format
            )));
        }
    }
    Ok(())
}

pub(super) fn final_layout(desc: &TextureDescriptor) -> vk::ImageLayout {
    if desc.usage.contains(crate::texture::TextureUsage::SAMPLED) {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    } else {
        vk::ImageLayout::GENERAL
    }
}

fn final_access(desc: &TextureDescriptor) -> vk::AccessFlags2 {
    if desc.usage.contains(crate::texture::TextureUsage::SAMPLED) {
        vk::AccessFlags2::SHADER_READ
    } else {
        vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE
    }
}

pub(super) fn upload(
    context: &Rc<VulkanContext>,
    image: vk::Image,
    desc: &TextureDescriptor,
    pixels: &[u8],
    old_layout: vk::ImageLayout,
) -> Result<(), RendererError> {
    let staging = if pixels.is_empty() {
        None
    } else {
        Some(Staging::new(context, pixels)?)
    };
    let command = context.begin_single_time_commands()?;
    let cmd = command.vk_command_buffer();
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(image_aspects(desc.format))
        .level_count(desc.mip_levels)
        .layer_count(1);
    let (source_stage, source_access) = if old_layout == vk::ImageLayout::UNDEFINED {
        (vk::PipelineStageFlags2::NONE, vk::AccessFlags2::NONE)
    } else {
        (vk::PipelineStageFlags2::ALL_COMMANDS, final_access(desc))
    };
    if let Some(staging) = &staging {
        barrier(
            context,
            cmd,
            image,
            range,
            (old_layout, vk::ImageLayout::TRANSFER_DST_OPTIMAL),
            (source_stage, source_access),
            (
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
            ),
        );
        let copy = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width: desc.width,
                height: desc.height,
                depth: 1,
            });
        unsafe {
            context.device.cmd_copy_buffer_to_image(
                cmd,
                staging.buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy],
            );
        }
        if desc.generate_mips {
            generate_mips(context, cmd, image, desc);
        } else {
            barrier(
                context,
                cmd,
                image,
                range,
                (vk::ImageLayout::TRANSFER_DST_OPTIMAL, final_layout(desc)),
                (
                    vk::PipelineStageFlags2::TRANSFER,
                    vk::AccessFlags2::TRANSFER_WRITE,
                ),
                (vk::PipelineStageFlags2::ALL_COMMANDS, final_access(desc)),
            );
        }
    } else {
        barrier(
            context,
            cmd,
            image,
            range,
            (old_layout, final_layout(desc)),
            (source_stage, source_access),
            (vk::PipelineStageFlags2::ALL_COMMANDS, final_access(desc)),
        );
    }
    command.end_single_time_command()?;
    let fence = unsafe {
        context
            .device
            .create_fence(&vk::FenceCreateInfo::default(), None)
    }
    .map_err(|error| {
        RendererError::VulkanError("Failed to create texture upload fence".into(), error)
    })?;
    if let Err(error) = context.gfx_queue.submit(&[&command], &[], &[], fence) {
        unsafe {
            context.device.destroy_fence(fence, None);
        }
        return Err(error);
    }
    context.defer_image_upload(fence, command, staging.map(Staging::transfer), image);
    Ok(())
}

fn generate_mips(
    context: &VulkanContext,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    desc: &TextureDescriptor,
) {
    let range = |level| {
        vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(level)
            .level_count(1)
            .layer_count(1)
    };
    for level in 1..desc.mip_levels {
        barrier(
            context,
            cmd,
            image,
            range(level - 1),
            (
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            ),
            (
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
            ),
            (
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_READ,
            ),
        );
        let layer = |level| {
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(level)
                .layer_count(1)
        };
        let extent = |level: u32| vk::Offset3D {
            x: (desc.width >> level).max(1) as i32,
            y: (desc.height >> level).max(1) as i32,
            z: 1,
        };
        let blit = vk::ImageBlit::default()
            .src_subresource(layer(level - 1))
            .src_offsets([vk::Offset3D::default(), extent(level - 1)])
            .dst_subresource(layer(level))
            .dst_offsets([vk::Offset3D::default(), extent(level)]);
        unsafe {
            context.device.cmd_blit_image(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[blit],
                vk::Filter::LINEAR,
            );
        }
        barrier(
            context,
            cmd,
            image,
            range(level - 1),
            (vk::ImageLayout::TRANSFER_SRC_OPTIMAL, final_layout(desc)),
            (
                vk::PipelineStageFlags2::TRANSFER,
                vk::AccessFlags2::TRANSFER_READ,
            ),
            (vk::PipelineStageFlags2::ALL_COMMANDS, final_access(desc)),
        );
    }
    barrier(
        context,
        cmd,
        image,
        range(desc.mip_levels - 1),
        (vk::ImageLayout::TRANSFER_DST_OPTIMAL, final_layout(desc)),
        (
            vk::PipelineStageFlags2::TRANSFER,
            vk::AccessFlags2::TRANSFER_WRITE,
        ),
        (vk::PipelineStageFlags2::ALL_COMMANDS, final_access(desc)),
    );
}

fn barrier(
    context: &VulkanContext,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    range: vk::ImageSubresourceRange,
    layouts: (vk::ImageLayout, vk::ImageLayout),
    source: (vk::PipelineStageFlags2, vk::AccessFlags2),
    destination: (vk::PipelineStageFlags2, vk::AccessFlags2),
) {
    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(source.0)
        .src_access_mask(source.1)
        .dst_stage_mask(destination.0)
        .dst_access_mask(destination.1)
        .old_layout(layouts.0)
        .new_layout(layouts.1)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(range);
    let dependency =
        vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));
    unsafe {
        context.device.cmd_pipeline_barrier2(cmd, &dependency);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires native Vulkan image upload failure and retirement validation"]
    fn test_native_image_upload_failures_and_early_retirement_release_allocations() {
        let mut renderer = crate::VulkanRenderer::init_headless(
            16,
            16,
            crate::ValidationMode::Enabled,
            c"native image uploads".into(),
            c"Katla".into(),
        )
        .unwrap();
        assert!(renderer.context().validation_active());
        let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let captured = errors.clone();
        renderer
            .context()
            .set_validation_callback(move |message, level| {
                if level == crate::ValidationLevel::Error {
                    captured.lock().unwrap().push(message.into());
                }
            });
        renderer.wait_for_device();
        let baseline = renderer.context.allocator.debug_allocation_stats();
        let count = renderer.texture_manager.len();
        let mut descriptor = TextureDescriptor::rgba8_unorm(8, 4);
        descriptor.mip_levels = 4;
        descriptor.generate_mips = true;
        renderer
            .context
            .gfx_queue
            .fail_next_submission(vk::Result::ERROR_OUT_OF_HOST_MEMORY);
        let error = renderer
            .create_texture(&descriptor, &[255; 128])
            .unwrap_err();
        assert!(matches!(
            error,
            RendererError::VulkanError(_, vk::Result::ERROR_OUT_OF_HOST_MEMORY)
        ));
        assert_eq!(renderer.texture_manager.len(), count);
        assert_eq!(renderer.context.pending_staged_uploads(), 0);
        assert_eq!(
            renderer.context.allocator.debug_allocation_stats(),
            baseline
        );
        let texture =
            super::super::Texture::from_descriptor(&renderer.context, &descriptor, &[255; 128])
                .unwrap();
        texture.update_data(&[128; 128]).unwrap();
        assert_eq!(renderer.context.pending_staged_uploads(), 2);
        drop(texture);
        assert_eq!(renderer.texture_manager.len(), count);
        assert_ne!(
            renderer.context.allocator.debug_allocation_stats(),
            baseline,
            "pending commands still own upload and image allocations"
        );
        renderer.wait_for_device();
        assert_eq!(renderer.context.pending_staged_uploads(), 0);
        assert_eq!(
            renderer.context.allocator.debug_allocation_stats(),
            baseline
        );
        let mut attachment = TextureDescriptor::rgba8_unorm(1, 1);
        attachment.usage = crate::TextureUsage::COLOR_ATTACHMENT;
        let target = renderer.create_texture(&attachment, &[]).unwrap();
        assert!(renderer.get_bindless_slot(target).is_none());
        renderer.destroy_texture(target);
        renderer.wait_for_device();
        renderer.destroy();
        let errors = errors.lock().unwrap();
        assert!(errors.is_empty(), "native validation errors: {errors:?}");
    }
}
