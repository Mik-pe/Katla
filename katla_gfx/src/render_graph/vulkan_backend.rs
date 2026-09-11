//! Vulkan backend for the render graph.
//!
//! Implements `RenderGraphBackend` for `VulkanRenderer`, providing
//! concrete transient texture creation, bindless management, and
//! frame indexing using Vulkan GPU resources.

use std::rc::Rc;

use super::backend::RenderGraphBackend;
use super::error::RenderGraphError;
use super::resource::{GraphResourceDesc, GraphResourceType};
use super::transient_texture::{TransientTexture, VkSlotMemory};
use crate::renderer::VulkanRenderer;
use ash::vk;

impl RenderGraphBackend for VulkanRenderer {
    type TransientTexture = TransientTexture;
    type ImageView = crate::sync::VkImageView;

    fn create_transient_slot(
        &self,
        members: &[GraphResourceDesc],
    ) -> Result<Vec<Self::TransientTexture>, RenderGraphError> {
        if let [single] = members {
            return Ok(vec![create_standalone_transient_texture(self, single)?]);
        }
        create_aliased_transient_textures(self, members)
    }

    fn destroy_transient_texture(texture: Self::TransientTexture) {
        drop(texture);
    }

    fn current_frame(&self) -> usize {
        VulkanRenderer::current_frame(self)
    }

    fn transient_texture_frames() -> usize {
        2
    }

    fn register_bindless_texture(
        &mut self,
        texture: &Self::TransientTexture,
    ) -> Result<u32, RenderGraphError> {
        self.register_bindless_texture(texture.image_view.vk())
            .map_err(|e| RenderGraphError::BackendError(e.to_string()))
    }

    fn update_bindless_texture(
        &mut self,
        slot: u32,
        texture: &Self::TransientTexture,
    ) -> Result<(), RenderGraphError> {
        self.update_bindless_texture(slot, texture.image_view.vk())
            .map_err(|e| RenderGraphError::BackendError(e.to_string()))
    }

    fn transient_texture_format(texture: &Self::TransientTexture) -> crate::texture::ImageFormat {
        texture
            .format
            .try_into()
            .unwrap_or(crate::texture::ImageFormat::R8G8B8A8Srgb)
    }

    fn transient_texture_extent(texture: &Self::TransientTexture) -> (u32, u32) {
        (texture.extent.width, texture.extent.height)
    }

    fn transient_texture_is_depth(texture: &Self::TransientTexture) -> bool {
        texture.format == vk::Format::D32_SFLOAT
    }

    fn transient_texture_bindless_slot(texture: &Self::TransientTexture) -> Option<u32> {
        texture.bindless_slot
    }

    fn set_transient_texture_bindless_slot(texture: &mut Self::TransientTexture, slot: u32) {
        texture.bindless_slot = Some(slot);
    }

    fn transient_texture_view(texture: &Self::TransientTexture) -> Self::ImageView {
        texture.image_view
    }

    fn swapchain_image_view(&self, image_index: u32) -> Self::ImageView {
        self.frame_context.swapchain_image_views[image_index as usize]
    }

    fn depth_image_view(&self, frame_index: usize) -> Option<Self::ImageView> {
        self.frame_context
            .depth_render_textures
            .get(frame_index)
            .map(|dt| dt.image_view)
    }
}

/// Whether a resource's contents are only ever attachment data within a
/// render pass. Such resources may back onto lazily allocated memory,
/// which tile-based architectures can keep entirely in tile memory.
fn is_attachment_only(desc: &GraphResourceDesc) -> bool {
    match desc.resource_type {
        GraphResourceType::DepthAttachment { sampled, .. } => !sampled,
        GraphResourceType::ColorAttachment { .. } | GraphResourceType::SampledImage => false,
    }
}

fn transient_image_usage(desc: &GraphResourceDesc) -> vk::ImageUsageFlags {
    match desc.resource_type {
        GraphResourceType::ColorAttachment { .. } => {
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::INPUT_ATTACHMENT
                // Editor targets may be read back (GPU picking, PNG
                // capture), matching the swapchain image usage.
                | vk::ImageUsageFlags::TRANSFER_SRC
        }
        GraphResourceType::DepthAttachment { sampled, .. } => {
            let mut usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
            if sampled {
                usage |= vk::ImageUsageFlags::SAMPLED;
            } else {
                // Attachment-only depth may live in lazily allocated memory.
                usage |= vk::ImageUsageFlags::TRANSIENT_ATTACHMENT;
            }
            usage
        }
        GraphResourceType::SampledImage => {
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST
        }
    }
}

fn transient_image_create_info(desc: &GraphResourceDesc) -> vk::ImageCreateInfo<'static> {
    vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .extent(vk::Extent3D {
            width: desc.width,
            height: desc.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .format(desc.format.into())
        .tiling(vk::ImageTiling::OPTIMAL)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .samples(vk::SampleCountFlags::TYPE_1)
        .usage(transient_image_usage(desc))
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
}

fn transient_aspect(desc: &GraphResourceDesc) -> vk::ImageAspectFlags {
    if matches!(
        desc.resource_type,
        GraphResourceType::DepthAttachment { .. }
    ) {
        vk::ImageAspectFlags::DEPTH
    } else {
        vk::ImageAspectFlags::COLOR
    }
}

fn create_transient_image_view(
    backend: &VulkanRenderer,
    image: vk::Image,
    desc: &GraphResourceDesc,
) -> Result<crate::sync::VkImageView, RenderGraphError> {
    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(desc.format.into())
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: transient_aspect(desc),
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });

    unsafe {
        backend
            .context
            .device
            .create_image_view(&view_info, None)
            .map(crate::sync::VkImageView::new)
            .map_err(|e| {
                RenderGraphError::BackendError(format!("Failed to create image view: {}", e))
            })
    }
}

