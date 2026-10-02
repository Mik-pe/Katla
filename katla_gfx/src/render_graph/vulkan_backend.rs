//! Vulkan backend for the render graph.
//!
//! Implements `RenderGraphBackend` for `VulkanRenderer`, providing
//! concrete transient texture creation, bindless management, and
//! frame indexing using Vulkan GPU resources.

use std::rc::Rc;

use super::backend::{NativeTransientAllocation, RenderGraphBackend, TransientSlotPolicy};
use super::error::RenderGraphError;
use super::resource::{
    BufferDesc, BufferMemoryPolicy, BufferUsages, GraphResourceDesc, GraphResourceType,
};
use super::transient_buffer::VulkanGraphBuffer;
use super::transient_texture::{TransientTexture, VkSlotMemory};
use crate::renderer::VulkanRenderer;
use ash::vk;
use ash::vk::Handle;

impl RenderGraphBackend for VulkanRenderer {
    type TransientTexture = TransientTexture;
    type ImageView = crate::sync::VkImageView;
    type TransientBuffer = VulkanGraphBuffer;

    fn create_transient_slot(
        &self,
        members: &[GraphResourceDesc],
        policy: TransientSlotPolicy,
    ) -> Result<Vec<Self::TransientTexture>, RenderGraphError> {
        if let [single] = members {
            return Ok(vec![create_standalone_transient_texture(
                self, single, policy,
            )?]);
        }
        create_aliased_transient_textures(self, members, policy)
    }

    fn transient_allocation_info(
        texture: &Self::TransientTexture,
    ) -> Option<NativeTransientAllocation> {
        use ash::vk::Handle;
        if let Some(memory) = texture.slot_memory() {
            Some(NativeTransientAllocation {
                identity: memory.memory().as_raw(),
                offset: 0,
                bytes: memory.bytes(),
                logical_bytes: memory.bytes(),
                strategy: if memory.lazily_allocated() {
                    "vulkan_lazy_alias"
                } else {
                    "vulkan_memory_alias"
                },
            })
        } else {
            texture
                .allocation
                .as_ref()
                .map(|allocation| NativeTransientAllocation {
                    identity: unsafe { allocation.memory() }.as_raw(),
                    offset: allocation.offset(),
                    bytes: allocation.size(),
                    logical_bytes: allocation.size(),
                    strategy: "vulkan_standalone",
                })
        }
    }

