use crate::handle::PipelineHandle;
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::pass::PassDesc;
use crate::renderer::VulkanRenderer;
use crate::renderer::types::UIDrawList;
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
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

        // Resolve both pipelines: regular (for vertex-based commands) and
        // instanced (for instanced commands). The instanced pipeline uses
        // vs_instanced/fs_instanced entry points with UnitQuadVertex format.
        let regular_pipeline_handle = material
            .pipeline
            .ok_or(RenderGraphError::InvalidMaterialHandle(material_handle))?;
        let (regular_pipeline, pipeline_layout) = self
            .renderer
            .asset_registry
            .get_pipeline_handles(regular_pipeline_handle)?;

        let instanced_pipeline_handle = material
            .instanced_pipeline
            .unwrap_or(regular_pipeline_handle);
        let (instanced_pipeline, _) = self
            .renderer
            .asset_registry
            .get_pipeline_handles(instanced_pipeline_handle)?;

        let frame_idx = self.renderer.current_frame();
        let has_instances = !ui_draw_list.instances.is_empty();
        let has_vertices = !ui_draw_list.indices.is_empty();

        // Upload instance buffers (unit quad index buffer + per-instance data)
        // Instance data is bound as a storage buffer at descriptor binding 4,
        // not as a vertex buffer — the shader reads it via instance_data[] array.
        let instance_buffer: Option<vk::Buffer> = if has_instances {
            let (instance_vb, unit_quad_ib) =
                self.get_or_update_ui_instance_buffers(frame_idx, ui_draw_list)?;
            // Bind unit quad index buffer
            cmd.bind_index_buffer(unit_quad_ib, 0, vk::IndexType::UINT32);
            Some(instance_vb.0)
        } else {
            None
        };

        // Upload vertex/index buffers for complex geometry
        if has_vertices {
            let (vertex_buffer, index_buffer) =
                self.get_or_update_ui_buffers(frame_idx, ui_draw_list)?;

            let has_vertex_cmds = ui_draw_list.commands.iter().any(|c| !c.is_instanced);
            if has_vertex_cmds {
                cmd.bind_vertex_buffer(vertex_buffer.0, 0);
                cmd.bind_index_buffer(index_buffer, 0, vk::IndexType::UINT32);
            }
        }

        // For mixed mode, we need to bind both vertex and instance buffers
        // Bind unit quad vertex buffer at binding 0 for instanced draws
        if has_instances {
            let (unit_quad_vb, _) = self.get_or_update_ui_unit_quad(frame_idx)?;
            unsafe {
                self.renderer.context.device.cmd_bind_vertex_buffers(
                    cmd.vk_command_buffer(),
                    0, // binding 0 for unit quad
                    std::slice::from_ref(&unit_quad_vb),
                    &[0],
                );
            }
        } else if has_vertices {
            // Re-bind the vertex buffer at binding 0
            let (vertex_buffer, _) = self.get_or_update_ui_buffers(frame_idx, ui_draw_list)?;
            cmd.bind_vertex_buffer(vertex_buffer.0, 0);
        }

        let extent = self.renderer.frame_context.swapchain.get_extent();

        if self.renderer.ui_renderer.font_atlas_handle().is_none() {
            return Err(RenderGraphError::InvalidConfiguration(
                "UI font atlas not initialized".to_string(),
            ));
        }

        // Bind UI descriptor sets (sampler, uniforms, instance storage buffer, bindless textures)
        // Use screen_size from draw list (logical pixels, matches vertex coordinates)
        // Both pipelines share the same descriptor set layout, so binding once is sufficient.
        self.bind_ui_descriptor_sets(
            cmd,
            regular_pipeline_handle,
            pipeline_layout,
            ui_draw_list.screen_size,
            instance_buffer,
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

            if draw_cmd.is_instanced {
                // Bind instanced pipeline (vs_instanced/fs_instanced + UnitQuadVertex)
                unsafe {
                    self.renderer.context.device.cmd_bind_pipeline(
                        cmd.vk_command_buffer(),
                        vk::PipelineBindPoint::GRAPHICS,
                        instanced_pipeline,
                    );
                }
                // Re-bind unit quad vertex buffer at binding 0
                let (unit_quad_vb, _) = self.get_or_update_ui_unit_quad(frame_idx)?;
                unsafe {
                    self.renderer.context.device.cmd_bind_vertex_buffers(
                        cmd.vk_command_buffer(),
                        0,
                        std::slice::from_ref(&unit_quad_vb),
                        &[0],
                    );
                }

                // Instanced draw: unit quad + per-instance data
                unsafe {
                    self.renderer.context.device.cmd_draw_indexed(
                        cmd.vk_command_buffer(),
                        6,              // unit quad has 6 indices
                        draw_cmd.count, // instance count
                        0,              // first index (unit quad starts at 0)
                        0,
                        draw_cmd.offset, // first instance
                    );
                }
            } else {
                // Bind regular pipeline (vs_main/fs_main + UiVertex)
                unsafe {
                    self.renderer.context.device.cmd_bind_pipeline(
                        cmd.vk_command_buffer(),
                        vk::PipelineBindPoint::GRAPHICS,
                        regular_pipeline,
                    );
                }
                // Re-bind vertex buffer for non-instanced path (full UiVertex at binding 0)
                if has_vertices {
                    let (vertex_buffer, _) =
                        self.get_or_update_ui_buffers(frame_idx, ui_draw_list)?;
                    cmd.bind_vertex_buffer(vertex_buffer.0, 0);
                }

                // Update uniform buffer with per-command texture_index for non-instanced path
                let texture_index = self.renderer.get_texture_bindless_index(draw_cmd.texture);
                self.update_ui_uniform_texture_index(texture_index)?;

                // Vertex-based draw: complex geometry
                unsafe {
                    self.renderer.context.device.cmd_draw_indexed(
                        cmd.vk_command_buffer(),
                        draw_cmd.count,  // index count
                        1,               // instance count
                        draw_cmd.offset, // first index
                        0,
                        0,
                    );
                }
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

    /// Upload per-frame instance buffer and unit quad index buffer for instanced UI rendering.
    pub(super) fn get_or_update_ui_instance_buffers(
        &mut self,
        frame_idx: usize,
        ui_draw_list: &UIDrawList,
    ) -> Result<((vk::Buffer, u32), vk::Buffer), RenderGraphError> {
        let instance_bytes = bytemuck::cast_slice(&ui_draw_list.instances);
        let unit_quad_index_bytes = bytemuck::cast_slice(&crate::vertex::UNIT_QUAD_INDICES);

        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

        // Upload instance data
        let instance_ib = &mut ui_resources.instance_buffers[frame_idx];
        instance_ib.upload_data(instance_bytes);
        let instance_handle = (instance_ib.object(), instance_ib.count());

        // Upload unit quad index buffer (same every frame, but simple to re-upload)
        let quad_ib = &mut ui_resources.unit_quad_index_buffers[frame_idx];
        quad_ib.upload_data(unit_quad_index_bytes);
        let quad_ib_handle = quad_ib.object();

        Ok((instance_handle, quad_ib_handle))
    }

    /// Get or create the unit quad vertex buffer for instanced UI rendering.
    pub(super) fn get_or_update_ui_unit_quad(
        &mut self,
        frame_idx: usize,
    ) -> Result<(vk::Buffer, vk::Buffer), RenderGraphError> {
        let quad_vertex_bytes = bytemuck::cast_slice(&crate::vertex::UNIT_QUAD_VERTICES);
        let quad_index_bytes = bytemuck::cast_slice(&crate::vertex::UNIT_QUAD_INDICES);

        let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

        let quad_vb = &mut ui_resources.unit_quad_vertex_buffers[frame_idx];
        quad_vb.upload_data(quad_vertex_bytes);
        let quad_vb_handle = quad_vb.object();

        let quad_ib = &mut ui_resources.unit_quad_index_buffers[frame_idx];
        quad_ib.upload_data(quad_index_bytes);
        let quad_ib_handle = quad_ib.object();

        Ok((quad_vb_handle, quad_ib_handle))
    }

    /// Bind UI descriptor sets (Set 0: sampler, uniforms, instance buffer; Set 1: bindless textures).
    pub(super) fn bind_ui_descriptor_sets(
        &mut self,
        cmd: &CommandBuffer,
        pipeline_handle: PipelineHandle,
        pipeline_layout: vk::PipelineLayout,
        screen_size: [f32; 2],
        instance_buffer: Option<vk::Buffer>,
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
        let descriptor_set = self.get_or_create_ui_descriptor_set(
            frame_idx,
            descriptor_set_layout,
            screen_size,
            instance_buffer,
        )?;

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
        instance_buffer: Option<vk::Buffer>,
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
            self.update_ui_descriptor_set(ds_handle, screen_size, instance_buffer)?;
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
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
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

        self.update_ui_descriptor_set(descriptor_set, screen_size, instance_buffer)?;

        Ok(descriptor_set)
    }

    /// Update UI descriptor set with sampler, uniforms, and instance storage buffer.
    pub(super) fn update_ui_descriptor_set(
        &mut self,
        descriptor_set: vk::DescriptorSet,
        screen_size: [f32; 2],
        instance_buffer: Option<vk::Buffer>,
    ) -> Result<(), RenderGraphError> {
        let sampler = self.renderer.bindless_manager.ui_sampler();

        let uniform_data = [screen_size[0], screen_size[1], 1.0, 0.0];
        let uniform_bytes = bytemuck::cast_slice(&uniform_data);

        let uniform_buffer = {
            let ui_resources = self.renderer.ui_renderer.ui_resources_mut();

            ui_resources
                .uniform_buffer
                .as_ref()
                .expect("UI uniform buffer allocated in constructor")
                .0
        };

        let uniform_ptr = {
            let allocation = &self
                .renderer
                .ui_renderer
                .ui_resources_mut()
                .uniform_buffer
                .as_ref()
                .expect("UI uniform buffer allocated in constructor")
                .1;
            self.renderer.context.map_buffer(allocation)?
        };

        unsafe {
            std::ptr::copy_nonoverlapping(uniform_bytes.as_ptr(), uniform_ptr, uniform_bytes.len());
        }

        let uniform_buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(uniform_buffer)
            .offset(0)
            .range(uniform_bytes.len() as vk::DeviceSize);

        let image_info = vk::DescriptorImageInfo::default()
            .sampler(sampler.vk())
            .image_view(vk::ImageView::null()) // Null for sampler-only write
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        let mut writes = vec![
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
                .buffer_info(std::slice::from_ref(&uniform_buffer_info)),
        ];

        // Binding 4: instance data storage buffer (only when instances exist)
        let instance_buffer_info = instance_buffer.map(|buf| {
            vk::DescriptorBufferInfo::default()
                .buffer(buf)
                .offset(0)
                .range(vk::WHOLE_SIZE)
        });

        if let Some(ref info) = instance_buffer_info {
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(4)
                    .dst_array_element(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(std::slice::from_ref(info)),
            );
        }

        unsafe {
            self.renderer
                .context
                .device
                .update_descriptor_sets(&writes, &[]);
        }

        Ok(())
    }

    /// Update the texture_index field in the UI uniform buffer for non-instanced draws.
    ///
    /// The uniform layout is: [screen_size.x, screen_size.y, ndc_y_flip, texture_index].
    /// Only the texture_index (byte offset 12) is updated.
    fn update_ui_uniform_texture_index(
        &mut self,
        texture_index: u32,
    ) -> Result<(), RenderGraphError> {
        let allocation = &self
            .renderer
            .ui_renderer
            .ui_resources_mut()
            .uniform_buffer
            .as_ref()
            .expect("UI uniform buffer allocated in constructor")
            .1;
        let ptr = self.renderer.context.map_buffer(allocation)?;

        // texture_index is at byte offset 12 in the 16-byte uniform struct
        unsafe {
            let tex_idx_ptr = ptr.add(12) as *mut u32;
            *tex_idx_ptr = texture_index;
        }

        Ok(())
    }
}
