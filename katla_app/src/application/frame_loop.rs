use log::{debug, info, warn};

use crate::application::Application;

impl Application {
    /// Cleanup resources on exit.
    /// Called both from exiting() and directly before event_loop.exit() for max_frames mode.
    pub(crate) fn cleanup_on_exit(&mut self) {
        if self.cleaned_up {
            return;
        }
        self.cleaned_up = true;

        // Run shutdown hook for game-side cleanup
        if let Some(hook) = self.on_shutdown.take() {
            hook(self);
        }

        // Wait for any pending async readback to complete before destroying resources
        // This must happen BEFORE wait_for_device() to ensure readback finishes
        match self.renderer.wait_for_pending_readback() {
            Ok(Some((frame, image_data))) => {
                info!("Saving final frame {} before shutdown", frame);
                let extent = self.renderer.swapchain_extent();
                let width = extent.width as usize;
                let height = extent.height as usize;
                if let Err(e) = self.save_frame_as_png(frame, &image_data, width, height) {
                    log::error!("Failed to save final frame {}: {}", frame, e);
                }
            }
            Ok(None) => {
                log::debug!("No pending readback to complete during shutdown");
            }
            Err(e) => {
                log::error!("Failed to wait for pending readback during shutdown: {}", e);
            }
        }

        // Save preferences before exit
        if let Err(e) = self.preferences.save() {
            warn!("Failed to save preferences: {}", e);
        } else {
            info!("Saved preferences to disk");
        }

        // Save GUI state before exit
        self.save_editor_state();

        // Wait for device to ensure all GPU operations are complete
        self.renderer.wait_for_device();

        // Cleanup frame graph transient textures BEFORE destroying renderer
        // This ensures proper cleanup order and avoids heap corruption during shutdown
        self.frame_graph.cleanup();

        // Destroy renderer (which owns the particle system)
        self.renderer.destroy();
    }