fn create_standalone_transient_texture(
    backend: &VulkanRenderer,
    desc: &GraphResourceDesc,
) -> Result<TransientTexture, RenderGraphError> {
    let (image, allocation) = backend
        .context
        .create_image(
            transient_image_create_info(desc),
            gpu_allocator::MemoryLocation::GpuOnly,
        )
        .map_err(|_e| RenderGraphError::AllocationFailed(0))?;

    let image_view = create_transient_image_view(backend, image, desc)?;

    Ok(TransientTexture::new(
        backend.context.clone(),
        image,
        Some(allocation),
        image_view,
        desc.format.into(),
        vk::Extent2D {
            width: desc.width,
            height: desc.height,
        },
    ))
}

/// Select a memory type for one aliased slot from the intersection of the
/// member images' memory type bits.
///
/// Attachment-only slots prefer lazily allocated memory; everything else
/// prefers device-local memory. Lazily allocated types are never selected
/// for slots whose members observe contents outside a render pass.
fn select_slot_memory_type(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    type_bits: u32,
    attachment_only: bool,
) -> Option<u32> {
    let types = memory_properties.memory_types_as_slice();
    let usable = |index: usize| type_bits & (1 << index) != 0;
    let lazy =
        |flags: vk::MemoryPropertyFlags| flags.contains(vk::MemoryPropertyFlags::LAZILY_ALLOCATED);
    let device_local = |flags: vk::MemoryPropertyFlags| {
        flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL) && !lazy(flags)
    };

    types
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            attachment_only && usable(*index) && lazy(memory_type.property_flags)
        })
        .or_else(|| {
            types.iter().enumerate().find(|(index, memory_type)| {
                usable(*index) && device_local(memory_type.property_flags)
            })
        })
        .or_else(|| {
            types
                .iter()
                .enumerate()
                .find(|(index, memory_type)| usable(*index) && !lazy(memory_type.property_flags))
        })
        .map(|(index, _)| index as u32)
}

/// Create every member of one aliased slot: `ALIAS` images bound at offset
/// zero to a single allocation sized for the largest member.
///
/// Contents are undefined on entry to each member's live interval — the same
/// discard semantics the graph already models for fresh transients — so no
/// extra synchronization is needed beyond the compiled bootstrap transitions.
fn create_aliased_transient_textures(
    backend: &VulkanRenderer,
    members: &[GraphResourceDesc],
) -> Result<Vec<TransientTexture>, RenderGraphError> {
    let device = &backend.context.device;

    let mut images = Vec::with_capacity(members.len());
    for desc in members {
        let info = transient_image_create_info(desc).flags(vk::ImageCreateFlags::ALIAS);
        match unsafe { device.create_image(&info, None) } {
            Ok(image) => images.push(image),
            Err(_e) => {
                for image in &images {
                    unsafe { device.destroy_image(*image, None) };
                }
                return Err(RenderGraphError::AllocationFailed(0));
            }
        }
    }

    let requirements = images
        .iter()
        .map(|&image| unsafe { device.get_image_memory_requirements(image) })
        .collect::<Vec<_>>();
    let bytes = requirements
        .iter()
        .map(|requirements| requirements.size)
        .max()
        .unwrap_or(0);
    let type_bits = requirements.iter().fold(u32::MAX, |bits, requirements| {
        bits & requirements.memory_type_bits
    });

    let memory_properties = unsafe {
        backend
            .context
            .instance
            .get_physical_device_memory_properties(backend.context.physical_device)
    };
    let attachment_only = members.iter().all(is_attachment_only);
    let memory_type_index = select_slot_memory_type(&memory_properties, type_bits, attachment_only)
        .ok_or(RenderGraphError::AllocationFailed(bytes as usize))?;

    let memory = match unsafe {
        device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(bytes)
                .memory_type_index(memory_type_index),
            None,
        )
    } {
        Ok(memory) => memory,
        Err(_e) => {
            for image in &images {
                unsafe { device.destroy_image(*image, None) };
            }
            return Err(RenderGraphError::AllocationFailed(bytes as usize));
        }
    };

    let lazily_allocated = memory_properties.memory_types_as_slice()[memory_type_index as usize]
        .property_flags
        .contains(vk::MemoryPropertyFlags::LAZILY_ALLOCATED);
    let slot_memory = Rc::new(VkSlotMemory::new(
        backend.context.clone(),
        memory,
        bytes,
        attachment_only && lazily_allocated,
    ));

    let mut textures = Vec::with_capacity(members.len());
    for (desc, &image) in members.iter().zip(&images) {
        if let Err(e) = unsafe { device.bind_image_memory(image, slot_memory.memory(), 0) } {
            for unbound in &images[textures.len()..] {
                unsafe { device.destroy_image(*unbound, None) };
            }
            return Err(RenderGraphError::BackendError(format!(
                "Failed to bind aliased transient image: {e}"
            )));
        }

        let image_view = create_transient_image_view(backend, image, desc)?;
        let mut texture = TransientTexture::new(
            backend.context.clone(),
            image,
            None,
            image_view,
            desc.format.into(),
            vk::Extent2D {
                width: desc.width,
                height: desc.height,
            },
        );
        texture.set_slot_memory(slot_memory.clone());
        log::debug!(
            "Aliased '{}' into a {} KiB {} slot allocation",
            desc.name,
            texture.slot_memory().map(VkSlotMemory::bytes).unwrap_or(0) / 1024,
            if texture
                .slot_memory()
                .is_some_and(VkSlotMemory::lazily_allocated)
            {
                "lazily allocated"
            } else {
                "device-local"
            }
        );
        textures.push(texture);
    }

    Ok(textures)
}
