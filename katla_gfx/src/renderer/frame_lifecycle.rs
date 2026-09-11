use super::*;

impl VulkanRenderer {
    /// Wait for the current frame's previous GPU submission to complete.
    ///
    /// This must be called before any CPU writes to per-frame resources
    /// (storage buffers, uniforms, etc.) to prevent data races where the CPU
    /// overwrites data that the GPU is still reading from a prior submission.
    ///
    /// The recommended frame order is:
    /// 1. `wait_for_frame()` - ensures GPU is done with this frame slot
    /// 2. `set_frame_uniforms()` - writes frame data to storage buffer
    /// 3. `execute_draw_calls()` - writes per-object data to storage buffer
    /// 4. `render()` - submits GPU work
    pub fn wait_for_frame(&mut self) -> Result<(), crate::error::RendererError> {
        self.swap_data.wait_for_fence(&self.context.device)?;
        // This slot's previous submission completed, which retires every
        // resource replaced at least FRAMES_IN_FLIGHT frames ago. Bindless
        // slots freed here return to the free list only now, so no new
        // texture can resolve through a slot an older submission still
        // references.
        let expired_slots = self.retirements.drain_completed(
            self.swap_data.frame_counter(),
            self.swap_data.frames_in_flight(),
        );
        for slot in expired_slots {
            self.bindless_manager.release_texture_slot(slot);
        }
        // Release staged mesh uploads whose copy submissions finished.
        self.context.drain_completed_staged_uploads();
        Ok(())
    }

    /// Set frame-level uniforms for the current frame.
    ///
    /// This should be called once per frame before `render_frame()` or `execute_draw_calls()`.
    /// The uniforms are used by all draw calls in the frame.
    ///
    /// **Important:** `wait_for_frame()` must be called before this method to ensure
    /// the GPU is done reading from the frame's storage buffer. The recommended order is:
    /// 1. `wait_for_frame()` - ensures GPU is done with this frame slot
    /// 2. `set_frame_uniforms()` - writes frame data to storage buffer
    /// 3. `execute_draw_calls()` - writes per-object data to the same buffer
    /// 4. `render()` - renders using the prepared data
    ///
    /// # Arguments
    /// * `uniforms` - Frame uniforms containing view/proj matrices, camera position, and lighting
    pub fn set_frame_uniforms(&mut self, mut uniforms: FrameUniforms) {
        // Get frame index from swap_data (the source of truth for frame advancement)
        let frame_idx = self.swap_data.current_frame();

        // Inject depth texture bindless index into light_intensity.y for screen-space effects
        if let Some(depth_base) = self.depth_texture_base_index {
            uniforms.light_intensity = [
                uniforms.light_intensity[0],
                (depth_base + frame_idx as u32) as f32,
                uniforms.light_intensity[2],
                uniforms.light_intensity[3],
            ];
        }

        // Write frame uniforms to storage buffer for current frame
        self.storage_manager
            .update_from_frame_uniforms(frame_idx, &uniforms);

        // Store for reference
        self.frame_uniforms = uniforms;
    }

    /// Get the current frame uniforms (view/proj matrices, camera, lighting).
    pub fn frame_uniforms(&self) -> &FrameUniforms {
        &self.frame_uniforms
    }