    fn create_transient_buffer(
        &self,
        desc: BufferDesc,
    ) -> Result<Self::TransientBuffer, RenderGraphError> {
        let usage = vk_buffer_usages(desc.usages);

        let info = vk::BufferCreateInfo::default()
            .size(desc.size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let memory = match desc.memory {
            BufferMemoryPolicy::DeviceLocal => gpu_allocator::MemoryLocation::GpuOnly,
            BufferMemoryPolicy::CpuVisible => gpu_allocator::MemoryLocation::CpuToGpu,
            BufferMemoryPolicy::Readback => gpu_allocator::MemoryLocation::GpuToCpu,
        };
        let (buffer, allocation) = self
            .context
            .allocate_buffer_named(&info, memory, "Render Graph Buffer")
            .map_err(|error| RenderGraphError::BackendError(error.to_string()))?;
        Ok(VulkanGraphBuffer::new(
            self.context.clone(),
            buffer,
            allocation,
            desc,
        ))
    }

    fn destroy_transient_texture(texture: Self::TransientTexture) {
        drop(texture);
    }

    fn destroy_transient_buffer(buffer: Self::TransientBuffer) {
        drop(buffer);
    }

    fn transient_buffer_size(buffer: &Self::TransientBuffer) -> u64 {
        buffer.size()
    }

    fn buffer_desc(buffer: &Self::TransientBuffer) -> BufferDesc {
        buffer.desc
    }

    fn buffer_by_handle(
        &self,
        handle: crate::handle::BufferHandle,
    ) -> Option<&Self::TransientBuffer> {
        self.graph_buffers.get(handle)
    }

    fn builtin_buffer(&self, role: super::compute::BuiltinBuffer) -> Option<Self::TransientBuffer> {
        use super::compute::BuiltinBuffer::*;
        let (buffer, size) = match role {
            AnimationParams | AnimationClips | AnimationChannels | AnimationTimes
            | AnimationValues | AnimationJoints | AnimationWorld | AnimationOutput => {
                self.animation_buffers.as_ref()?.graph_buffer(role)?
            }
            Skeleton(handle) => {
                let skeleton = self.skeleton_buffers.get(handle)?;
                (self.skeleton_buffer_handle(handle)?, skeleton.size())
            }
            LightData | LightTiles | LightHeaders | LightFrame => {
                self.light_culling_buffers()?.graph_buffer(role)?
            }
            ParticleData
            | ParticleDeadList
            | ParticleAliveRead
            | ParticleAliveWrite
            | ParticleCounters
            | ParticlePreviousCounters
            | ParticleIndirect
            | ParticleFrame
            | ParticleEmitters => self
                .particle_system
                .as_ref()?
                .graph_buffer(role, self.current_frame())?,
        };
        let usage = match role {
            LightFrame | ParticleFrame => BufferUsages::UNIFORM,
            Skeleton(_) => BufferUsages::STORAGE | BufferUsages::TRANSFER_DESTINATION,
            ParticleIndirect => {
                BufferUsages::STORAGE
                    | BufferUsages::INDIRECT
                    | BufferUsages::TRANSFER_DESTINATION
                    | BufferUsages::TRANSFER_SOURCE
            }
            _ => {
                BufferUsages::STORAGE
                    | BufferUsages::TRANSFER_SOURCE
                    | BufferUsages::TRANSFER_DESTINATION
            }
        };
        Some(VulkanGraphBuffer::borrowed(
            self.context.clone(),
            buffer,
            match role {
                AnimationParams | AnimationWorld | AnimationOutput => {
                    self.animation_buffers.as_ref()?.graph_buffer_offset(role)
                }
                LightData | LightTiles | LightHeaders | LightFrame => {
                    self.light_culling_buffers()?.graph_buffer_offset(role)
                }
                _ => self
                    .particle_system
                    .as_ref()
                    .and_then(|system| system.graph_buffer_offset(role, self.current_frame()))
                    .unwrap_or(0),
            },
            BufferDesc::new(size, usage, BufferMemoryPolicy::DeviceLocal),
        ))
    }

    fn buffer_offset(buffer: &Self::TransientBuffer) -> u64 {
        buffer.offset
    }

    fn graph_buffer_previous_accesses(
        &self,
        buffer: &Self::TransientBuffer,
    ) -> Vec<super::BufferAccess> {
        self.context.graph_buffer_history.borrow().previous(
            buffer.buffer.as_raw(),
            buffer.offset,
            buffer.size(),
        )
    }

    fn record_graph_buffer_accesses(
        &self,
        buffer: &Self::TransientBuffer,
        accesses: &[super::BufferAccess],
    ) {
        self.context.graph_buffer_history.borrow_mut().record(
            buffer.buffer.as_raw(),
            buffer.offset,
            accesses,
        );
    }

    fn prepare_compute_pipeline(
        &mut self,
        descriptor: &super::compute::ComputePipelineDesc,
    ) -> Result<(), RenderGraphError> {
        if self.graph_compute_pipelines.contains_key(descriptor) {
            return Ok(());
        }
        let pipeline = super::vulkan_compute::VulkanGraphComputePipeline::new(
            self.context.clone(),
            descriptor,
        )?;
        self.graph_compute_pipelines
            .insert(descriptor.clone(), pipeline);
        Ok(())
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
        matches!(
            texture.format,
            vk::Format::D32_SFLOAT | vk::Format::D32_SFLOAT_S8_UINT | vk::Format::D24_UNORM_S8_UINT
        )
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

pub(crate) fn vk_buffer_usages(usages: BufferUsages) -> vk::BufferUsageFlags {
    let mut flags = vk::BufferUsageFlags::empty();
    if usages.contains(BufferUsages::UNIFORM) {
        flags |= vk::BufferUsageFlags::UNIFORM_BUFFER;
    }
    if usages.contains(BufferUsages::STORAGE) {
        flags |= vk::BufferUsageFlags::STORAGE_BUFFER;
    }
    if usages.contains(BufferUsages::VERTEX) {
        flags |= vk::BufferUsageFlags::VERTEX_BUFFER;
    }
    if usages.contains(BufferUsages::INDEX) {
        flags |= vk::BufferUsageFlags::INDEX_BUFFER;
    }
    if usages.contains(BufferUsages::INDIRECT) {
        flags |= vk::BufferUsageFlags::INDIRECT_BUFFER;
    }
    if usages.contains(BufferUsages::TRANSFER_SOURCE) {
        flags |= vk::BufferUsageFlags::TRANSFER_SRC;
    }
    if usages.contains(BufferUsages::TRANSFER_DESTINATION)
        || usages.contains(BufferUsages::READBACK)
    {
        flags |= vk::BufferUsageFlags::TRANSFER_DST;
    }
    flags
}

fn transient_image_usage(desc: &GraphResourceDesc, tile_local: bool) -> vk::ImageUsageFlags {
    if tile_local {
        return vk::ImageUsageFlags::TRANSIENT_ATTACHMENT
            | match desc.resource_type {
                GraphResourceType::DepthAttachment { .. } => {
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                }
                _ => vk::ImageUsageFlags::COLOR_ATTACHMENT,
            };
    }
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
            }
            usage
        }
        GraphResourceType::SampledImage => {
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST
        }
    }
}

