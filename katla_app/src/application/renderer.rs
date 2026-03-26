//! Frame rendering implementation.
//!
//! This module implements frame rendering using the FrameGraph API
//! and the new FrameContext for automatic instance allocation.

use super::Application;
use crate::rendering::FrameContext;
use katla_gfx::renderer::{FrameUniforms, UIDrawList};

impl Application {
    /// Render a single frame using the frame graph.
    ///
    /// Uses FrameContext for draw submission with automatic instance allocation.
    pub fn render_frame(
        &mut self,
        ui_draw_list: Option<UIDrawList>,
        delta_time: f32,
        frame_count: usize,
    ) {
        // Note: viewport bindless index is updated BEFORE generate_ui_draw_list()
        // in the RedrawRequested handler to ensure the UI samples from the
        // correct per-frame transient texture.

        let (viewport_width, viewport_height) = self.editor_ui.viewport_size();
        let viewport_aspect = if viewport_height > 0 {
            viewport_width as f32 / viewport_height as f32
        } else {
            16.0 / 9.0 // Fallback to default aspect ratio
        };
        self.camera
            .borrow_mut()
            .aspect_ratio_changed(&mut self.world, viewport_aspect);

        let mut frame = FrameContext::new();

        let camera = self.camera.borrow();
        let view_mat = camera.get_view_mat(&self.world);
        let proj_mat = camera.get_proj_mat(&self.world);
        let camera_entity = camera.entity;
        drop(camera);

        use crate::components::TransformComponent;
        let cam_pos = if let Some(transform) = self
            .world
            .get_component::<TransformComponent>(camera_entity)
        {
            [
                transform.transform.position.x(),
                transform.transform.position.y(),
                transform.transform.position.z(),
                1.0,
            ]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };

        let inv_view_proj = (proj_mat * view_mat).inverse();

        // Wait for the current frame's previous GPU submission to complete
        // before writing to per-frame storage buffers.
        self.renderer.wait_for_frame();

        // Tile grid dimensions for Forward+ light culling.
        // Must match the render target (swapchain) size, NOT the editor viewport panel size,
        // because clip_position in the fragment shader covers the full render target.
        let extent = self.renderer.swapchain_extent();
        let tiles_x = extent.width.div_ceil(16);
        let tiles_y = extent.height.div_ceil(16);

        let frame_uniforms = FrameUniforms {
            view_matrix: view_mat.to_array(),
            proj_matrix: proj_mat.to_array(),
            inv_view_proj_matrix: inv_view_proj.to_array(),
            camera_position: cam_pos,
            // Sunlight defaults
            light_direction: [0.3, 1.0, 0.2, 0.0],
            light_color: [1.0, 0.98, 0.95, 0.0],
            light_intensity: [
                1.0,
                self.renderer
                    .depth_texture_base_index()
                    .map(|base| base + self.renderer.current_frame() as u32)
                    .unwrap_or(0) as f32,
                0.0,
                0.0,
            ],
            tiles: [tiles_x, tiles_y, 0, 0],
        };
        frame.set_frame_uniforms(frame_uniforms.clone());

        // Collect draw calls from ECS world using FrameContext
        self.collect_draws_with_context(&mut frame);

        // Collect point lights for Forward+ culling
        self.collect_and_upload_lights();

        // Must be before update_shadows so CSM uses the current frame's view/proj matrices
        self.renderer
            .set_frame_uniforms(frame.frame_uniforms().clone());

        self.renderer.update_shadows([
            frame_uniforms.light_direction[0],
            frame_uniforms.light_direction[1],
            frame_uniforms.light_direction[2],
        ]);

        self.renderer.upload_shadow_cascades();

        if let Err(e) = self.renderer.execute_draw_calls(frame.draw_list()) {
            log::error!("Failed to execute draw calls: {}", e);
            return; // Skip rendering this frame
        }

        let draw_list = frame.take_draw_list();

        log::trace!(
            "About to submit {} draw calls to geometry pass",
            draw_list.len()
        );

        self.frame_graph.set_delta_time(delta_time);
        self.frame_graph.set_frame_count(frame_count);

        let frame_index = self.renderer.current_frame() as u32;
        if let Some(ref mut particle_system) = self.renderer.particle_system {
            match particle_system.update(delta_time, frame_index) {
                Ok((_max_alive, emit_count)) => {
                    let emit_workgroups = if emit_count > 0 {
                        emit_count.div_ceil(katla_gfx::particles::PARTICLE_EMIT_WORKGROUP_SIZE)
                    } else {
                        0
                    };

                    if emit_workgroups == 0 && emit_count > 0 {
                        log::warn!(
                            "Frame {}: emit_count={} but emit_workgroups=0! Particles won't be emitted!",
                            frame_count,
                            emit_count
                        );
                    }

                    // Simulate workgroups use a generous upper bound based on emitter configs:
                    //   sum(emit_rate_i * base_lifetime_i * (1 + lifetime_variation_i))
                    // No GPU readback needed — simulate shader self-bounds via counters.
                    // Over-dispatching is cheap (extra workgroups exit immediately).
                    let max_alive = particle_system.max_estimated_alive();
                    let total_particles_to_simulate = max_alive + emit_count;
                    let simulate_workgroups = if total_particles_to_simulate > 0 {
                        total_particles_to_simulate
                            .div_ceil(katla_gfx::particles::PARTICLE_SIMULATE_WORKGROUP_SIZE)
                    } else {
                        1 // ALWAYS run at least 1 workgroup for swap to happen
                    };

                    log::trace!(
                        "Particle compute workgroups: emit {} particles = {} workgroups, simulate ~{} max_alive + {} emit = {} total particles = {} workgroups",
                        emit_count,
                        emit_workgroups,
                        max_alive,
                        emit_count,
                        total_particles_to_simulate,
                        simulate_workgroups
                    );

                    // DEBUG: Record particle data readback at frame 10 (in debug builds)
                    #[cfg(debug_assertions)]
                    {
                        // Only trigger debug readback once to avoid WRITE_AFTER_WRITE hazards
                        // when writing to the same staging buffers multiple consecutive frames
                        if frame_count == 10 && !self.particle_readback_done {
                            if particle_system.has_debug_readback() {
                                // We'll record the copy during frame graph execution
                                // Just mark that we want to read back this frame
                                log::info!(
                                    "Frame {}: Triggering particle debug readback (once only)",
                                    frame_count
                                );

                                // Store a flag to trigger readback after frame execution
                                self.particle_readback_pending = true;
                                // Set flag in frame graph to record copy commands during execution
                                self.frame_graph.set_particle_debug_readback(true);
                            } else {
                                log::warn!(
                                    "Frame {}: Particle debug readback not initialized",
                                    frame_count
                                );
                            }
                        }
                    }

                    // Update frame graph with workgroup counts for this frame
                    self.frame_graph
                        .set_particle_emit_workgroup_count(emit_workgroups);
                    self.frame_graph
                        .set_particle_simulate_workgroup_count(simulate_workgroups);
                }
                Err(e) => {
                    log::error!("Failed to update particle system: {}", e);
                }
            }
        } else {
            log::warn!("⚠️ No particle system in renderer!");
        }

        self.renderer.render(&mut self.frame_graph, |frame| {
            log::trace!(
                "Inside render closure: submitting {} draw calls to geometry pass",
                draw_list.len()
            );
            if !draw_list.is_empty() {
                frame.submit("depth_prepass", &draw_list);
                frame.submit("geometry", &draw_list);
                frame.submit("shadow", &draw_list);
                log::trace!(
                    "Submitted {} draw calls to depth_prepass, geometry, and shadow passes",
                    draw_list.len()
                );
            } else {
                log::warn!("No draw calls to submit to geometry pass!");
            }

            if let Some(ref ui_list) = ui_draw_list {
                log::trace!("Submitting {} UI draw commands", ui_list.commands.len());
                frame.submit_ui("ui", ui_list);
            }
        });

        #[cfg(debug_assertions)]
        {
            if self.particle_readback_pending {
                self.particle_readback_pending = false;

                if let Some(ref mut particle_system) = self.renderer.particle_system {
                    match particle_system.read_debug_data() {
                        Ok(debug_data) => {
                            log::info!("=== PARTICLE DEBUG READBACK ===");
                            log::info!("{}", debug_data.summary());

                            // Print first 10 particles to see if they're moving
                            debug_data.print_particles(10);

                            // Check dead list initialization
                            log::info!("=== Checking dead list initialization ===");
                            debug_data.print_dead_indices(10);

                            // Mark readback as done so we don't trigger it again
                            self.particle_readback_done = true;
                            log::info!("Particle debug readback complete (will not trigger again)");

                            // Check specifically for the test particle at index 0
                            if !debug_data.particles.is_empty() {
                                let test_particle = &debug_data.particles[0];
                                log::info!(
                                    "TEST PARTICLE [0]: pos=({:.2},{:.2},{:.2}) vel=({:.2},{:.2},{:.2}) lifetime={:.2}",
                                    test_particle.position[0],
                                    test_particle.position[1],
                                    test_particle.position[2],
                                    test_particle.velocity[0],
                                    test_particle.velocity[1],
                                    test_particle.velocity[2],
                                    test_particle.lifetime
                                );
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to read particle debug data: {}", e);
                        }
                    }
                }
            }
        }
    }

    /// Collect point lights from the ECS world and upload to the GPU
    /// for Forward+ tile-based light culling.
    fn collect_and_upload_lights(&mut self) {
        use crate::components::{PointLight, TransformComponent};
        use katla_gfx::lighting::PointLightGPU;

        let mut lights = Vec::new();
        for (_entity, point_light, transform) in
            self.world.query::<(&PointLight, &TransformComponent)>()
        {
            let pos = transform.transform.position;
            lights.push(PointLightGPU {
                position: [pos.x(), pos.y(), pos.z()],
                range: point_light.range,
                color: point_light.color,
                intensity: point_light.intensity,
            });
        }

        if !lights.is_empty() {
            log::trace!(
                "Uploading {} point lights to GPU for Forward+ culling",
                lights.len()
            );
        }
        self.renderer.upload_lights(&lights);
    }

    /// Collect drawable components from the ECS world and submit to FrameContext.
    ///
    /// This automatically allocates instance indices and builds the draw list.
    /// Also populates entity_instance_map for GPU picking resolution.
    fn collect_draws_with_context(&mut self, frame: &mut FrameContext) {
        use crate::components::{DrawableComponent, TransformComponent};

        // Clear the entity-instance map for this frame
        self.entity_instance_map.clear();

        let entity_count = self.world.entity_ids().count();
        let mut drawable_count = 0;

        for entity_id in self.world.entity_ids() {
            let drawable = match self.world.get_component::<DrawableComponent>(entity_id) {
                Some(d) => d,
                None => continue,
            };
            let transform = match self.world.get_component::<TransformComponent>(entity_id) {
                Some(t) => t,
                None => continue,
            };

            let mesh_handle = drawable.mesh_handle;
            if mesh_handle.is_none() {
                continue;
            }

            let material_handle = drawable.material_handle;
            if material_handle.is_none() {
                continue;
            }

            if !drawable.skeleton_handle.is_none() {
                // Skeleton matrices are computed on the GPU via the animation
                // pose evaluation compute pass and copied to the per-entity
                // SkeletonBuffer. No CPU upload needed.
            }

            // Get instance_index before creating the draw builder (which borrows frame mutably).
            // instance_count() returns the next index that will be allocated by draw().
            let instance_index = frame.instance_count();

            let mut draw = frame
                .draw(mesh_handle, material_handle)
                .with_transform(transform.transform.make_mat4().to_array());

            // Skeleton for skinned meshes
            if !drawable.skeleton_handle.is_none() {
                draw = draw.with_skeleton(drawable.skeleton_handle);
            }

            if let Some(color) = drawable.color {
                draw = draw.with_color(color.to_array());
            }

            draw = draw.with_pbr(drawable.metallic, drawable.roughness, drawable.ao);

            if drawable.emission > 0.0 {
                draw = draw.with_emission(drawable.emission);
            }

            draw.submit();

            self.entity_instance_map.insert(instance_index, entity_id);

            drawable_count += 1;
        }

        log::trace!(
            "Submitted {} draw calls from {} entities",
            drawable_count,
            entity_count
        );
    }
}
