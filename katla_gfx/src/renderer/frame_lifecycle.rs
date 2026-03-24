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
    pub fn wait_for_frame(&mut self) {
        self.swap_data.wait_for_fence(&self.context.device);
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

    /// Execute draw calls from FrameContext and prepare them for rendering.
    ///
    /// This method writes all per-object data from draw calls to the storage buffer.
    /// Frame uniforms should be set separately via `set_frame_uniforms()`.
    ///
    /// # Arguments
    /// * `draw_list` - The DrawList from FrameContext containing draw calls with instance_index
    ///
    /// # Errors
    ///
    /// Returns `RendererError::ObjectLimitExceeded` if any draw call's `instance_index`
    /// exceeds `MAX_OBJECTS_PER_FRAME`.
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
    ///     frame.submit("geometry", &frame.draw_list());
    /// })?;
    /// ```
    pub fn execute_draw_calls(&mut self, draw_list: &DrawList) -> Result<(), RendererError> {
        // Get current frame index from swap_data (source of truth)
        let frame_idx = self.current_frame();

        // Write all per-object data to storage buffer
        for draw_call in &draw_list.draws {
            let index = draw_call.instance_index as usize;

            // Bounds check with clear error message
            if index >= MAX_OBJECTS_PER_FRAME as usize {
                return Err(RendererError::ObjectLimitExceeded {
                    index,
                    limit: MAX_OBJECTS_PER_FRAME as usize,
                });
            }

            // Extract material parameters
            let color = draw_call.color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let metallic = draw_call.metallic;
            let roughness = draw_call.roughness;
            let ao = draw_call.ao;
            let emission_idx = draw_call.emission;

            // Get texture indices from material
            // Default: [albedo=0, normal=1, metallic_roughness=2, ao=3]
            let texture_indices = self
                .asset_registry
                .get_material(draw_call.material)
                .map(|m| m.textures.texture_indices)
                .unwrap_or([0, 1, 2, 3]);

            // Write to storage buffer at instance_index
            self.storage_manager.update_object_bindless(
                frame_idx,
                index,
                &draw_call.model_matrix,
                &color,
                metallic,
                roughness,
                ao,
                emission_idx,
                texture_indices,
            );
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
    /// let mesh = renderer.create_cube_mesh([1.0, 1.0, 1.0]);
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
    ///     frame.submit("geometry", &draw_list);
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
