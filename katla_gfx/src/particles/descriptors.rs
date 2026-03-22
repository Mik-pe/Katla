use std::rc::Rc;

use ash::vk;
use log::info;

use crate::vulkan::context::VulkanContext;

use super::{EmitterConfig, FrameData, GlobalParticleSystem, MAX_EMITTERS, buffer};

impl GlobalParticleSystem {
    /// Create descriptor set layouts for particle system.
    pub(crate) fn create_descriptor_layouts(
        &mut self,
        context: &Rc<VulkanContext>,
    ) -> Result<(), String> {
        // Compute layout (Set 0: static buffers only - particles, dead list, alive lists, counters, indirect draw)
        let compute_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];

        // Enable UPDATE_AFTER_BIND for all bindings to allow per-frame descriptor updates
        // without causing validation errors when command buffers are still pending
        let compute_binding_flags = [
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 0: particles
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 1: dead_list
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 2: alive_list (critical!)
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 3: alive write target
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 4: counters
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 5: indirect draw command
        ];

        let mut compute_binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
                .binding_flags(&compute_binding_flags);

        let compute_layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&compute_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut compute_binding_flags_info);

        let compute_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&compute_layout_info, None)
                .map_err(|e| format!("Failed to create compute descriptor layout: {:?}", e))?
        };

        self.compute_descriptor_layout = Some(compute_layout);

        // Compute push descriptor layout (Set 1: frame data + emitter configs)
        // NOTE: This uses PUSH_DESCRIPTOR bit to indicate these will be pushed via cmd_push_descriptor_set
        let compute_push_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];

        let compute_push_layout_create_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&compute_push_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR);

        let compute_push_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&compute_push_layout_create_info, None)
                .map_err(|e| format!("Failed to create compute push descriptor layout: {:?}", e))?
        };

        self.compute_push_descriptor_layout = Some(compute_push_layout);

        // Render layout (Set 0: particle data + alive lists)
        // We reuse the compute layout since both pipelines share the same descriptor set
        // The render shader uses binding 0 (particles) and binding 2 (alive list from simulate)
        // All 5 bindings must match the compute layout for descriptor set compatibility
        let render_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];

        // Enable UPDATE_AFTER_BIND for all bindings to allow per-frame descriptor updates
        let render_binding_flags = [
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 0
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 1
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 2
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 3
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 4
        ];

        let mut render_binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
                .binding_flags(&render_binding_flags);

        let render_layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&render_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut render_binding_flags_info);

        let render_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&render_layout_info, None)
                .map_err(|e| format!("Failed to create render descriptor layout: {:?}", e))?
        };

        self.render_descriptor_layout = Some(render_layout);

        // Render push descriptor layout (Set 1: frame data for graphics)
        // This is similar to compute push layout but with VERTEX/FRAGMENT stages
        let render_push_bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];

        let render_push_layout_create_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&render_push_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR);

        let render_push_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&render_push_layout_create_info, None)
                .map_err(|e| format!("Failed to create render push descriptor layout: {:?}", e))?
        };

        self.render_push_descriptor_layout = Some(render_push_layout);

        info!("Created particle system descriptor layouts");
        Ok(())
    }

    /// Create buffers for push descriptor updates (double-buffered for frames-in-flight).
    pub(crate) fn create_push_descriptor_buffers(
        &mut self,
        context: &Rc<VulkanContext>,
    ) -> Result<(), String> {
        for frame_idx in 0..2 {
            // Frame data buffer (uniform + storage, CPU-visible)
            let frame_data_size = std::mem::size_of::<FrameData>() as u64;
            let frame_buffer_info = vk::BufferCreateInfo::default()
                .size(frame_data_size)
                .usage(vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let frame_buffer = unsafe {
                context
                    .device
                    .create_buffer(&frame_buffer_info, None)
                    .map_err(|e| {
                        format!("Failed to create frame data buffer[{}]: {:?}", frame_idx, e)
                    })?
            };

            let frame_requirements =
                unsafe { context.device.get_buffer_memory_requirements(frame_buffer) };

            let frame_allocation = context
                .allocator
                .borrow_mut()
                .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                    name: &format!("particle_frame_data[{}]", frame_idx),
                    requirements: frame_requirements,
                    location: gpu_allocator::MemoryLocation::CpuToGpu,
                    linear: true,
                    allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
                })
                .map_err(|e| {
                    format!("Failed to allocate frame data memory[{}]: {}", frame_idx, e)
                })?;

            unsafe {
                context
                    .device
                    .bind_buffer_memory(
                        frame_buffer,
                        frame_allocation.memory(),
                        frame_allocation.offset(),
                    )
                    .map_err(|e| {
                        format!("Failed to bind frame data memory[{}]: {:?}", frame_idx, e)
                    })?
            }

            self.frame_data_buffers[frame_idx] = Some((frame_buffer, frame_allocation));

            // Emitter configs buffer (storage, CPU-visible)
            let emitter_size =
                (MAX_EMITTERS as usize * std::mem::size_of::<EmitterConfig>()) as u64;
            let emitter_buffer_info = vk::BufferCreateInfo::default()
                .size(emitter_size)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let emitter_buffer = unsafe {
                context
                    .device
                    .create_buffer(&emitter_buffer_info, None)
                    .map_err(|e| {
                        format!(
                            "Failed to create emitter configs buffer[{}]: {:?}",
                            frame_idx, e
                        )
                    })?
            };

            let emitter_requirements = unsafe {
                context
                    .device
                    .get_buffer_memory_requirements(emitter_buffer)
            };

            let emitter_allocation = context
                .allocator
                .borrow_mut()
                .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                    name: &format!("particle_emitter_configs[{}]", frame_idx),
                    requirements: emitter_requirements,
                    location: gpu_allocator::MemoryLocation::CpuToGpu,
                    linear: true,
                    allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
                })
                .map_err(|e| {
                    format!(
                        "Failed to allocate emitter configs memory[{}]: {}",
                        frame_idx, e
                    )
                })?;

            unsafe {
                context
                    .device
                    .bind_buffer_memory(
                        emitter_buffer,
                        emitter_allocation.memory(),
                        emitter_allocation.offset(),
                    )
                    .map_err(|e| {
                        format!(
                            "Failed to bind emitter configs memory[{}]: {:?}",
                            frame_idx, e
                        )
                    })?
            }

            self.emitter_configs_buffers[frame_idx] = Some((emitter_buffer, emitter_allocation));
        }

        // Validate push descriptor buffer alignments
        let device_properties = unsafe {
            context
                .instance
                .get_physical_device_properties(context.physical_device)
        };

        let min_storage_buffer_offset_alignment =
            device_properties.limits.min_storage_buffer_offset_alignment;

        let frame_data_alignment = std::mem::size_of::<FrameData>() as u64;
        let emitter_config_alignment = std::mem::size_of::<EmitterConfig>() as u64;

        if frame_data_alignment < min_storage_buffer_offset_alignment {
            log::warn!(
                "FrameData size ({}) is smaller than min_storage_buffer_offset_alignment ({}), \
                 this may cause performance issues",
                frame_data_alignment,
                min_storage_buffer_offset_alignment
            );
        }

        if emitter_config_alignment < min_storage_buffer_offset_alignment {
            log::warn!(
                "EmitterConfig size ({}) is smaller than min_storage_buffer_offset_alignment ({}), \
                 this may cause performance issues",
                emitter_config_alignment,
                min_storage_buffer_offset_alignment
            );
        }

        info!("Created particle system push descriptor buffers (double-buffered)");
        Ok(())
    }

    /// Create descriptor pool and allocate static descriptor set (internal helper).
    ///
    /// # Arguments
    /// * `layout` - Descriptor set layout to use
    /// * `pool_name` - Name for logging/debugging
    /// * `validate_alignment` - Whether to validate descriptor offset alignment
    fn create_descriptor_set_internal(
        &mut self,
        layout: vk::DescriptorSetLayout,
        pool_name: &str,
        validate_alignment: bool,
        include_indirect_binding: bool,
    ) -> Result<(vk::DescriptorSet, vk::DescriptorPool), String> {
        // Create descriptor pool
        let descriptor_count = if include_indirect_binding { 6 } else { 5 };
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(descriptor_count)];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1)
            .flags(
                vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET
                    | vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND,
            );

        let descriptor_pool = unsafe {
            self.context
                .device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| format!("Failed to create {} descriptor pool: {:?}", pool_name, e))?
        };

        // Allocate descriptor set
        let set_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&layout));

        let descriptor_sets = unsafe {
            self.context
                .device
                .allocate_descriptor_sets(&set_info)
                .map_err(|e| format!("Failed to allocate {} descriptor sets: {:?}", pool_name, e))?
        };

        let descriptor_set = descriptor_sets[0];

        // Update descriptor set with buffer views
        let buf_layout = self.buffer.layout();

        // Calculate each region's size (use unaligned sizes for descriptor ranges)
        let particles_region_size =
            buf_layout.max_particles * std::mem::size_of::<buffer::ParticleData>() as u64;
        let dead_list_region_size = buf_layout.max_particles * std::mem::size_of::<u32>() as u64;
        let alive_list_region_size = buf_layout.alive_list_size;

        let particle_buffer_handle = self.buffer.particle_buffer();
        let counters_buffer_handle = self.buffer.counters_buffer(0);
        let frame_buffer_handle = self.frame_data_buffers[0].as_ref().map(|(b, _)| *b);

        log::info!(
            "Buffer handles - particle: {:?}, counters: {:?}, frame_data: {:?}",
            particle_buffer_handle,
            counters_buffer_handle,
            frame_buffer_handle
        );

        let particle_buffer_info = [vk::DescriptorBufferInfo {
            buffer: particle_buffer_handle,
            offset: 0,
            range: particles_region_size, // Use actual particle region size
        }];

        log::info!(
            "Creating descriptor with particle buffer range: {} bytes",
            particles_region_size
        );

        let dead_list_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: buf_layout.dead_list_offset,
            range: dead_list_region_size, // Use actual dead list size, not aligned
        }];

        // Binding 2: alive_list (read by emit/simulate)
        // Maps to alive[0] initially, updated per-frame via update_compute_descriptor_binding
        let alive_list_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: buf_layout.alive_offset,
            range: alive_list_region_size,
        }];

        // Binding 3: alive_list_next (written by simulate)
        // Maps to alive[1] initially, updated per-frame via update_compute_descriptor_binding
        let alive_list_next_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: buf_layout.alive_frame_offset[1],
            range: alive_list_region_size,
        }];

        let counters_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.counters_buffer(0),
            offset: 0,
            range: std::mem::size_of::<buffer::ParticleCounters>() as u64,
        }];

        // Binding 5: indirect draw command buffer (16 bytes)
        // Written by simulate shader, read by vkCmdDrawIndirect.
        // Only included for compute descriptor set.
        let indirect_draw_info = if include_indirect_binding {
            Some([vk::DescriptorBufferInfo {
                buffer: self.buffer.indirect_draw_buffer(0),
                offset: 0,
                range: 16,
            }])
        } else {
            None
        };

        let mut descriptor_writes = vec![
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&particle_buffer_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&dead_list_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&alive_list_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&alive_list_next_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&counters_info),
        ];

        if include_indirect_binding && let Some(info) = &indirect_draw_info {
            descriptor_writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(5)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(info),
            );
        }

        log::info!(
            "Updating descriptor set {:?}: binding 0 range={} bytes",
            descriptor_set,
            particle_buffer_info[0].range
        );
        log::info!(
            "Updating descriptor set {:?}: binding 1 range={} bytes",
            descriptor_set,
            dead_list_info[0].range
        );

        unsafe {
            self.context
                .device
                .update_descriptor_sets(&descriptor_writes, &[]);
        }

        // Validate descriptor set offsets for alignment (optional)
        if validate_alignment {
            let device_properties = unsafe {
                self.context
                    .instance
                    .get_physical_device_properties(self.context.physical_device)
            };

            let min_storage_buffer_offset_alignment =
                device_properties.limits.min_storage_buffer_offset_alignment;

            // Validate that all descriptor buffer offsets are properly aligned
            let binding_offsets = [
                (0, 0u64),                             // particle data
                (1, buf_layout.dead_list_offset),      // dead list
                (2, buf_layout.alive_offset),          // alive[0]
                (3, buf_layout.alive_frame_offset[1]), // alive[1]
            ];

            for (binding, offset) in binding_offsets.iter() {
                if offset % min_storage_buffer_offset_alignment != 0 {
                    return Err(format!(
                        "Descriptor set binding {} offset {} is not aligned to min_storage_buffer_offset_alignment ({})",
                        binding, offset, min_storage_buffer_offset_alignment
                    ));
                }
            }
        }

        info!(
            "Created and allocated particle {} descriptor set",
            pool_name
        );
        Ok((descriptor_set, descriptor_pool))
    }

    /// Create descriptor pool and allocate static descriptor set for compute (Set 0).
    pub(super) fn create_compute_descriptor_set(
        &mut self,
    ) -> Result<(vk::DescriptorSet, vk::DescriptorPool), String> {
        let compute_layout = self
            .compute_descriptor_layout
            .ok_or("Compute descriptor layout not created")?;

        self.create_descriptor_set_internal(compute_layout, "compute", true, true)
    }

    /// Create descriptor pool and allocate static descriptor set for render (Set 0).
    /// Uses VERTEX/FRAGMENT stage flags instead of COMPUTE for graphics pipeline compatibility.
    pub(super) fn create_render_descriptor_set(
        &mut self,
    ) -> Result<(vk::DescriptorSet, vk::DescriptorPool), String> {
        let render_layout = self
            .render_descriptor_layout
            .ok_or("Render descriptor layout not created")?;

        self.create_descriptor_set_internal(render_layout, "render", false, false)
    }
}
