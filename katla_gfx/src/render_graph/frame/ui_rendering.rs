use crate::handle::PipelineHandle;
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::pass::PassDesc;
use crate::renderer::types::UIDrawList;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl<'a> Frame<'a> {
    /// Execute a UI draw list.
    pub(super) fn execute_ui_draw_list(
        &mut self,
        cmd: &CommandBuffer,
        pass: &PassDesc,
        ui_draw_list: &UIDrawList,
    ) -> Result<(), RenderGraphError> {
        if ui_draw_list.is_empty() {
            return Ok(());
        }

        let material_handle = pass.material.ok_or(RenderGraphError::InvalidConfiguration(
            "UI pass has no material specified. Use .material() on UIPass.".to_string(),
        ))?;

        let material = self
            .renderer
            .asset_registry
            .get_material(material_handle)
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;

        let pipeline_handle = material
            .pipeline
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;
        let (pipeline, pipeline_layout) = self
            .renderer
            .asset_registry
            .get_pipeline_handles(pipeline_handle)?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        let frame_idx = self.renderer.current_frame();
        let (vertex_buffer, index_buffer) =
            self.get_or_update_ui_buffers(frame_idx, ui_draw_list)?;

        cmd.bind_vertex_buffer(vertex_buffer.0, 0);
        cmd.bind_index_buffer(index_buffer, 0, vk::IndexType::UINT32);

        let extent = self.renderer.frame_context.swapchain.get_extent();

        if self.renderer.ui_renderer.font_atlas_handle().is_none() {
            return Err(RenderGraphError::InvalidConfiguration(
                "UI font atlas not initialized".to_string(),
            ));
        }

        // Bind UI descriptor sets (sampler, uniforms, bindless textures)
        // Use screen_size from draw list (logical pixels, matches vertex coordinates)
        // Bind set 0 once (sampler, uniforms don't change per frame)
        // Bind set 1 once (bindless texture array, shared with 3D materials)
        self.bind_ui_descriptor_sets(
            cmd,
            pipeline_handle,
            pipeline_layout,
            ui_draw_list.screen_size,
        )?;

        for draw_cmd in &ui_draw_list.commands {
            // clip_rect is in logical pixels, convert to physical pixels for Vulkan scissor
            if let Some([x, y, width, height]) = draw_cmd.clip_rect {
                let scale = ui_draw_list.scale_factor;
                let scissor = crate::sync::Rect2D::new(
                    (x * scale).max(0.0) as i32,
                    (y * scale).max(0.0) as i32,
                    (width * scale).max(0.0) as u32,
                    (height * scale).max(0.0) as u32,
                );
                cmd.set_scissor(&[scissor]);
            } else {
                cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
                    extent.width,
                    extent.height,
                )]);
            }

            // Draw indexed
            unsafe {
                self.renderer.context.device.cmd_draw_indexed(
                    cmd.vk_command_buffer(),
                    draw_cmd.index_count,
                    1,
                    draw_cmd.index_offset,
                    0,
                    0,
                );
            }
        }

        cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
            extent.width,
            extent.height,
        )]);

        Ok(())
    }

    /// Update per-frame UI vertex and index buffers with new data.
    ///
    /// This reuses buffers across frames to avoid memory leaks. Buffers are resized
    /// if needed to accommodate larger data.
    pub(super) fn get_or_update_ui_buffers(
        &mut self,
        frame_idx: usize,
        ui_draw_list: &UIDrawList,
    ) -> Result<((vk::Buffer, u32), vk::Buffer), RenderGraphError> {
        let vertex_bytes = bytemuck::cast_slice(&ui_draw_list.vertices);
        let index_bytes = bytemuck::cast_slice(&ui_draw_list.indices);

        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

        let vb = &mut ui_resources.vertex_buffers[frame_idx];
        vb.upload_data(vertex_bytes);
        let vb_handle = (vb.object(), vb.count());

        let ib = &mut ui_resources.index_buffers[frame_idx];
        ib.upload_data(index_bytes);
        let ib_handle = ib.object();

        Ok((vb_handle, ib_handle))
    }

    /// Bind UI descriptor sets (Set 0: font atlas, sampler, uniforms).
    pub(super) fn bind_ui_descriptor_sets(
        &mut self,
        cmd: &CommandBuffer,
        pipeline_handle: PipelineHandle,
        pipeline_layout: vk::PipelineLayout,
        screen_size: [f32; 2],
    ) -> Result<(), RenderGraphError> {
        // Get the pipeline to access its descriptor set layouts (separate borrow to avoid conflicts)
        let descriptor_set_layout = {
            let pipeline = self
                .renderer
                .asset_registry
                .get_pipeline(pipeline_handle)
                .ok_or(RenderGraphError::InvalidPipelineHandle(pipeline_handle))?;

            let descriptor_set_layouts = pipeline.descriptor_set_layouts();
            if descriptor_set_layouts.is_empty() {
                return Err(RenderGraphError::InvalidConfiguration(
                    "UI pipeline has no descriptor set layouts".to_string(),
                ));
            }

            descriptor_set_layouts[0]
        };

        // Now we can mutate the renderer state
        let frame_idx = self.renderer.current_frame();
        let descriptor_set =
            self.get_or_create_ui_descriptor_set(frame_idx, descriptor_set_layout, screen_size)?;

        // Bind descriptor set 0 (sampler, uniforms)
        cmd.bind_descriptor_sets(pipeline_layout, 0, &[descriptor_set], &[]);

        let bindless_descriptor_set = self.renderer.bindless_manager.descriptor_set();
        cmd.bind_descriptor_sets(pipeline_layout, 1, &[bindless_descriptor_set.vk()], &[]);

        Ok(())
    }

    /// Get or create per-frame UI descriptor set.
    pub(super) fn get_or_create_ui_descriptor_set(
        &mut self,
        frame_idx: usize,
        layout: vk::DescriptorSetLayout,
        screen_size: [f32; 2],
    ) -> Result<vk::DescriptorSet, RenderGraphError> {
        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

        // Ensure we have storage for this frame
        while ui_resources.descriptor_sets.len() <= frame_idx {
            ui_resources.descriptor_sets.push(None);
        }

        let descriptor_set_handle = ui_resources.descriptor_sets[frame_idx]
            .as_ref()
            .map(|ds| ds.vk());

        let _ = ui_resources; // Release borrow before calling update

        if let Some(ds_handle) = descriptor_set_handle {
            self.update_ui_descriptor_set(ds_handle, screen_size)?;
            return Ok(ds_handle);
        }

        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1),
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        let descriptor_pool = unsafe {
            self.renderer
                .context
                .device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Failed to create UI descriptor pool: {:?}",
                        e
                    ))
                })?
        };

        let layouts = [layout];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe {
            self.renderer
                .context
                .device
                .allocate_descriptor_sets(&allocate_info)
                .map_err(|e| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Failed to allocate UI descriptor set: {:?}",
                        e
                    ))
                })?
        };

        let descriptor_set = descriptor_sets[0];

        // Wrap in DescriptorSet for automatic cleanup (owns pool and layout)
        let descriptor_set_wrapper = crate::vulkan::descriptor_set::DescriptorSet::from_raw(
            descriptor_set,
            descriptor_pool,
            None, // Layout is owned by Pipeline, not by the descriptor set
            self.renderer.context.device.clone(),
        );

        // Store descriptor set (owns pool, automatic cleanup)
        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();
        if frame_idx < ui_resources.descriptor_sets.len() {
            ui_resources.descriptor_sets[frame_idx] = Some(descriptor_set_wrapper);
        }
        let _ = ui_resources;

        self.update_ui_descriptor_set(descriptor_set, screen_size)?;

        Ok(descriptor_set)
    }

    /// Update UI descriptor set with sampler and uniforms.
    pub(super) fn update_ui_descriptor_set(
        &mut self,
        descriptor_set: vk::DescriptorSet,
        screen_size: [f32; 2],
    ) -> Result<(), RenderGraphError> {
        let sampler = self.renderer.bindless_manager.ui_sampler();

        let uniform_data = [screen_size[0], screen_size[1], 0.0, 0.0];
        let uniform_bytes = bytemuck::cast_slice(&uniform_data);

        let uniform_buffer = {
            let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

            if ui_resources.uniform_buffer.is_none() {
                let uniform_buffer_info = vk::BufferCreateInfo::default()
                    .size(uniform_bytes.len() as vk::DeviceSize)
                    .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE);

                let (uniform_buffer, uniform_allocation) = self
                    .renderer
                    .context
                    .allocate_buffer(
                        &uniform_buffer_info,
                        gpu_allocator::MemoryLocation::CpuToGpu,
                    )
                    .map_err(|e| {
                        RenderGraphError::VulkanError(format!(
                            "Failed to allocate UI uniform buffer: {}",
                            e
                        ))
                    })?;
                ui_resources.uniform_buffer = Some((uniform_buffer, uniform_allocation));
            }

            // vk::Buffer is Copy, so this is fine to return from the borrow
            ui_resources
                .uniform_buffer
                .as_ref()
                .expect("UI uniform buffer should be allocated before rendering")
                .0
        };

        let uniform_ptr = {
            let allocation = &self
                .renderer
                .ui_renderer
                .ui_resources_mut()
                .uniform_buffer
                .as_ref()
                .expect("UI uniform buffer should be allocated before mapping")
                .1;
            self.renderer.context.map_buffer(allocation)?
        };

        unsafe {
            std::ptr::copy_nonoverlapping(uniform_bytes.as_ptr(), uniform_ptr, uniform_bytes.len());
        }

        let buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(uniform_buffer)
            .offset(0)
            .range(uniform_bytes.len() as vk::DeviceSize);

        let image_info = vk::DescriptorImageInfo::default()
            .sampler(sampler.vk())
            .image_view(vk::ImageView::null()) // Null for sampler-only write
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        let writes = [
            // Binding 1: sampler
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .image_info(std::slice::from_ref(&image_info)),
            // Binding 3: screen size uniform
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .buffer_info(std::slice::from_ref(&buffer_info)),
        ];

        unsafe {
            self.renderer
                .context
                .device
                .update_descriptor_sets(&writes, &[]);
        }

        Ok(())
    }
}