fn transient_image_create_info(
    desc: &GraphResourceDesc,
    policy: TransientSlotPolicy,
) -> vk::ImageCreateInfo<'static> {
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
        .usage(
            transient_image_usage(desc, policy.memoryless)
                | if policy.storage {
                    vk::ImageUsageFlags::STORAGE
                } else {
                    vk::ImageUsageFlags::empty()
                }
                | if policy.transfer_destination {
                    vk::ImageUsageFlags::TRANSFER_DST
                } else {
                    vk::ImageUsageFlags::empty()
                },
        )
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
}

fn transient_aspect(desc: &GraphResourceDesc) -> vk::ImageAspectFlags {
    if matches!(
        desc.resource_type,
        GraphResourceType::DepthAttachment { .. }
    ) {
        if matches!(
            desc.format,
            crate::texture::ImageFormat::D32SfloatS8Uint
                | crate::texture::ImageFormat::D24UnormS8Uint
        ) {
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        } else {
            vk::ImageAspectFlags::DEPTH
        }
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
    policy: TransientSlotPolicy,
) -> Result<TransientTexture, RenderGraphError> {
    let (image, allocation) = backend
        .context
        .create_image(
            transient_image_create_info(desc, policy),
            gpu_allocator::MemoryLocation::GpuOnly,
        )
        .map_err(|_e| RenderGraphError::AllocationFailed(0))?;

    let image_view = match create_transient_image_view(backend, image, desc) {
        Ok(view) => view,
        Err(error) => {
            unsafe {
                backend.context.device.destroy_image(image, None);
            }
            backend
                .context
                .allocator
                .free(allocation, "failed transient texture");
            return Err(error);
        }
    };

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
    policy: TransientSlotPolicy,
) -> Result<Vec<TransientTexture>, RenderGraphError> {
    let device = &backend.context.device;

    let mut images = Vec::with_capacity(members.len());
    for desc in members {
        let info = transient_image_create_info(desc, policy).flags(vk::ImageCreateFlags::ALIAS);
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

    let mut requires_dedicated = false;
    let requirements = images
        .iter()
        .map(|&image| {
            let mut dedicated = vk::MemoryDedicatedRequirements::default();
            let mut requirements = vk::MemoryRequirements2::default().push_next(&mut dedicated);
            unsafe {
                device.get_image_memory_requirements2(
                    &vk::ImageMemoryRequirementsInfo2::default().image(image),
                    &mut requirements,
                );
            }
            let memory_requirements = requirements.memory_requirements;
            requires_dedicated |= dedicated.requires_dedicated_allocation == vk::TRUE;
            memory_requirements
        })
        .collect::<Vec<_>>();
    if requires_dedicated {
        for image in images {
            unsafe {
                device.destroy_image(image, None);
            }
        }
        return members
            .iter()
            .map(|desc| create_standalone_transient_texture(backend, desc, policy))
            .collect();
    }
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
    let attachment_only = policy.memoryless;
    let Some(memory_type_index) =
        select_slot_memory_type(&memory_properties, type_bits, attachment_only)
    else {
        for image in &images {
            unsafe {
                device.destroy_image(*image, None);
            }
        }
        return members
            .iter()
            .map(|desc| create_standalone_transient_texture(backend, desc, policy))
            .collect();
    };

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
        policy.frame_slot,
        policy.allocation_slot,
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

        let image_view = match create_transient_image_view(backend, image, desc) {
            Ok(view) => view,
            Err(error) => {
                for remaining in &images[textures.len()..] {
                    unsafe {
                        device.destroy_image(*remaining, None);
                    }
                }
                return Err(error);
            }
        };
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
            "Aliased '{}' into a {} KiB {} slot allocation (frame {}, slot {})",
            desc.name,
            texture.slot_memory().map(VkSlotMemory::bytes).unwrap_or(0) / 1024,
            if texture
                .slot_memory()
                .is_some_and(VkSlotMemory::lazily_allocated)
            {
                "lazily allocated"
            } else {
                "device-local"
            },
            slot_memory.frame_slot(),
            slot_memory.allocation_slot(),
        );
        textures.push(texture);
    }

    Ok(textures)
}
