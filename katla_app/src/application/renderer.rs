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
        // Get frame-in-flight index for per-frame resource selection
        let frame_idx = self.renderer.current_frame();

        // Update viewport bindless index for this frame's LDR texture
        // With per-frame transient textures, the correct slot is base_slot + frame_idx
        if let Some(base_ldr_index) = self.frame_graph.get_ldr_texture_base_index() {
            let actual_ldr_index = base_ldr_index + frame_idx as u32;
            log::trace!(
                "Viewport bindless index: base={}, frame_idx={}, actual={}",
                base_ldr_index,
                frame_idx,
                actual_ldr_index
            );
            self.editor_ui.set_viewport_bindless_index(actual_ldr_index);
        }

        // Update camera aspect ratio based on viewport panel size
        let (viewport_width, viewport_height) = self.editor_ui.viewport_size();
        let viewport_aspect = if viewport_height > 0 {
            viewport_width as f32 / viewport_height as f32
        } else {
            16.0 / 9.0 // Fallback to default aspect ratio
        };
        self.camera
            .borrow_mut()
            .aspect_ratio_changed(&mut self.world, viewport_aspect);

        // Create frame context for this frame
        let mut frame = FrameContext::new();

        // Get camera view/projection matrices
        let camera = self.camera.borrow();
        let view_mat = camera.get_view_mat(&self.world);
        let proj_mat = camera.get_proj_mat(&self.world);
        let camera_entity = camera.entity;
        drop(camera); // Release transform borrow

        // Get camera position from transform component
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

        // Compute inverse view-projection for sky rendering
        let inv_view_proj = (proj_mat.clone() * view_mat.clone()).inverse();

        // Set frame uniforms (uses katla_gfx public type directly)
        let frame_uniforms = FrameUniforms {
            view_matrix: view_mat.to_array(),
            proj_matrix: proj_mat.to_array(),
            inv_view_proj_matrix: inv_view_proj.to_array(),
            camera_position: cam_pos,
            // Default lighting (sunlight)
            light_direction: [0.3, 1.0, 0.2, 0.0],
            light_color: [1.0, 0.98, 0.95, 0.0],
            light_intensity: 1.0,
        };
        frame.set_frame_uniforms(frame_uniforms);

        // Collect draw calls from ECS world using FrameContext
        self.collect_draws_with_context(&mut frame);

        // Apply frame uniforms to renderer
        self.renderer
            .set_frame_uniforms(frame.frame_uniforms().clone());

        // Execute draw calls (writes per-object data to storage buffer)
        if let Err(e) = self.renderer.execute_draw_calls(frame.draw_list()) {
            log::error!("Failed to execute draw calls: {}", e);
            return; // Skip rendering this frame
        }

        // Render using the frame graph
        let draw_list = frame.take_draw_list();

        log::debug!(
            "About to submit {} draw calls to geometry pass",
            draw_list.len()
        );

        // Set delta time and frame count for particle simulation
        self.frame_graph.set_delta_time(delta_time);
        self.frame_graph.set_frame_count(frame_count);

        // Update particle system simulation and calculate workgroup count
        log::info!(
            "Checking particle system: {}",
            self.renderer.particle_system.is_some()
        );
        if let Some(ref mut particle_system) = self.renderer.particle_system {
            log::info!("Calling particle system update...");
            match particle_system.update(delta_time) {
                Ok(alive_count) => {
                    log::info!(
                        "Particle system update result: {} alive particles",
                        alive_count
                    );
                    if alive_count > 0 {
                        log::info!(
                            "✅ Particle system updated: {} alive particles",
                            alive_count
                        );
                    }

                    // Calculate particles to emit this frame
                    let emitters = particle_system.get_emitters();
                    log::info!("🔍 Calculating emit_count: {} emitters, delta_time={}",
                        emitters.len(), delta_time);
                    for (i, config) in emitters.iter().enumerate() {
                        log::info!("  Emitter {}: emit_rate={}, particles_this_frame={}",
                            i, config.emit_rate, (config.emit_rate * delta_time) as u32);
                    }
                    let emit_count: u32 = emitters
                        .iter()
                        .map(|config| (config.emit_rate * delta_time) as u32)
                        .sum();

                    // Calculate emit workgroups (256 particles per workgroup)
                    let workgroup_size = katla_gfx::particles::PARTICLE_WORKGROUP_SIZE;
                    let emit_workgroups = if emit_count > 0 {
                        (emit_count + workgroup_size - 1) / workgroup_size
                    } else {
                        0 // No particles to emit
                    };

                    log::info!("🔍 Emit workgroups: emit_count={}, workgroups={}", emit_count, emit_workgroups);
                    if emit_workgroups == 0 {
                        log::warn!("⚠️ Emit workgroups is 0! Particles won't be emitted!");
                    }

                    // Calculate simulate workgroups (based on alive particles)
                    let simulate_workgroups = if alive_count > 0 {
                        (alive_count + workgroup_size - 1) / workgroup_size
                    } else {
                        0 // No particles to simulate
                    };

                    log::debug!(
                        "🎯 Particle compute workgroups: emit {} particles = {} workgroups, simulate {} particles = {} workgroups",
                        emit_count,
                        emit_workgroups,
                        alive_count,
                        simulate_workgroups
                    );

                    // Update frame graph with workgroup counts for this frame
                    self.frame_graph.set_particle_emit_workgroup_count(emit_workgroups);
                    self.frame_graph.set_particle_simulate_workgroup_count(simulate_workgroups);
                }
                Err(e) => {
                    log::error!("Failed to update particle system: {}", e);
                }
            }
        } else {
            log::warn!("⚠️ No particle system in renderer!");
        }

        self.renderer.render(&mut self.frame_graph, |frame| {
            // Submit draw list to the geometry pass
            log::debug!(
                "Inside render closure: submitting {} draw calls to geometry pass",
                draw_list.len()
            );
            if !draw_list.is_empty() {
                frame.submit("geometry", &draw_list);
                log::debug!(
                    "Submitted {} draw calls to geometry pass successfully",
                    draw_list.len()
                );
            } else {
                log::warn!("No draw calls to submit to geometry pass!");
            }

            // Submit UI draw list to the UI pass
            if let Some(ref ui_list) = ui_draw_list {
                log::debug!("Submitting {} UI draw commands", ui_list.commands.len());
                frame.submit_ui("ui", ui_list);
            }
        });
    }

    /// Collect drawable components from the ECS world and submit to FrameContext.
    ///
    /// This automatically allocates instance indices and builds the draw list.
    fn collect_draws_with_context(&mut self, frame: &mut FrameContext) {
        use crate::animation::Skeleton;
        use crate::components::{DrawableComponent, TransformComponent};

        let entity_count = self.world.entity_ids().count();
        let mut drawable_count = 0;

        // Collect drawable components
        for entity_id in self.world.entity_ids() {
            // Get drawable and transform components
            let drawable = match self.world.get_component::<DrawableComponent>(entity_id) {
                Some(d) => d,
                None => continue,
            };
            let transform = match self.world.get_component::<TransformComponent>(entity_id) {
                Some(t) => t,
                None => continue,
            };

            // Get mesh and material handles
            let mesh_handle = drawable.mesh_handle;
            if mesh_handle.is_none() {
                continue;
            }

            let material_handle = drawable.material_handle;
            if material_handle.is_none() {
                continue;
            }

            // Check for skeleton - upload if present
            if !drawable.skeleton_handle.is_none() {
                // Get Skeleton component and upload joint matrices
                if let Some(skeleton) = self.world.get_component::<Skeleton>(entity_id) {
                    // Convert Mat4 to [f32; 16] format for GPU
                    let matrices: Vec<[f32; 16]> = skeleton
                        .joint_transforms
                        .iter()
                        .map(|m| m.to_array())
                        .collect();

                    // Upload to GPU
                    self.renderer
                        .update_skeleton(drawable.skeleton_handle, &matrices);
                }
            }

            // Submit draw via FrameContext (instance allocation is automatic)
            let mut draw = frame
                .draw(mesh_handle, material_handle)
                .with_transform(transform.transform.make_mat4().to_array());

            // Add skeleton if present (for skinned meshes)
            if !drawable.skeleton_handle.is_none() {
                draw = draw.with_skeleton(drawable.skeleton_handle);
            }

            // Add color if present
            if let Some(color) = drawable.color {
                draw = draw.with_color(color.to_array());
            }

            // Add PBR material parameters
            draw = draw.with_pbr(drawable.metallic, drawable.roughness, drawable.ao);

            // Add emission texture index if present
            if drawable.emission > 0.0 {
                draw = draw.with_emission(drawable.emission);
            }

            draw.submit();

            drawable_count += 1;
        }

        log::debug!(
            "Submitted {} draw calls from {} entities",
            drawable_count,
            entity_count
        );
    }
}
