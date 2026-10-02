use ash::vk;

use super::Frame;
use crate::render_graph::{BufferDesc, BufferMemoryPolicy, BufferUsages, RenderGraphError};
use crate::renderer::VulkanRenderer;
use crate::renderer::frame_bindings::{PassBindings, SamplingMode};
use crate::renderer::graphics_interface::GraphicsBindingKind;
use crate::vulkan::commandbuffer::CommandBuffer;

impl Frame<'_, VulkanRenderer> {
    pub(super) fn bind_graphics_resources(
        &mut self,
        cmd: &CommandBuffer,
        material: crate::MaterialHandle,
        pipeline: crate::handle::PipelineHandle,
        packet: &PassBindings,
        skeleton: crate::SkeletonHandle,
        accesses: &[crate::render_graph::BufferAccess],
    ) -> Result<(), RenderGraphError> {
        let (_, layout) = self
            .renderer
            .asset_registry
            .get_pipeline_handles(pipeline)?;
        let descriptor = &self
            .renderer
            .asset_registry
            .get_material(material)
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration("Graphics material unavailable".into())
            })?
            .descriptor;
        let interface = self
            .renderer
            .material_compiler
            .interface(descriptor)
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration("Graphics interface unavailable".into())
            })?
            .clone();
        let slot = self.current_frame();
        let bindless_group = interface
            .bindings
            .iter()
            .any(|binding| binding.group == 1 && binding.array);
        let mut provided = Vec::new();
        for binding in &interface.bindings {
            if (binding.group == 0
                && binding.binding == 1
                && !packet
                    .buffers
                    .iter()
                    .any(|value| value.group == binding.group && value.binding == binding.binding)
                && !packet
                    .constants
                    .iter()
                    .any(|value| value.group == binding.group && value.binding == binding.binding))
                || (binding.group == 1 && bindless_group)
                || (matches!(binding.group, 2 | 3)
                    && binding.binding == 0
                    && !skeleton.is_none()
                    && !packet
                        .buffers
                        .iter()
                        .any(|value| value.group == binding.group && value.binding == 0))
            {
                provided.push(*binding);
            }
        }
        interface
            .validate_bindings(packet, &provided, |resource| {
                self.graph
                    .buffer_by_id(self.renderer, resource, slot)
                    .map(|buffer| buffer.size())
            })
            .map_err(RenderGraphError::InvalidConfiguration)?;
        interface
            .validate_buffer_accesses(packet, accesses)
            .map_err(RenderGraphError::InvalidConfiguration)?;
        let layouts = self
            .renderer
            .asset_registry
            .get_pipeline(pipeline)
            .ok_or_else(|| {
                RenderGraphError::PipelineNotSet("Graphics pipeline unavailable".into())
            })?
            .descriptor_set_layouts();
        let mut sets = std::collections::BTreeMap::new();
        for reflected in &interface.bindings {
            if reflected.group == 1 && bindless_group {
                continue;
            }
            if sets.contains_key(&reflected.group) {
                continue;
            }
            let mut sizes = std::collections::BTreeMap::<i32, u32>::new();
            for binding in interface
                .bindings
                .iter()
                .filter(|binding| binding.group == reflected.group)
            {
                let kind = match binding.kind {
                    GraphicsBindingKind::Buffer {
                        usage: crate::render_graph::BufferUsage::Uniform,
                        ..
                    } => vk::DescriptorType::UNIFORM_BUFFER,
                    GraphicsBindingKind::Buffer { .. } => vk::DescriptorType::STORAGE_BUFFER,
                    GraphicsBindingKind::Image { storage: false } => {
                        vk::DescriptorType::SAMPLED_IMAGE
                    }
                    GraphicsBindingKind::Image { storage: true } => {
                        vk::DescriptorType::STORAGE_IMAGE
                    }
                    GraphicsBindingKind::Sampler { .. } => vk::DescriptorType::SAMPLER,
                };
                *sizes.entry(kind.as_raw()).or_default() += 1;
            }
            let sizes: Vec<_> = sizes
                .into_iter()
                .map(|(kind, count)| {
                    vk::DescriptorPoolSize::default()
                        .ty(vk::DescriptorType::from_raw(kind))
                        .descriptor_count(count)
                })
                .collect();
            let set = self.renderer.graphics_descriptors[slot]
                .allocate(layouts[reflected.group as usize], &sizes)?;
            sets.insert(reflected.group, set);
        }
        let mut bound_bindless = false;
        for reflected in &interface.bindings {
            if reflected.group == 1 && bindless_group {
                if !bound_bindless {
                    cmd.bind_descriptor_sets(
                        layout,
                        1,
                        &[self.renderer.bindless_manager.descriptor_set().vk()],
                        &[],
                    );
                    self.capture_binding_set("graphics:bindless:texture_array_and_sampler".into());
                    bound_bindless = true;
                }
                continue;
            }
            match reflected.kind {
                GraphicsBindingKind::Buffer {
                    usage,
                    minimum_bytes,
                    ..
                } => {
                    let info = if let Some(constant) = packet.constants.iter().find(|value| {
                        value.group == reflected.group && value.binding == reflected.binding
                    }) {
                        let usages = if usage == crate::render_graph::BufferUsage::Uniform {
                            BufferUsages::UNIFORM
                        } else {
                            BufferUsages::STORAGE
                        };
                        let desc = BufferDesc::new(
                            constant.bytes.len() as u64,
                            usages,
                            BufferMemoryPolicy::CpuVisible,
                        );
                        let buffer = <VulkanRenderer as crate::render_graph::RenderGraphBackend>::create_transient_buffer(self.renderer, desc)?;
                        buffer
                            .write(0, &constant.bytes)
                            .map_err(|error| RenderGraphError::BackendError(error.to_string()))?;
                        let info = vk::DescriptorBufferInfo::default()
                            .buffer(buffer.vk_buffer())
                            .range(desc.size);
                        self.renderer.graphics_constants[slot].push(buffer);
                        info
                    } else if let Some(binding) = packet.buffers.iter().find(|value| {
                        value.group == reflected.group && value.binding == reflected.binding
                    }) {
                        let buffer = self
                            .graph
                            .buffer_by_id(self.renderer, binding.resource, slot)
                            .ok_or_else(|| {
                                RenderGraphError::ResourceNotFound(
                                    "Graphics buffer unavailable".into(),
                                )
                            })?;
                        let size = if binding.range.size == u64::MAX {
                            buffer.size() - binding.range.offset
                        } else {
                            binding.range.size
                        };
                        vk::DescriptorBufferInfo::default()
                            .buffer(buffer.vk_buffer())
                            .offset(buffer.offset + binding.range.offset)
                            .range(size)
                    } else if reflected.group == 0 && reflected.binding == 1 {
                        use crate::vulkan::material::storage_uniform::StorageUniformLayout;
                        let offset = StorageUniformLayout::OBJECT_ARRAY_OFFSET as u64;
                        let size = self.renderer.storage_manager.buffer_size() - offset;
                        vk::DescriptorBufferInfo::default()
                            .buffer(self.renderer.storage_manager.buffer(slot))
                            .offset(offset)
                            .range(size)
                    } else if matches!(reflected.group, 2 | 3)
                        && reflected.binding == 0
                        && !skeleton.is_none()
                    {
                        let handle = self
                            .renderer
                            .skeleton_buffers
                            .get(skeleton)
                            .and_then(|buffers| buffers.get(slot))
                            .ok_or(RenderGraphError::InvalidSkeletonHandle(skeleton))?;
                        let buffer = self
                            .renderer
                            .graph_buffers
                            .get(*handle)
                            .ok_or(RenderGraphError::InvalidSkeletonHandle(skeleton))?;
                        vk::DescriptorBufferInfo::default()
                            .buffer(buffer.vk_buffer())
                            .range(buffer.size())
                    } else {
                        return Err(RenderGraphError::InvalidConfiguration(
                            "Missing explicit graphics buffer".into(),
                        ));
                    };
                    use ash::vk::Handle;
                    self.renderer
                        .pending_graph_buffers
                        .insert(info.buffer.as_raw());
                    let limits = self.renderer.context.limits;
                    let alignment = if usage == crate::render_graph::BufferUsage::Uniform {
                        limits.min_uniform_buffer_offset_alignment
                    } else {
                        limits.min_storage_buffer_offset_alignment
                    }
                    .max(1);
                    if info.offset % alignment != 0 {
                        return Err(RenderGraphError::InvalidConfiguration(
                            "Graphics buffer offset violates native device alignment".into(),
                        ));
                    }
                    if let Some(binding) = packet.buffers.iter().find(|binding| {
                        binding.group == reflected.group && binding.binding == reflected.binding
                    }) {
                        self.capture_bound_resource(binding.resource);
                    }
                    if info.range < minimum_bytes {
                        return Err(RenderGraphError::InvalidConfiguration(
                            "Graphics buffer is smaller than its reflected block".into(),
                        ));
                    }
                    let infos = [info];
                    let writes = [vk::WriteDescriptorSet::default()
                        .dst_set(sets[&reflected.group])
                        .dst_binding(reflected.binding)
                        .descriptor_type(if usage == crate::render_graph::BufferUsage::Uniform {
                            vk::DescriptorType::UNIFORM_BUFFER
                        } else {
                            vk::DescriptorType::STORAGE_BUFFER
                        })
                        .buffer_info(&infos)];
                    unsafe {
                        self.renderer
                            .context
                            .device
                            .update_descriptor_sets(&writes, &[]);
                    }
                }
                GraphicsBindingKind::Image { storage } => {
                    let binding = packet
                        .images
                        .iter()
                        .find(|binding| {
                            binding.group == reflected.group && binding.binding == reflected.binding
                        })
                        .ok_or_else(|| {
                            RenderGraphError::InvalidConfiguration(
                                "Missing explicit graphics image".into(),
                            )
                        })?;
                    self.capture_bound_resource(binding.resource);
                    let view = self.graphics_image_view(binding)?;
                    let image_layout = if storage {
                        vk::ImageLayout::GENERAL
                    } else {
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                    };
                    let infos = [vk::DescriptorImageInfo::default()
                        .image_view(view)
                        .image_layout(image_layout)];
                    let writes = [vk::WriteDescriptorSet::default()
                        .dst_set(sets[&reflected.group])
                        .dst_binding(reflected.binding)
                        .descriptor_type(if storage {
                            vk::DescriptorType::STORAGE_IMAGE
                        } else {
                            vk::DescriptorType::SAMPLED_IMAGE
                        })
                        .image_info(&infos)];
                    unsafe {
                        self.renderer
                            .context
                            .device
                            .update_descriptor_sets(&writes, &[]);
                    }
                }
                GraphicsBindingKind::Sampler { .. } => {
                    let binding = packet
                        .samplers
                        .iter()
                        .find(|binding| {
                            binding.group == reflected.group && binding.binding == reflected.binding
                        })
                        .ok_or_else(|| {
                            RenderGraphError::InvalidConfiguration(
                                "Missing explicit graphics sampler".into(),
                            )
                        })?;
                    let sampler = self.graphics_sampler(binding.sampling)?;
                    let infos = [vk::DescriptorImageInfo::default().sampler(sampler)];
                    let writes = [vk::WriteDescriptorSet::default()
                        .dst_set(sets[&reflected.group])
                        .dst_binding(reflected.binding)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .image_info(&infos)];
                    unsafe {
                        self.renderer
                            .context
                            .device
                            .update_descriptor_sets(&writes, &[]);
                    }
                }
            }
        }
        for (group, set) in sets {
            cmd.bind_descriptor_sets(layout, group, &[set], &[]);
            self.capture_binding_set(format!(
                "graphics:group{group}:{:?}",
                interface
                    .bindings
                    .iter()
                    .filter(|binding| binding.group == group)
                    .collect::<Vec<_>>()
            ));
        }
        Ok(())
    }

    pub(super) fn graphics_sampler(
        &mut self,
        sampling: SamplingMode,
    ) -> Result<vk::Sampler, RenderGraphError> {
        if let Some(&sampler) = self.renderer.graphics_samplers.get(&sampling) {
            return Ok(sampler);
        }
        let filter = if sampling == SamplingMode::Nearest {
            vk::Filter::NEAREST
        } else {
            vk::Filter::LINEAR
        };
        let info = vk::SamplerCreateInfo::default()
            .min_filter(filter)
            .mag_filter(filter)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .compare_enable(sampling == SamplingMode::DepthComparison)
            .compare_op(vk::CompareOp::LESS_OR_EQUAL);
        let sampler = unsafe { self.renderer.context.device.create_sampler(&info, None) }.map_err(
            |error| RenderGraphError::BackendError(format!("Graphics sampler: {error}")),
        )?;
        self.renderer.graphics_samplers.insert(sampling, sampler);
        Ok(sampler)
    }
    fn graphics_image_view(
        &mut self,
        binding: &crate::renderer::frame_bindings::ImageBinding,
    ) -> Result<vk::ImageView, RenderGraphError> {
        use crate::render_graph::ImageAspects;
        let slot = self.current_frame();
        let (image, format) =
            if let Some(texture) = self.graph.transient_texture_by_id(binding.resource, slot) {
                (texture.image, texture.format)
            } else if let Some(texture) = self.imported_texture(binding.resource) {
                (texture.image().vk(), texture.format().into())
            } else if self.graph.resource_id(crate::render_graph::BACKBUFFER_NAME)
                == Some(binding.resource)
            {
                (
                    self.renderer.frame_context.swapchain_images[self.image_index as usize].vk(),
                    vk::Format::B8G8R8A8_SRGB,
                )
            } else {
                return Err(RenderGraphError::ResourceNotFound(
                    "Sampled image unavailable".into(),
                ));
            };
        let mut aspects = vk::ImageAspectFlags::empty();
        for (neutral, native) in [
            (ImageAspects::COLOR, vk::ImageAspectFlags::COLOR),
            (ImageAspects::DEPTH, vk::ImageAspectFlags::DEPTH),
            (ImageAspects::STENCIL, vk::ImageAspectFlags::STENCIL),
        ] {
            if binding.range.aspects.contains(neutral) {
                aspects |= native;
            }
        }
        let range = binding.range;
        if range.base_mip_level != 0
            || range.base_array_layer != 0
            || (range.mip_level_count != 1 && range.mip_level_count != u32::MAX)
            || (range.array_layer_count != 1 && range.array_layer_count != u32::MAX)
        {
            return Err(RenderGraphError::InvalidConfiguration(
                "Graphics image range exceeds its single mip/layer allocation".into(),
            ));
        }
        let info = vk::ImageViewCreateInfo::default()
            .image(image)
            .format(format)
            .view_type(vk::ImageViewType::TYPE_2D)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: aspects,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let view = unsafe { self.renderer.context.device.create_image_view(&info, None) }.map_err(
            |error| {
                RenderGraphError::BackendError(format!("Graphics image subresource view: {error}"))
            },
        )?;
        self.renderer.graphics_image_views[slot].push(view);
        Ok(view)
    }
}