    /// Execute draw calls from FrameContext and prepare them for rendering.
    ///
    /// This method writes all per-object data from draw calls to the storage buffer.
    /// Every instance of an instanced draw is uploaded to its own object slot.
    /// Frame uniforms should be set separately via `set_frame_uniforms()`.
    ///
    /// # Arguments
    /// * `draw_list` - The DrawList containing draw calls with allocated object slots
    ///
    /// # Errors
    ///
    /// Returns `RendererError::ObjectLimitExceeded` if any draw call's object slot
    /// range crosses `MAX_OBJECTS_PER_FRAME`.
    ///
    /// # Example
    /// ```ignore
    /// // In application render loop
    /// let mut frame = FrameContext::new();
    /// frame.set_camera(&view, &proj);
    /// frame.draw(mesh, material)
    ///     .with_transform(transform)
    ///     .submit();
    ///
    /// // Set frame uniforms
    /// renderer.set_frame_uniforms(&frame.frame_uniforms().unwrap());
    ///
    /// // Execute draw calls (writes to storage buffer)
    /// renderer.execute_draw_calls(&frame.draw_list())?;
    ///
    /// // Render with frame graph
    /// renderer.render(&mut frame_graph, |frame| {
    ///     frame.submit(geometry_pass_id, &frame.draw_list());
    /// })?;
    /// ```
    pub fn execute_draw_calls(&mut self, draw_list: &DrawList) -> Result<(), RendererError> {
        // Get current frame index from swap_data (source of truth)
        let frame_idx = self.current_frame();

        // Write all per-object data to storage buffer
        for draw_call in &draw_list.draws {
            let base = draw_call.instance_index as usize;
            let count = draw_call.instance_count().max(1) as usize;

            // Bounds check with clear error message
            if base + count > MAX_OBJECTS_PER_FRAME as usize {
                return Err(RendererError::ObjectLimitExceeded {
                    index: base,
                    limit: MAX_OBJECTS_PER_FRAME as usize,
                });
            }

            // Material parameters and texture indices are shared by all
            // instances; handles resolve to slots (with per-role fallback)
            // here, right before the upload. Emission resolves to 0 for
            // NONE/stale handles, keeping the shader's no-emission sentinel.
            let emission_idx = self.resolve_emission_texture_slot(draw_call.emission) as f32;
            let texture_indices = self.resolve_material_texture_slots(draw_call.material);

            for (i, instance) in draw_call
                .instances
                .iter()
                .chain(std::iter::repeat(&InstanceData::default()))
                .take(count)
                .enumerate()
            {
                self.storage_manager.update_object_bindless(
                    frame_idx,
                    base + i,
                    &crate::vulkan::material::storage_uniform::ObjectBindlessParams {
                        model: &instance.model_matrix,
                        color: &instance.color,
                        metallic: instance.metallic,
                        roughness: instance.roughness,
                        ao: instance.ao,
                        emission_idx,
                        texture_indices,
                    },
                );
            }
        }
        Ok(())
    }

    /// Simple immediate mode draw - the happy path for basic rendering.
    ///
    /// This method combines three steps into one:
    /// 1. Sets frame uniforms (camera, lighting)
    /// 2. Writes draw call data to GPU storage buffer
    /// 3. Returns a DrawList for submission to render passes
    ///
    /// # Arguments
    /// * `uniforms` - Frame-level data (view/proj matrices, lighting)
    /// * `draw_calls` - Slice of DrawCall objects to render
    ///
    /// # Returns
    /// A DrawList that can be passed to `frame.submit()` in the render callback.
    ///
    /// # Example
    /// ```ignore
    /// // Setup
    /// let mesh = crate::primitives::create_cube(&mut renderer, [1.0, 1.0, 1.0]);
    /// let material = renderer.default_material();
    ///
    /// // Render loop
    /// let draw_list = renderer.draw(
    ///     &frame_uniforms,
    ///     &[DrawCall::new(mesh, material)
    ///         .with_transform(model_matrix)
    ///         .with_color([1.0, 0.0, 0.0, 1.0])]
    /// )?;
    ///
    /// renderer.render(&mut frame_graph, |frame| {
    ///     frame.submit(geometry_pass_id, &draw_list);
    /// })?;
    /// ```
    ///
    /// # Performance Note
    /// For complex scenes with >100 draw calls, use `DrawList` directly with
    /// `set_frame_uniforms()` + `execute_draw_calls()` for better control.
    pub fn draw(
        &mut self,
        uniforms: &FrameUniforms,
        draw_calls: &[DrawCall],
    ) -> Result<DrawList, RendererError> {
        // Set frame uniforms
        self.set_frame_uniforms(uniforms.clone());

        // Build draw list
        let mut draw_list = DrawList::new();
        for draw in draw_calls {
            draw_list.push(draw.clone());
        }

        // Write to storage buffer
        self.execute_draw_calls(&draw_list)?;

        Ok(draw_list)
    }
}