    /// Handle the RedrawRequested event — the main per-frame orchestration.
    pub(crate) fn handle_redraw_requested(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        if self.minimized {
            self.window.request_redraw();
            return;
        }

        debug!("RedrawRequested (frame {})", self.frame_count);
        self.timer.add_timestamp();
        let dt = self.timer.get_delta() as f32;

        // Update world (runs animation systems)
        debug!("Updating world...");
        self.world.update(dt);
        debug!("World updated");

        // Clear per-frame mouse delta after the tick.
        if let Some(input) = self.world.get_resource_mut::<crate::input::InputState>() {
            input.mouse_delta = (0.0, 0.0);
            input.mouse_wheel_delta = 0.0;
        }

        // Run per-frame update hook (after ECS systems, before rendering)
        if let Some(ref mut hook) = self.on_update {
            hook(&mut self.world, dt);
        }

        // Process ECS events to clean up GPU resources for destroyed entities
        crate::gpu_cleanup::process_gpu_cleanup_events(
            &self.world,
            &mut self.gpu_resource_tracker,
            &mut self.renderer,
        );

        // Update particle emitters from ECS components
        self.particle_system
            .update(&mut self.world, &mut self.renderer.particle_system, dt);

        // Update GPU animation: prepare data and upload per-frame params
        if let (Some(gpu_anim), Some(pipeline), Some(buffers)) = (
            &mut self.gpu_animation_system,
            &mut self.renderer.animation_pipeline,
            &mut self.renderer.animation_buffers,
        ) {
            gpu_anim.prepare(&mut self.world, pipeline, buffers);
            gpu_anim.update_params(&mut self.world, buffers);
            self.frame_graph
                .set_animation_skeleton_count(gpu_anim.skeleton_count() as u32);

            // Build per-entity skeleton copy commands:
            // (skeleton_handle_index, joint_offset, joint_count)
            use crate::components::DrawableComponent;
            let mut copy_cmds = Vec::new();
            for entity in gpu_anim.entities() {
                if let Some(drawable) = self.world.get_component::<DrawableComponent>(entity)
                    && let Some(info) = gpu_anim.entity_info(entity)
                {
                    copy_cmds.push((
                        drawable.skeleton_handle.index(),
                        info.joint_offset,
                        info.joint_count,
                    ));
                }
            }
            self.frame_graph.set_skeleton_copy_commands(copy_cmds);
        }

        // Poll background loader for completed asset loads
        self.poll_background_loader();

        // DEBUG: Test particle readback at frame 10
        #[cfg(debug_assertions)]
        {
            if self.frame_count == 10
                && let Some(ref particle_system) = self.renderer.particle_system
            {
                log::info!(
                    "=== Attempting Particle Debug Readback at frame {} ===",
                    self.frame_count
                );

                // Read debug data from staging buffers
                match particle_system.read_debug_data() {
                    Ok(debug_data) => {
                        log::info!("Particle Summary: {}", debug_data.summary());

                        // Print first 10 particles
                        log::info!("=== First 10 Particles ===");
                        for (i, p) in debug_data.particles.iter().take(10).enumerate() {
                            log::info!(
                                "Particle {}: pos=({:.2},{:.2},{:.2}) vel=({:.2},{:.2},{:.2}) lifetime={:.2} scale={:.3} color=({:.2},{:.2},{:.2},{:.2})",
                                i,
                                p.position[0],
                                p.position[1],
                                p.position[2],
                                p.velocity[0],
                                p.velocity[1],
                                p.velocity[2],
                                p.lifetime,
                                p.scale,
                                p.color[0],
                                p.color[1],
                                p.color[2],
                                p.color[3]
                            );
                        }

                        // Print alive particle indices
                        log::info!("=== First 10 Alive Particle Indices ===");
                        for (i, idx) in debug_data.alive_list.iter().take(10).enumerate() {
                            log::info!("Alive[{}] = {}", i, idx);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to read particle debug data: {}", e);
                    }
                }
            }
        }

        // Note: Transient textures are double-buffered (one per FRAMES_IN_FLIGHT).
        // The viewport bindless index must be updated BEFORE generating the UI
        // draw list so the UI samples from the correct per-frame texture.
        // Doing it after would cause an off-by-one mismatch: the UI would
        // sample from the previous frame's stale texture.
        self.render_editor_frame(dt);

        // Asynchronous black frame checking:
        // - On frame N: Queue async readback (non-blocking)
        // - On frame N+1: Check if readback from frame N is complete and save to disk
        // This allows us to catch synchronization issues that synchronous readback would mask
        if self.info.check_black_frames && self.frame_count > 0 {
            // Check if previous frame's async readback is complete
            match self.renderer.check_pending_readback() {
                Ok(Some((prev_frame, image_data))) => {
                    let extent = self.renderer.swapchain_extent();
                    let width = extent.width as usize;
                    let height = extent.height as usize;

                    // Save frame as PNG for visual inspection
                    if let Err(e) = self.save_frame_as_png(prev_frame, &image_data, width, height) {
                        log::error!("Failed to save frame {}: {}", prev_frame, e);
                    }

                    // Check 9 pixels in a 3x3 grid to detect if ANY pixel has color
                    let mut all_pixels_black = true;
                    let mut first_non_black_pixel = None;

                    // Sample positions: center, corners, and mid-edges
                    let sample_positions = [
                        (width / 2, height / 2),         // Center
                        (width / 4, height / 4),         // Top-left
                        (3 * width / 4, height / 4),     // Top-right
                        (width / 4, 3 * height / 4),     // Bottom-left
                        (3 * width / 4, 3 * height / 4), // Bottom-right
                        (width / 2, height / 4),         // Top-middle
                        (width / 2, 3 * height / 4),     // Bottom-middle
                        (width / 4, height / 2),         // Middle-left
                        (3 * width / 4, height / 2),     // Middle-right
                    ];

                    for (i, (x, y)) in sample_positions.iter().enumerate() {
                        let pixel_offset = (y * width + x) * 4;

                        if pixel_offset + 3 < image_data.len() {
                            let r = image_data[pixel_offset];
                            let g = image_data[pixel_offset + 1];
                            let b = image_data[pixel_offset + 2];

                            // Check if pixel has any color (any channel >= 10)
                            if r >= 10 || g >= 10 || b >= 10 {
                                all_pixels_black = false;
                                if first_non_black_pixel.is_none() {
                                    first_non_black_pixel = Some((i, r, g, b, *x, *y));
                                }
                            }
                        }
                    }

                    if all_pixels_black {
                        log::error!(
                            "BLACK FRAME DETECTED at frame {}! All 9 sampled pixels are black",
                            prev_frame
                        );
                    } else if let Some((i, r, g, b, x, y)) = first_non_black_pixel {
                        log::info!(
                            "Frame {} has color! Sample #{} at ({},{}): RGB({},{},{})",
                            prev_frame,
                            i,
                            x,
                            y,
                            r,
                            g,
                            b
                        );
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    log::error!("Failed to check pending readback: {}", e);
                }
            }

            // Queue async readback for current frame (will be checked on next frame)
            if let Err(e) = self.renderer.queue_async_readback(self.frame_count) {
                log::error!(
                    "Frame {} - Failed to queue async readback: {}",
                    self.frame_count,
                    e
                );
            }
        }

        // Handle max_frames limit (after readback to ensure last frame's readback is queued)
        self.frame_count += 1;

        if let Some(max) = self.info.max_frames {
            if self.frame_count >= max {
                info!("Rendered {} frames, exiting", self.frame_count);
                // Call cleanup directly since exiting() may not be triggered
                self.cleanup_on_exit();
                event_loop.exit();
            }
        }

        self.window.request_redraw();
    }

    /// Save frame data as PNG file for visual inspection
    pub(crate) fn save_frame_as_png(
        &self,
        frame: usize,
        bgra_data: &[u8],
        width: usize,
        height: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        use std::path::PathBuf;

        // Create frames directory if it doesn't exist
        let frames_dir = PathBuf::from("frames");
        fs::create_dir_all(&frames_dir)?;

        // Save as PNG using the image library
        let filename = frames_dir.join(format!("frame_{:04}.png", frame));

        // Convert from BGRA (swapchain format) to RGBA (PNG format)
        // The swapchain uses B8G8R8A8_SRGB format, so we need to swap channels
        // IMPORTANT: Force alpha to 255 (fully opaque) since swapchain is OPAQUE
        let rgba_data: Vec<u8> = bgra_data
            .chunks_exact(4)
            .flat_map(|bgra| {
                // BGRA -> RGBA conversion, force alpha to 255
                [bgra[2], bgra[1], bgra[0], 255]
            })
            .collect();

        // Create RGBA image buffer from the converted data
        let img: image::RgbaImage =
            image::ImageBuffer::from_raw(width as u32, height as u32, rgba_data)
                .ok_or("Failed to create image buffer from raw data")?;

        // Save to file (image crate will handle sRGB properly based on the ColorType)
        img.save(&filename)?;

        info!(
            "Saved frame {} to {:?} ({}x{} pixels, converted from BGRA_sRGB to RGBA, alpha forced to 255)",
            frame, filename, width, height
        );
        Ok(())
    }
}
